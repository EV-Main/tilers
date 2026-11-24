//! The consumer-facing content seam. Equivalent of `dockview-core/src/framwork.ts`
//! + the framework adapters (`docs/refs/dockview-react`, `dockview-vue`): the point
//! where an app injects its own widget for each panel id while the library owns layout.

use dioxus::prelude::*;
use tilers_core::{MinSize, PanelId};

/// One widget the consumer hands to [`PackedArea`](crate::view::PackedArea). The `content`
/// `Element` is rendered once per panel in a stable, id-keyed list inside the content-overlay
/// layer — never re-parented when the layout changes — so the component instance (and any inner
/// JS state, e.g. a live map) is preserved. Core only ever needs the `id` + `title`; this Dioxus
/// VNode is placed by the binding's overlay loop.
///
/// `label` is what its tab shows, `title` when `None`; the drag ghost and the help name it by `title`.
/// A press on a control in it (a button, a link, a field) is the control's, never the tab's. The tab's
/// close is the framework's, after the label.///
/// `content` is an eagerly-built `VNode`; that is fine because the overlay diffs by key, not by
/// tree position. If lazy construction is ever needed, swap to `Callback<PanelId, Element>` — the
/// overlay's keying contract stays identical.
#[derive(Clone, PartialEq)]
pub struct DockPanel {
	pub id: PanelId,
	pub title: String,
	pub content: Element,
	/// What the content needs to stay legible; every tile hosting it is floored to fit.
	pub min: MinSize,
	pub label: Option<Element> = None,
}
