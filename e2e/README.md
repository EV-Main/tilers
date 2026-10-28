# Cross-browser browser tests

Everything here runs in **both** Chromium and Firefox, since that split is what silently broke
before.

- `keybind.spec.mjs` — the key footprint: bare letters stay the page's, `Alt+z` then a key acts
  (`preventDefault()` + a `tilers keybind:` console line), `Control+z` is ours on the page and the
  browser's inside an input, the which-key `.tl-hint` reveals only after its delay, macOS takes
  `Control+t` (via a `navigator.platform` override), and a tile's `?` opens the help naming it. The
  lib prevents only when it acted, so the undo checks make an edit first.
- `seed-cache.spec.mjs` — the built-in per-band `localStorage` cache: `Alt+z s` writes
  `<storage_key>-<band>`, a reload restores it instead of re-seeding, another band uses another
  key, and `Alt+z S` caches and calls the host hook. Each says so in the dock's `.tl-notice`. Playwright gives each test a fresh context, so
  the cache starts empty without any wiping (an `addInitScript` clear would also run on `reload`
  and destroy what the test just saved).
- `autoscroll.spec.mjs` — a drag held past the root's top (over the host chrome) or inside its
  bottom band scrolls it with no further moves, and the drop lands in the scrolled view. Firefox is
  `fixme`: its drag never arms (moves go to the pressed header, not the capture surface).
- `foreign-drag.spec.mjs` — a native `dragenter` (with a `DataTransfer`, as a host card or a file
  sends) on an inactive tab activates it and shows its content.
- `grafana.spec.mjs` — the Grafana plugin (`tilers_grafana`) against `nix run .#dev -- grafana`
  and its testdata fixture: panels render natively in tiles, tab-joins switch panels, an inactive
  tab issues no queries, the root re-measures on a container-only resize, the cache survives a
  reload, declared and compiled mins hold when the window halves, a second load takes compiled sizes
  from the cache, `?`, `d` and `Control+z` stay Grafana's while `Alt+z ?` opens ours, and an idle dock mutates nothing. Chromium only, as its own project:
  `npm test -- --project grafana` (`GRAFANA_URL` overrides `http://127.0.0.1:54581`).

- `grafana-host.spec.mjs` — the same project, the plugin framed with `?_tilers.host` by a same-origin
  parent page (the dev Grafana allows embedding): panels sharing a `gridPos` open as tabs; nothing mounts
  before the parent's `host` answer; a published layout opens fresh, the browser's cache beats it;
  `leader S` posts `publish` only if allowed, and a `notify` reply shows in the toast; no flag, no messages.
- `grafana-drag.spec.mjs` — a native drag from one panel's slot held over an inactive tab of another tile opens it, and
  that panel's content takes the drop.

## Run

Needs a host that mounts `PackedArea`. Serve the bundled example:

```sh
dx serve --example insilico --package tilers_dioxus --platform web --port 8111
```

then, from this dir:

```sh
npm install            # pinned to @playwright/test 1.63.0 (matches the nix browser set)
npm test
```

Point it at any other host (e.g. the real REA dashboard) with `TILERS_URL`:

```sh
TILERS_URL=http://127.0.0.1:8122/ npm test
```

## NixOS

Playwright's own browser downloads don't run on NixOS; use the nix-provided set instead. The pin
`@playwright/test@1.63.0` matches `chromium-1243`/`firefox-1543`, which `playwright-driver.browsers`
ships. Export its store path before running:

```sh
export PLAYWRIGHT_BROWSERS_PATH=$(nix eval --raw nixpkgs#playwright-driver.browsers)
```

(`PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=1` is set automatically in `playwright.config.mjs`.)
