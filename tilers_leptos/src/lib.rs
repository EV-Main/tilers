#![feature(default_field_values)]
//! `tilers_leptos` — the Leptos binding for the [`tilers_core`] packed-grid tiling
//! layout. Same shape as the Dioxus binding, Leptos idioms: one `RwSignal<PackedState>` (thread-
//! local storage, since [`PackedState`] holds `Rc`s and so is
//! `!Send`), `view!` templates rendered from the core view-model getters, and `on:*` events
//! forwarding one DOM primitive each to the identical reducer methods.
//!
//! The tile *skeleton* re-renders coarsely (it holds no user content, so churning it is harmless);
//! the *content overlay* is a keyed `<For>` whose per-panel position is a fine-grained reactive
//! style — so a panel's view node survives layout changes while it still tracks its tile. This is
//! the same property the Dioxus binding gets from keyed diffing, proving one core drives both.

pub mod panel;
pub mod view;

pub use panel::{DockPanel, PanelView};
pub use tilers_core::{Band, Config, Group, GroupId, MinSize, PackedGrid, PackedState, PanelId, Step, keys, model};
pub use view::{PackedApi, PackedArea};
