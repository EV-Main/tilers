# Tilers

Opens a Grafana dashboard at `/a/evmain-tilers-app/d/<uid>`, laid out by
[tilers](https://github.com/EV-Main/tilers): panels resize, join as tabs, maximize, and
each browser keeps its own arrangement (`Alt+Z`, then `S`). Panels render through Grafana's own `VizPanel`.

Rows, repeats, library panels, panel links and non-built-in annotations are refused with an error
naming the panel; open such a dashboard natively.

## Tabs

Panels whose `gridPos` is identical open as the tabs of one tile, in the dashboard's `panels` order, the
first active. A partial overlap is packed as separate tiles, as ever.

## Framed in another page

Add `_tilers.host` to the URL the page frames (next to `_dash.hideTimePicker` and the other kiosk flags).
Without it the plugin never posts nor listens. With it outside a frame, the page errors. Grafana has to
allow being framed (`allow_embedding`).

Messages are objects with a `tilers` kind. The plugin only accepts those from `window.parent`, and a
malformed one throws, naming what is wrong.

```
plugin -> page  { tilers: 'ready' }
                Once per dock mount, when the plugin listens. The dock does not mount until it is answered.
page -> plugin  { tilers: 'host', published: { sm?: string, md?: string, xl?: string }, publish: boolean }
                Exactly one per `ready`. `published[band]` is a layout this plugin produced (the `layout` of an
                earlier `publish`); it opens when the browser has no cached layout for the band. A layout that
                does not load is logged and the band seeds. `publish` binds `leader S`.
plugin -> page  { tilers: 'publish', band: 'sm' | 'md' | 'xl', layout: string }
                On `leader S` (`Alt+Z`, then `Shift+S`), after the browser's cache has written it.
page -> plugin  { tilers: 'notify', text: string }
                Any time after `host`: a line in the dock's toast, e.g. how the publish went.
```

Storing and authorizing published layouts is the page's business.
