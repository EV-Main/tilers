#![feature(default_field_values)]
//! `tilers_core` — the framework-agnostic engine behind the `tilers` packed-grid
//! tiling layout. No Dioxus, no Leptos: a pure data **model** ([`model`]), a gesture **reducer**
//! ([`state`]) over a plain [`PackedState`] struct, the built-in per-[`Band`] seed cache, and the
//! structural **[`CSS`]**. Both the Dioxus and Leptos bindings drive this same core.
//!
//! Tiles have a fixed starting size, snap to a step grid, never overlap, and fill the view down to
//! its bottom (InsilicoTerminal's look). Every field the old Signal-driven renderer split across ~14
//! reactive cells lives here as a plain field of [`PackedState`], so the whole gesture machine
//! (drag/resize/undo/fit) is unit-testable without a DOM — see `tests/integration`.

pub mod config;
pub mod keys;
pub mod model;
pub mod state;

mod css;
mod persist;
pub use config::{Band, Breakpoint, Config};
pub use css::CSS;
pub use model::{
	Group, GroupId, PanelId,
	packed::{MinSize, PackedGrid, Step},
};
#[cfg(target_arch = "wasm32")]
pub use state::framed;
pub use state::{ContentSlot, Frame, Ghost, Grip, Notice, PackedState};
