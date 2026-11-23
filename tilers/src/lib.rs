//! `tilers` — a packed-grid tiling/docking layout: fixed-size tiles that snap to a step grid,
//! never overlap, and leave whitespace below (InsilicoTerminal's look). Panes split, resize, tab
//! together, collapse, maximize, and float, with the arrangement cached per screen band in `localStorage`.
//!
//! This crate is a thin **facade** over the ecosystem — pick a UI binding with a feature flag:
//!
//! ```toml
//! tilers = { version = "0.5", features = ["dioxus"] }   # or ["leptos"]
//! ```
//!
//! - Always available: [`core`] re-exports [`tilers_core`] — the framework-agnostic engine
//!   (grid model, gesture reducer, seed cache, CSS). A bare `tilers` (no features) is just this.
//! - `dioxus` feature → `dioxus` re-exports [`tilers_dioxus`](https://docs.rs/tilers_dioxus) (the Dioxus binding).
//! - `leptos` feature → `leptos` re-exports [`tilers_leptos`](https://docs.rs/tilers_leptos) (the Leptos binding).
//!
//! Depending on this facade vs. the individual crates is purely ergonomic — the code is identical
//! either way.

pub use tilers_core as core;
#[cfg(feature = "dioxus")]
pub use tilers_dioxus as dioxus;
#[cfg(feature = "leptos")]
pub use tilers_leptos as leptos;
