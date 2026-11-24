//! The stylesheet. Its geometry is the framework's own; a host themes it through the `--tl-*` color
//! and timing properties only. Both bindings inject this verbatim into a `<style>`.

pub const CSS: &str = r#"
.tl-group { display: flex; flex-direction: column; width: 100%; height: 100%;
	background: var(--tl-group-bg, #1e1e1e); }
/* One header bar holds the tabs and the actions (insilico's elevated tab strip); the active
   tab is the title, so there's no separate titlebar. `--tl-title-h` is emitted on the root from
   `Config::title_h_rem`, which the state also resolves to px for the content overlay's chrome
   offset — the two cannot drift.
   Its empty area is the tile's move-handle; tabs/actions stop propagation for their own gestures.
   The left rim keeps a grip on the whole tile however many tabs fill the strip. */
.tl-header { flex: 0 0 auto; height: var(--tl-title-h, 2rem); box-sizing: border-box; display: flex;
	padding: calc(var(--tl-title-h, 2rem) / 8) 0 0 calc(var(--tl-title-h, 2rem) * 3 / 8);
	align-items: stretch; overflow: hidden; cursor: grab; background: var(--tl-tabstrip-bg, #2d2d2d);
	position: relative; user-select: none; }
/* A locked dock's strips don't move their tiles. */
.tl-locked .tl-header { cursor: default; }
/* The grip's paint on the left rim, the drag-handle texture. */
.tl-header::before { content: ""; position: absolute; pointer-events: none;
	left: calc(var(--tl-title-h, 2rem) / 16); width: calc(var(--tl-title-h, 2rem) / 4);
	top: calc(var(--tl-title-h, 2rem) / 4); bottom: calc(var(--tl-title-h, 2rem) / 8);
	background: radial-gradient(circle, var(--tl-fg, #ddd) 1px, transparent 1.2px) center / 4px 4px;
	opacity: 0.3; }
/* Lit only where a press takes the whole tile, never over a tab or an action. */
.tl-header:hover:not(:has(.tl-tab:hover, .tl-actions:hover))::before { opacity: 0.6; }
.tl-actions { flex: 0 0 auto; margin-inline-start: auto; display: flex; align-items: center; gap: 0.125rem; padding: 0 0.25rem; }
/* A host's buttons come first, then the framework's own. */
.tl-action { cursor: pointer; border: 0; background: transparent; color: var(--tl-fg, #ddd);
	opacity: 0.55; padding: 0 0.3125rem; font: inherit; line-height: 1; }
.tl-action:hover { opacity: 1; background: var(--tl-tab-bg, #2d2d2d); }
.tl-action:disabled { cursor: default; opacity: 0.3; background: transparent; }
/* A lock scope's switch, wherever the host places it. */
.tl-lock::before { content: ""; display: block; width: 1em; height: 1em; background: currentColor;
	mask: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round'%3E%3Crect x='5' y='11' width='14' height='10' rx='2'/%3E%3Cpath d='M8 11V7a4 4 0 0 1 7.5-2'/%3E%3C/svg%3E") center / contain no-repeat; }
.tl-lock[aria-pressed="true"]::before {
	mask-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='black' stroke-width='2' stroke-linecap='round'%3E%3Crect x='5' y='11' width='14' height='10' rx='2'/%3E%3Cpath d='M8 11V7a4 4 0 0 1 8 0v4'/%3E%3C/svg%3E"); }
.tl-tab { display: flex; align-items: center; gap: 0.375rem; padding: 0 0.875rem; font-size: 0.8125rem;
	white-space: nowrap; cursor: pointer; background: var(--tl-tab-bg, #2d2d2d);
	border-right: 1px solid var(--tl-tab-border, #1e1e1e); }
.tl-tab.tl-active { background: var(--tl-tab-active-bg, #1e1e1e);
	color: var(--tl-tab-active-fg, #fff); }
.tl-tab:hover { filter: brightness(1.3); }
/* A tab's own close: shown on the active tab, and on any tab under the pointer. */
.tl-tab-close { cursor: pointer; border: 0; background: transparent; color: inherit; font: inherit; line-height: 1;
	padding: 0 0.125rem; opacity: 0; }
.tl-tab.tl-active .tl-tab-close, .tl-tab:hover .tl-tab-close { opacity: 0.55; }
.tl-tab .tl-tab-close:hover { opacity: 1; }
.tl-content-slot { flex: 1 1 auto; overflow: hidden; }
/* The content overlay and the floats' frames: as large as the scrolled content, so a float's sticky box
   holds its place in the view at any scroll. `--tl-extent-*` come from `PackedState::root_style`. */
.tl-overlay, .tl-floats { position: absolute; left: 0; top: 0; pointer-events: none;
	width: max(100%, var(--tl-extent-w, 0px)); height: max(100%, var(--tl-extent-h, 0px)); }
/* A float: its frame paints over its content, so its body is see-through and only its strip and
   grips take the pointer. */
.tl-tile.tl-float { display: block; background: transparent; pointer-events: none; box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45); }
.tl-float .tl-group { background: transparent; }
.tl-float .tl-header, .tl-float .tl-resize-handle, .tl-float .tl-resize-left, .tl-float .tl-resize-right,
.tl-float .tl-resize-bottom, .tl-float .tl-resize-fold, .tl-float .tl-resize-wall { pointer-events: auto; }
.tl-render-overlay { position: absolute; overflow: hidden; pointer-events: auto;
	box-sizing: border-box; padding: var(--tl-content-pad, 0.375rem 0.625rem); }
/* Pinned to half the title bar, so the grab corner keeps its proportion at any chrome height. */
.tl-resize-handle { position: absolute; right: 0; bottom: 0;
	width: calc(var(--tl-title-h, 2rem) / 2); height: calc(var(--tl-title-h, 2rem) / 2);
	cursor: nwse-resize; z-index: 101;
	/* opaque where a scrollbar still runs under it (no track margins outside Chromium/WebKit) */
	background: linear-gradient(var(--tl-resize-bg, #555), var(--tl-resize-bg, #555)), var(--tl-group-bg, #1e1e1e); }
/* The edge grips: thin strips over the content's own edges, bare until hovered. */
.tl-resize-bottom, .tl-resize-fold, .tl-resize-left, .tl-resize-right, .tl-resize-wall { position: absolute; z-index: 100; }
.tl-resize-bottom { left: 0; right: 0; bottom: 0; height: calc(var(--tl-title-h, 2rem) / 8); cursor: ns-resize; }
.tl-resize-left, .tl-resize-right, .tl-resize-wall { top: var(--tl-title-h, 2rem); bottom: 0; width: calc(var(--tl-title-h, 2rem) / 8); cursor: ew-resize; }
.tl-resize-left { left: 0; }
/* Just before the band a host dock's own grip may cover when this dock is framed in one of its tiles:
   the view's last two grip-thicknesses (a host's root font may be larger) and its 1px border. Measured
   from inside the tile's 1px border. */
.tl-resize-wall { left: calc(var(--tl-wall) - 2px - var(--tl-title-h, 2rem) * 3 / 8); cursor: w-resize; }
.tl-resize-fold { left: 0; right: 0; top: calc(var(--tl-fold) - 2px - var(--tl-title-h, 2rem) * 3 / 8); height: calc(var(--tl-title-h, 2rem) / 8); cursor: n-resize; }
.tl-resize-right { right: 0; }
.tl-resize-bottom:hover, .tl-resize-fold:hover, .tl-resize-left:hover, .tl-resize-right:hover, .tl-resize-wall:hover { background: var(--tl-resize-bg, #555); }
/* Content keeps the whole body; only its scrollbars give way: they stop short of the corner, and their
   outer edge (under the edge grip) is left bare, so the thumb is never under a grip. */
.tl-render-overlay ::-webkit-scrollbar { width: calc(var(--tl-title-h, 2rem) / 8 + 0.5rem); height: calc(var(--tl-title-h, 2rem) / 8 + 0.5rem); }
.tl-render-overlay ::-webkit-scrollbar-thumb { background: var(--tl-resize-bg, #555); background-clip: padding-box; border-radius: 6px; }
.tl-render-overlay ::-webkit-scrollbar-thumb:vertical { border-right: calc(var(--tl-title-h, 2rem) / 8) solid transparent; }
.tl-render-overlay ::-webkit-scrollbar-thumb:horizontal { border-bottom: calc(var(--tl-title-h, 2rem) / 8) solid transparent; }
.tl-render-overlay ::-webkit-scrollbar-track:vertical { margin-bottom: calc(var(--tl-title-h, 2rem) / 2); }
.tl-render-overlay ::-webkit-scrollbar-track:horizontal { margin-right: calc(var(--tl-title-h, 2rem) / 2); }
.tl-render-overlay ::-webkit-scrollbar-corner { background: transparent; }
.tl-resize-handle::after { content: "⌟"; position: absolute; right: 0.08em; bottom: -0.23em;
	font-size: calc(var(--tl-title-h, 2rem) / 2); line-height: 1; color: var(--tl-fg, #ddd); }
/* Horizontal is fully model-bounded (every tile satisfies x + w ≤ cols), so a horizontal
   scrollbar is never legitimate — clip it. Only the vertical axis (whitespace/stack below) scrolls. */
.tl-packed { position: relative; width: 100%; height: 100%; overflow-x: hidden; overflow-y: auto;
	color: var(--tl-fg, #ddd); font: 0.8125rem/1.4 system-ui, sans-serif; }
/* `Config::scroll_x`: the grid runs past the view's width, so this axis scrolls too. */
.tl-scroll-x { overflow-x: auto; }
.tl-tile { position: absolute; overflow: hidden; box-sizing: border-box;
	background: var(--tl-group-bg, #1e1e1e); border: 1px solid var(--tl-tab-border, #333); }
/* Drop feedback: the landing cell drawn as a plain greyed-out area (no chrome, no content),
   the floating ghost that tracks the pointer, and a Tab target's drop site. */
.tl-shadow { background: var(--tl-shadow-bg, rgba(160, 160, 160, 0.18)); border-style: dashed; }
.tl-ghost { position: fixed; z-index: 1001; pointer-events: none; opacity: 0.8; overflow: hidden;
	background: var(--tl-group-bg, #1e1e1e); border: 1px solid var(--tl-accent, #63e9cd);
	box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45); }
.tl-tab-drop { box-shadow: inset 0 0 0 2px var(--tl-accent, #63e9cd); }
/* Which-key popup while a chord is pending. It mounts at once and reveals after the delay, so a
   chord typed fast never flashes it. `visibility`, not `opacity`: a transparent box still counts as
   shown to hit-testing and to tests. */
.tl-hint { position: absolute; right: 0.75rem; bottom: 0.75rem; z-index: 1075; min-width: 11.25rem;
	padding: 0.625rem 0.875rem; background: var(--tl-group-bg, #1e1e1e); border: 1px solid var(--tl-accent, #63e9cd);
	border-radius: 6px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
	animation: tl-hint-in 0s var(--tl-hint-delay, 150ms) both; }
@keyframes tl-hint-in { from { visibility: hidden; } to { visibility: visible; } }
.tl-hint-prefix { font-family: ui-monospace, monospace; opacity: 0.6; margin-bottom: 0.375rem; }
.tl-hint-row { display: flex; gap: 0.75rem; padding: 0.125rem 0; }
/* A toast in the hint's corner, where the chord that caused it was just shown; it ends hidden. */
.tl-notice { position: absolute; right: 0.75rem; bottom: 0.75rem; z-index: 1075; padding: 0.625rem 0.875rem;
	pointer-events: none; background: var(--tl-group-bg, #1e1e1e); border: 1px solid var(--tl-accent, #63e9cd);
	border-radius: 6px; box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
	animation: tl-notice-out 400ms var(--tl-notice-hold, 2500ms) both; }
@keyframes tl-notice-out { to { opacity: 0; visibility: hidden; } }
/* The full keymap: a dim scrim over the whole root with a centered card. */
.tl-help-scrim { position: absolute; inset: 0; z-index: 1100; display: flex; align-items: center;
	justify-content: center; background: rgba(0, 0, 0, 0.45); cursor: pointer; }
.tl-help { min-width: 15rem; padding: 0.875rem 1.125rem; background: var(--tl-group-bg, #1e1e1e);
	border: 1px solid var(--tl-accent, #63e9cd); border-radius: 6px; cursor: default;
	box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5); }
.tl-help-title { font-weight: 600; margin-bottom: 0.5rem; }
.tl-help-pane { opacity: 0.7; margin-bottom: 0.5rem; }
.tl-help-row { display: flex; justify-content: space-between; gap: 1.5rem; padding: 0.1875rem 0; }
.tl-help-key { font-family: ui-monospace, monospace; background: var(--tl-tab-bg, #2d2d2d);
	padding: 0 0.375rem; border-radius: 3px; color: var(--tl-tab-active-fg, #fff); }
.tl-help-foot { margin-top: 0.625rem; opacity: 0.5; font-size: 0.6875rem; }
/* Inspect popup: translucent dimensions box over a container, content shows through. */
.tl-inspect { position: absolute; z-index: 1050; pointer-events: none;
	display: flex; align-items: center; justify-content: center;
	background: var(--tl-group-bg, rgba(30, 30, 30, 0.25));
	border: 1px solid var(--tl-accent, #63e9cd);
	color: var(--tl-tab-active-fg, #fff); font: 600 1.125rem ui-monospace, monospace; }
"#;
