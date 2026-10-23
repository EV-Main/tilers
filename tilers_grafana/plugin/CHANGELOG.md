# Changelog

## 0.3.3

Resizing a panel by its side pushes the panels it grows into, which give up the space, instead of
shoving them down; shrinking it leaves its neighbours where they are. A left edge with room beside it
has a grip of its own. Dragging back restores what a resize shrank. A panel on the right edge of the
view, or reaching its bottom, has that grip just inside the edge, clear of the frame's own; it only
shrinks the panel.
`leader s s` says it cached the layout in a toast.
The leader is `Alt+z` (was `Alt+t`), and binds match the key your layout sends, never its physical
position.

## 0.3.2

A panel keeps its whole body again: the bottom and right grips are thin strips over its edges, and only
its scrollbars give way, stopping short of the corner.

## 0.3.1

Panels resize by their bottom edge and their right edge as well as the corner; a panel ends above its
footer and left of its rail, so its scrollbar no longer sits under the corner.

## 0.3.0

A dock row is a Grafana row (30px and its 8px gap), so panels open at their dashboard height and keep it:
a shorter page or frame shows less, its whitespace going first, then scrolls; it no longer shrinks every
panel. Layouts cached before keep their rows, which are now taller. A tile's resize corner is opaque over
its panel.

## 0.2.2

Textbox variables render. Grafana's kiosk flags (`_dash.hideTimePicker`, `_dash.hideVariables`, `_dash.hideLinks`)
hide the same controls as on `/d/`, `native` going with the links; a hidden refresh picker still refreshes.

## 0.2.1

`precompile` compiles canvases whose buttons clip their own label (a placement shorter than Grafana's button):
only clipping beyond what the content does in an unbounded box counts as not fitting.

## 0.2.0

Panels keep their content as the window shrinks: a tile's floor follows the px its content needs at the live
row height, not the one it was placed at. A panel's `dock` key declares that need — `minW`/`minH` in CSS px for
growable panels (a table's header and first row), or `precompile: true` for content with a natural size (canvas
buttons, text), which the plugin measures once in its slot and caches per browser.

## 0.1.1

The release wasm loads: it is optimized by binaryen 132, where Ubuntu's binaryen broke wasm-bindgen's externref table.

## 0.1.0

First release: dashboards open at `/a/evmain-tilers-app/d/<uid>`, laid out by tilers.
