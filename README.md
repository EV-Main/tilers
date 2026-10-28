# tilers
![Minimum Supported Rust Version](https://img.shields.io/badge/nightly-1.92+-ab6000.svg)
[<img alt="crates.io" src="https://img.shields.io/crates/v/tilers.svg?color=fc8d62&logo=rust" height="20" style=flat-square>](https://crates.io/crates/tilers)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-66c2a5?style=for-the-badge&labelColor=555555&logo=docs.rs&style=flat-square" height="20">](https://docs.rs/tilers)
![Lines Of Code](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/valeratrades/b48e6f02c61942200e7d1e3eeabf9bcb/raw/tilers-loc.json)
<br>
[<img alt="ci errors" src="https://img.shields.io/github/actions/workflow/status/EV-Main/tilers/errors.yml?branch=main&style=for-the-badge&style=flat-square&label=errors&labelColor=420d09" height="20">](https://github.com/EV-Main/tilers/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->
[<img alt="ci warnings" src="https://img.shields.io/github/actions/workflow/status/EV-Main/tilers/warnings.yml?branch=main&style=for-the-badge&style=flat-square&label=warnings&labelColor=d16002" height="20">](https://github.com/EV-Main/tilers/actions?query=branch%3Amain) <!--NB: Won't find it if repo is private-->

🌐 **[Live demo](https://EV-Main.github.io/tilers/)** — no setup, runs in the browser.

A tiling/docking layout for [Dioxus](https://dioxuslabs.com/) and [Leptos](https://leptos.dev/) — the IDE/trading-terminal kind: panes split, resize, tab together, float, and maximize, with the arrangement cached per screen size and restored on reload.

![fuzz trace, seed 172](docs/.readme_assets/assets/fuzz.svg)

A random fuzz run (seed 172), not a demo. For the real thing, [play with it](https://EV-Main.github.io/tilers/).
// generate it yourself: `nix run .#film`.

## Usage
Hand `PackedArea` a `Signal` of `DockPanel`s (id + title + the content to render + what that content needs); the library owns layout, you own content.

```rust
use dioxus::prelude::*;
use tilers_dioxus::{Config, DockPanel, Group, MinSize, PackedApi, PackedArea, PanelId};

fn app() -> Element {
    let panels = use_signal(Vec::<DockPanel>::new);
    // The per-tile `+` button asks the host to open a tab; it must find this in context.
    use_context_provider(|| Callback::new(|_gid| {}));

    let on_band = Callback::new(move |mut api: PackedApi| {
        // The cache already had this band's layout; re-attach content by walking `api.tab_ids()`.
        if api.restored() { return }
        let mut panels = panels;
        panels.write().clear();
        for (id, title) in [("chart", "Chart"), ("orders", "Orders")] {
            let id = PanelId(id.into());
            let min = MinSize::Px { w: 240.0, h: 120.0 }; // the content box, below the title bar
            panels.write().push(DockPanel { id: id.clone(), title: title.into(), content: rsx! { "…" }, min });
            let gid = api.mint_group_id();
            api.place(Group::new(gid, id), 22, 16);
        }
    });

    let config = Config { storage_key: Some("my-app-layout".into()), ..Default::default() };
    rsx! {
        // Needs a sized parent; height:100% collapses to 0 otherwise.
        div { style: "position:fixed; inset:0;",
            PackedArea { panels, on_band: Some(on_band), config: Some(config) }
        }
    }
}
```

Runnable demo: `dx serve --example insilico --package tilers_dioxus --platform web` — or open the [hosted demo](https://EV-Main.github.io/tilers/), no local setup needed. The Leptos binding is the same component with `Arc<dyn Fn(PackedApi) + Send + Sync>` in place of `Callback` — see `examples/insilico_leptos`.

**Props:** `panels` (order = stable overlay render order — don't reorder it, that remounts panels), `on_band` (see below), `config`.

**Mins** belong to the panel: `MinSize::Px`/`Rem` size its content box, `Steps` the whole tile. Every tile is floored to the max over its tabs, re-resolved on every measure, so a px need holds however short the window gets; a want the floor overrode comes back once the floor drops. Edit a panel's `min` in `panels` to re-floor it.

**Bands and the seed cache.** A container width classifies into one of three `Band`s — `sm`/`md`/`xl` — each keeping its own arrangement, because a phone doesn't want a desktop's tiling. Set `Config::storage_key` and the framework caches the layout in `localStorage` under `<key>-<band>` on `leader s`, restoring it before your code runs. `on_band` fires once per band entry — at mount, and again on every crossing — *after* that resolution:

- `api.restored()` ⇒ the cache had a layout and the grid is already populated; walk `api.tab_ids()` to rebuild your panel list from the ids it holds.
- otherwise the grid is empty and it's yours to fill: `place` your tiles, or `load` a default you fetched yourself.

A cached payload that is corrupt, from a future schema, or empty counts as a miss and says so on the console — never a silent reset. Persistence is wasm-only (a no-op natively, so every band reports a miss).

**Publishing a default.** The framework never talks to a server. `leader S` caches, then hands `Config::on_publish` the band and its layout JSON, after the keydown; persist it wherever you like (behind your own authorization) to become the default fresh clients `load` in `on_band`, and say how it went with `api.notify(text)`, the dock's toast. `leader s` caches alone, and toasts on its own.

**Resizing.** A grip takes its own tile only. On a boundary between two tiles it is the left tile's right grip: dragging it left shrinks that tile and leaves the right one where it is; dragging it right pushes the tiles it reaches, shrinking them down to their min. A left edge with room beside it has a grip of its own, pushing what it reaches the same way; the view's edge and a boundary have none. In a dock that is framed (its page in an iframe, or itself inside another dock's tile), a tile on the right edge of the view, or reaching or running past its bottom, has that grip just inside the edge instead, clear of the host's own; it only shrinks the tile, and the bottom one brings the tile's bottom to where you pull it. The bottom grip sets the height and pushes the stack below down; the corner is the right grip and the bottom one. A gesture is worked out from the layout at the press, so dragging back restores whatever it shrank.

**Scripting** — `PackedApi` arrives through `on_band`; stash it in a signal to drive the dock from elsewhere. Every method is a read/write of the one state cell:

```rust
let gid = api.mint_group_id();
api.place(Group::new(gid, id), w, h);    // auto-packs left-to-right, top-to-bottom
api.add_tab(gid, panel_id);
api.close_active(gid);
api.resize(idx, new_w, new_h);             // the right grip and the bottom one, at once
api.reset();                               // wipe the layout back to empty
let json = api.save(); api.load(&json)?;   // `load` errors on corrupt/future JSON
api.notify(text);                          // the dock's toast
api.cols(); api.band(); api.tab_ids();
```

**Keybinds** (`Config::keymap`). The framework claims only `Keymap::root` and one leader, `Alt+z` (`Ctrl+t` on macOS); everything else waits behind the leader, where a which-key popup lists the next keys after `--tl-hint-delay` (150ms). Root holds `Ctrl+z`/`Ctrl+y`/`Ctrl+Shift+z` (`Cmd` on macOS) for undo/redo; aimed at a text field they stay the browser's text undo. Behind the leader: `u`/`U` undo/redo, `Backspace` closes the focused pane, `f` maximizes it, `d` inspects it, `?` lists the whole keymap (as does each tile's `?` button), and `s`/`S` cache/cache-and-publish the layout (unbound without `storage_key`/`on_publish`). Rebinding the leader is `Keymap { leader: Chord { key: "g", alt: true, .. }.into(), ..Default::default() }`; dropping it for flat binds is `k.root.append(&mut k.scope)`. Your own commands are `Node::Run(Command::Custom { label, run })` at any depth, `Node::Scope` nests.

**Grid** — `Config::steps` (default 64) is the desktop column count, scaled down per band so the *physical* step stays ~constant and tiles reflow instead of shrinking; `Config::row_h_rem` (default `2rem`) is the least row, stretched so a layout shorter than the view ends at its bottom.

**Theming** — only structural CSS ships; set `--tl-*` custom properties on any ancestor for colors/sizes (`--tl-accent`, `--tl-fg`, `--tl-group-bg`, `--tl-tab-bg`, `--tl-tab-active-bg`, `--tl-tab-active-fg`, `--tl-tab-border`, `--tl-tabstrip-bg`, `--tl-resize-bg`, `--tl-shadow-bg`, `--tl-content-pad`, `--tl-hint-delay`).



<br>

<sup>
	This repository follows <a href="https://github.com/valeratrades/.github/tree/master/best_practices">my best practices</a> and <a href="https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md">Tiger Style</a> (except "proper capitalization for acronyms": (VsrState, not VSRState) and formatting). For project's architecture, see <a href="./docs/ARCHITECTURE.md">ARCHITECTURE.md</a>.
</sup>

#### License

<sup>
	Licensed under <a href="LICENSE">Blue Oak 1.0.0</a>
</sup>

<br>

<sub>
	Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be licensed as above, without any additional terms or conditions.
</sub>

