Hand `PackedArea` a `Signal` of `DockPanel`s (id + title + the content to render + what that content needs); the library owns layout, you own content.

```rust
use dioxus::prelude::*;
use tilers_dioxus::{Config, DockPanel, Group, MinSize, PackedApi, PackedArea, PanelId};

fn app() -> Element {
    let panels = use_signal(Vec::<DockPanel>::new);

    let on_band = Callback::new(move |mut api: PackedApi| {
        // The cache already had this band's layout; re-attach content by walking `api.tab_ids()`.
        if api.restored() { return }
        let mut panels = panels;
        panels.write().clear();
        for (id, title) in [("chart", "Chart"), ("orders", "Orders")] {
            let id = PanelId(id.into());
            let min = MinSize::Px { w: 240.0, h: 120.0 }; // the content box, below the title bar
            panels.write().push(DockPanel { id: id.clone(), title: title.into(), content: rsx! { "…" }, min, .. });
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

Runnable demo: `dx serve --example insilico --package tilers_dioxus --platform web` — or open the [hosted demo](https://@@OWNER@@.github.io/tilers/), no local setup needed. The Leptos binding is the same component with `Arc<dyn Fn(PackedApi) + Send + Sync>` in place of `Callback` — see `examples/insilico_leptos`.

**Props:** `panels` (order = stable overlay render order — don't reorder it, that remounts panels), `on_band` (see below), `config`, `actions` and `on_add` (below).

**Tile chrome is the framework's.** A tile's header is tilers' own strip — its empty area moves the tile, a tab drags out or joins another — and its management is always there: each tab's `×`, then `?` (the keybinds), `+` (add a tab), `⧉`/`⊟` (float/dock) and `✕` (close the tile). What a host can do with it:
- `DockPanel::label`: what the panel's tab shows before its `×` (an icon, a count); `None` shows `title`, which the drag ghost and the help always use. A press on a control in a label (`button`, `a`, `input`, `select`, `textarea`) is the control's, never a drag.
- `actions: Callback<Tile, Element>`: buttons of the host's own, **added** before the framework's in each header; `Tile` carries its `group`, `active` panel, `maximized`/`floating` and the `api` to act on it. A press anywhere in them is never the tile's. It can't remove, replace or restyle the framework's buttons.
- `on_add: Callback<GroupId>`: what the `+` does — the host picks what opens, e.g. a catalog that `api.add_tab`s into that tile. Without it there is no `+`, since nothing could be added.
- `LockScope`: the one way to take management away (below).

```rust
let actions = Callback::new(|Tile { group, maximized, mut api, .. }: Tile| rsx! {
    button { class: "tl-action", onclick: move |_| api.maximize(group), if maximized { "⤡" } else { "⤢" } }
});
let on_add = Callback::new(move |group: GroupId| catalog.set(Some(group)));
```

**Floating.** A float is a group above the grid, never a cell: `⧉` (or `leader z`) lifts a tile out where it shows, and the grid settles into its space; `⊟` (or `leader z` again) docks it where it shows, as if dropped there — onto a header it joins that tile, over a body it displaces below it, in open space it packs. Its header moves it, its edges and corner resize it (in grid steps, held inside the view), any press raises it, and dragging one of its tabs docks it exactly where a tile drag would land, with the same preview. It stays put while the grid scrolls under it, its content is the same mounted node throughout, and floats are saved with the band's layout. A host opens one directly with `api.float(group, w, h)` (centered, on top) or toggles one with `api.toggle_float(group)`. A float is not a drop target: a drag over it lands on the grid beneath.

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
api.close(&panel_id);                      // its tile goes with its last tab
api.float(Group::new(gid, id), w, h);     // a new group, floating above the grid
api.toggle_float(gid);                     // float a tile out where it shows, or dock a float where it shows
api.maximize(gid);                         // toggles
api.resize(idx, new_w, new_h);             // the right grip and the bottom one, at once
api.reset();                               // wipe the layout back to empty
let json = api.save(); api.load(&json)?;   // `load` errors on corrupt/future JSON
api.notify(text);                          // the dock's toast
api.cols(); api.band(); api.tab_ids();     // tab_ids: tiles' then floats'
```

**Dragging in from outside** — a host's own draggable (a sidebar item) lands in the dock where it is dropped: in its `ondragstart`, put the panel in `panels` and call `api.carry(panel_id, w, h)`. Over the dock the landing shows as for a tile drag, a tab strip takes it as a tab, and a drop anywhere else lands nothing. A panel already hosted is moved, not copied.

**Locking** — wrap an area in `LockScope { locked, .. }` over a signal of yours and place its `LockToggle {}` anywhere inside: while locked, the user can't move, resize, tear, close, float or add tiles in any dock in it, nested docks and nested scopes included; maximizing stays open, and your own calls still act.

**Nested docks** — a dock inside another's tile works as is: the keyboard goes to the dock last pressed in (the innermost), and before any press to the outer one.

**Keybinds** (`Config::keymap`). The framework claims only `Keymap::root` and one leader, `Alt+z` (`Ctrl+t` on macOS); everything else waits behind the leader, where a which-key popup lists the next keys after `--tl-hint-delay` (150ms). Root holds the leader and `Ctrl+z`/`Ctrl+y`/`Ctrl+Shift+z` (`Cmd` on macOS) for undo/redo; aimed at a text field they stay the browser's text undo. Behind the leader: `u`/`U` undo/redo, `Backspace` closes the focused tile (all its tabs), `f` maximizes the hovered one (the focused one when the pointer is over none), `z` floats or docks it, `d` inspects it, `?` lists the whole keymap (as does a header's `?`), and `s`/`S` cache/cache-and-publish the layout (unbound without `storage_key`/`on_publish`). Rebinding the leader is changing `on` of the root binding holding `Node::Scope`; dropping it for flat binds is moving that scope's children into `root`, starting from `Config::default().keymap`. Your own commands are `Node::Run { id, command: Command::Custom { label, run } }` at any depth, `id` unique and stable, `Node::Scope` nests.

**Grid** — `Config::steps` (default 64) is the desktop column count, scaled down per band so the *physical* step stays ~constant and tiles stack instead of shrinking; within a band the count is fixed, so a layout spans its view at any width the band covers. `Config::row_h_rem` (default `2rem`) is the least row, stretched so a layout shorter than the view ends at its bottom.

**Sideways scrolling** — `Config::scroll_x: true` lets the grid run past the view's width, as it runs past its bottom: steps are square (`row_h_rem` wide), tiles placed at their least width (`place(group, 1, h)`, floored by their mins) line up in one row, the view scrolls sideways when the row is wider and the row stretches to its right edge when narrower. `api.cols()` is then the grid's bound, not a width to share out.

**Theming** — set `--tl-*` custom properties on any ancestor for colors and timing: `--tl-accent`, `--tl-fg`, `--tl-group-bg`, `--tl-tab-bg`, `--tl-tab-active-bg`, `--tl-tab-active-fg`, `--tl-tab-border`, `--tl-tabstrip-bg`, `--tl-resize-bg`, `--tl-shadow-bg`, `--tl-content-pad` (the padding around your content inside its slot), `--tl-hint-delay`, `--tl-notice-hold`. The header's height is `Config::title_h_rem`, since the layout resolves it to px. That is the whole theming surface: the geometry of tiles, headers and tabs (padding, gaps, radii, borders) is the framework's, and the `.tl-*` classes are not a styling API — a host rule that overrides them is outside what tilers keeps working.
