#![feature(default_field_values)]
//! `tilers_dioxus` — the Dioxus binding for the [`tilers_core`] packed-grid tiling
//! layout. A thin re-expression of the engine's view-model getters as `rsx!`, plus event wiring
//! that reads a DOM primitive and forwards it to a core reducer method. All layout logic lives in
//! [`tilers_core`]; this crate owns only the `Signal<PackedState>` cell, the templates, and
//! the imperative [`PackedApi`] handle a host drives.
//!
//! Web-only (DOM-backed). Tiles have a fixed starting size, snap to a step grid, never overlap,
//! and leave whitespace below (InsilicoTerminal's look). Panel content lives in a flat, id-keyed
//! content overlay so component instances (and their JS state) survive layout restructuring.

pub mod panel;
pub mod view;

pub use panel::DockPanel;
pub use tilers_core::{Band, Config, Group, GroupId, MinSize, PackedGrid, PackedState, PanelId, Step, keys, model};
pub use view::{PackedApi, PackedArea, Tile};
