{
  inputs = {
    v_flakes.url = "github:valeratrades/v_flakes?ref=v1.6";
  };
  outputs = { self, v_flakes }:
    let
      inherit (v_flakes) flake-utils pre-commit-hooks;
    in
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import v_flakes.default_nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };
        # Canonical toolchain pinned in v_flakes — byte-identical across repos, so
        # the nix store dedups it and sccache cross-references compilations.
        rust = v_flakes.rs.default_nightly system;
        pre-commit-check = pre-commit-hooks.lib.${system}.run (v_flakes.files.preCommit { inherit pkgs; });
        manifest = (pkgs.lib.importTOML ./tilers/Cargo.toml).package;
        pname = manifest.name;
        stdenv = pkgs.stdenvAdapters.useMoldLinker pkgs.stdenv;

        rs = v_flakes.rs {
          inherit pkgs rust;
          build = {
            deny = false;
            workspace = let deprecate_by = "v1.0.0"; in {
              "./tilers_core/" = [ "git_version" "log_directives" { deprecate = { by_version = deprecate_by; force = true; }; } ];
            };
          };
        };
        github = v_flakes.github {
          inherit pkgs pname rs;
          enable = true;
          lastSupportedVersion = "nightly-2026-06-18";
          jobs.default = true;
          # The film is generated and committed, so nothing else notices when the generator moves
          # under it. Daily rather than per-run: it builds the workspace to redraw one SVG.
          jobs.other.augment = [{
            name = "asset-gate";
            args = {
              asset = "docs/.readme_assets/assets/fuzz.svg";
              command = "nix run .#film";
              everySeconds = 86400;
            };
          }];
        };
        readme = v_flakes.readme-fw {
          inherit pkgs pname;
          defaults = true;
          lastSupportedVersion = "nightly-1.92";
          rootDir = ./.;
          badges = [ "msrv" "crates_io" "docs_rs" "loc" "ci" ];
        };
        combined = v_flakes.utils.combine { inherit rust; modules = [ rs github readme ]; };

        # The crate graph pins wasm-bindgen =0.2.125, and the CLI must match it; nixpkgs carries another.
        wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
          src = pkgs.fetchCrate {
            pname = "wasm-bindgen-cli";
            version = "0.2.125";
            hash = "sha256-zRawtjxMOdTMX+mZaiNR3YYfTiZJhf9qj7kXSSeMxrc=";
          };
          cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
            inherit src;
            inherit (src) pname version;
            hash = "sha256-aZCfgR23Qb0Pn4Mm4ToMtuuRQqSJjXCR9li/VvP5CTM=";
          };
        };
        # Prod runs 12.3.1 and the plugin builds against its libraries; nixpkgs is a major ahead.
        grafana = pkgs.fetchzip {
          url = "https://dl.grafana.com/oss/release/grafana-12.3.1.linux-amd64.tar.gz";
          hash = "sha256-PKOxXRmqtLVneGsQO8phLXnacXaRRWfDQDPu3g9f6Lo=";
        };
        # The plugin against a local Grafana, provisioned with a testdata twin of devops' `ev-deployments`.
        # Rust changes need a restart; the TS side rebuilds on save.
        devGrafana = pkgs.writeShellApplication {
          name = "dev-grafana";
          runtimeInputs = [ rust pkgs.mold pkgs.pkg-config pkgs.openssl wasm-bindgen-cli pkgs.nodejs_24 ];
          text = ''
            root="$(git rev-parse --show-toplevel)"
            plugin="$root/tilers_grafana/plugin"
            state="$root/tmp/grafana"
            cargo b -p tilers_grafana --target wasm32-unknown-unknown
            wasm-bindgen --target web --out-dir "$plugin/src/wasm" "$root/target/wasm32-unknown-unknown/debug/tilers_grafana.wasm"
            [ -d "$plugin/node_modules" ] || npm --prefix "$plugin" ci
            rm -rf "$plugin/dist" # so the wait below sees this run's build
            mkdir -p "$state/plugins"
            ln -sfn "$plugin/dist" "$state/plugins/evmain-tilers-app"
            npm --prefix "$plugin" run dev &
            trap 'kill %1' EXIT
            # first build has to land before Grafana scans for plugins
            until [ -f "$plugin/dist/plugin.json" ]; do sleep 1; done
            # development: plugin files are read live, so watch rebuilds (new chunks) serve without a restart
            GF_DEFAULT_APP_MODE=development \
            GF_SERVER_HTTP_PORT=54581 \
            GF_PATHS_DATA="$state/data" \
            GF_PATHS_LOGS="$state/logs" \
            GF_PATHS_PLUGINS="$state/plugins" \
            GF_PATHS_PROVISIONING="$plugin/provisioning" \
            TL_FIXTURES="$plugin/provisioning/fixtures" \
            GF_AUTH_ANONYMOUS_ENABLED=true \
            GF_AUTH_ANONYMOUS_ORG_ROLE=Admin \
            GF_AUTH_DISABLE_LOGIN_FORM=true \
            GF_SECURITY_ALLOW_EMBEDDING=true \
            GF_PLUGINS_ALLOW_LOADING_UNSIGNED_PLUGINS=evmain-tilers-app \
            GF_NEWS_NEWS_FEED_ENABLED=false \
            GF_PLUGINS_PREINSTALL_DISABLED=true \
              ${grafana}/bin/grafana server --homepath ${grafana} "$@"
          '';
        };
      in
      {
        packages =
          let
            build_rust = v_flakes.rs.build_nightly system;
            rustc = build_rust;
            cargo = build_rust;
            rustPlatform = pkgs.makeRustPlatform {
              inherit rustc cargo stdenv;
            };
          in
          {
            default = rustPlatform.buildRustPackage {
              inherit pname;
              version = manifest.version;

              buildInputs = with pkgs; [
                openssl.dev
              ];
              nativeBuildInputs = with pkgs; [ pkg-config ];

              cargoLock.lockFile = ./Cargo.lock;
              src = pkgs.lib.cleanSource ./.;
              RUSTC_WRAPPER = ""; # `.cargo/config.toml`'s sccache isn't in the sandbox
            };
          };

        apps.help = v_flakes.utils.help { inherit pkgs self system; };

        # Extra args pass through to the underlying serve command, e.g. `nix run .#dev -- leptos --port 9000`.
        apps.dev = {
          type = "app";
          meta.description = "Serve an example: `nix run .#dev -- [dioxus|leptos|grafana]` (default dioxus)";
          program = pkgs.lib.getExe (pkgs.writeShellApplication {
            name = "dev";
            runtimeInputs = [ rust pkgs.mold pkgs.pkg-config pkgs.openssl wasm-bindgen-cli pkgs.nodejs_24 ];
            text = ''
              framework="''${1:-dioxus}"; shift 2>/dev/null || true
              case "$framework" in
                dioxus)
                  exec ${pkgs.dioxus-cli}/bin/dx serve --example insilico --package tilers_dioxus "$@"
                  ;;
                leptos)
                  cd tilers_leptos && exec ${pkgs.trunk}/bin/trunk serve "$@"
                  ;;
                grafana)
                  exec ${devGrafana}/bin/dev-grafana "$@"
                  ;;
                *)
                  echo "usage: nix run .#dev -- [dioxus|leptos|grafana] [extra serve args]" >&2
                  exit 1
                  ;;
              esac
            '';
          });
        };

        # The seed is pinned rather than left to the film's own best-of-scan: an asset a CI job
        # diffs has to be a function of the tree alone, and best-of-scan makes it a function of the scan too.
        apps.film = {
          type = "app";
          meta.description = "Redraw the README's fuzz film";
          program = pkgs.lib.getExe (pkgs.writeShellApplication {
            name = "film";
            runtimeInputs = with pkgs; [ rust git pkg-config openssl mold ];
            text = ''
              cd "$(git rev-parse --show-toplevel)"
              cargo run --example fuzz_film -p tilers_core -- \
                --seed 172 --out "''${ASSET_OUT:-''${1:-docs/.readme_assets/assets/fuzz.svg}}"
            '';
          });
        };

        devShells.default =
          with pkgs;
          mkShell {
            inherit stdenv;
            shellHook =
              pre-commit-check.shellHook
              + combined.shellHook
              + ''
                cp -f ${(v_flakes.files.treefmt) { inherit pkgs; }} ./.treefmt.toml
              '';

            packages = [
              mold
              openssl
              pkg-config
              rust
              # nixpkgs dioxus-cli vendors wasm-bindgen 0.2.118 but the crate graph pins =0.2.125;
              # dx uses the matching external binary if present: `cargo binstall wasm-bindgen-cli@0.2.125`.
              dioxus-cli
              # Serves the Leptos CSR example: `nix run .#dev -- leptos`.
              trunk
              # `tilers_grafana/plugin`, and its wasm glue
              nodejs_24
              wasm-bindgen-cli
            ] ++ pre-commit-check.enabledPackages ++ combined.enabledPackages;

            env.RUST_BACKTRACE = 1;
            env.RUST_LIB_BACKTRACE = 0;
            env.DIOXUS_DEVSERVER_PORT = 54580;
          };
      }
    );
}
