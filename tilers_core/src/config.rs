//! Host-supplied configuration for the packed layout, passed once into
//! [`PackedState`](crate::PackedState).

use std::rc::Rc;

use crate::keys::Keymap;

#[derive(Clone)]
pub struct Config {
	pub keymap: Keymap,
	/// Desktop (`Xl`) column count: how many grid steps span the container's width on a wide screen.
	/// Smaller [`Band`]s scale this down so the *physical* step stays ~constant and tiles stack
	/// instead of shrinking. The rendered horizontal step is `container_width / cols`, so within a band
	/// the layout stretches to fill. A finer grid (more steps) gives smaller resize/placement increments.
	/// Unused with [`scroll_x`](Self::scroll_x).
	pub steps: u32 = 64,
	/// Least height of a grid row, in `rem`: rows stretch so a layout shorter than the container ends at
	/// its bottom, and a deeper one scrolls at this length. Panel mins resolve against it. The default is
	/// the title bar's, and ≈ a square step on a desktop-wide container.
	pub row_h_rem: f64 = 2.0,
	/// `localStorage` namespace for the built-in seed cache; the [`Band`] is appended. `None` ⇒ no
	/// client cache at all, and every band entry reports a miss for the host to seed.
	pub storage_key: Option<String> = None,
	/// [`Publish`](crate::keys::Builtin::Publish): the host persists the band's layout JSON wherever it
	/// wants, to become the default fresh clients start from, and reports the outcome with `notify`.
	/// `None` ⇒ unbound, since the framework has nowhere to put a published layout on its own.
	pub on_publish: Option<Rc<dyn Fn(Band, String)>> = None,
	/// Height of a tile's title bar, in `rem` — so it tracks the reader's font size (the bar has to
	/// fit its tab text) rather than the viewport, which the grid already handles via [`Band`].
	/// Emitted as `--tl-title-h` on the root and resolved to px for the content overlay's offset.
	/// The corner resize handle is pinned to half of it.
	pub title_h_rem: f64 = 2.0,
	/// The grid runs past the view's width in square steps (`row_h_rem` wide) and the view scrolls
	/// sideways, instead of fitting `steps` columns to it: a row of tiles at their mins stays one row.
	/// Columns stretch so a layout narrower than the view ends at its right edge, as rows do at its bottom.
	pub scroll_x: bool = false,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			keymap: crate::keys::default_keymap(),
			..
		}
	}
}

/// Config never changes at runtime; a binding compares it only to decide whether to re-seed.
/// Closures have no identity, so `on_publish` compares by presence.
impl PartialEq for Config {
	fn eq(&self, other: &Self) -> bool {
		self.keymap == other.keymap
			&& self.steps == other.steps
			&& self.row_h_rem == other.row_h_rem
			&& self.storage_key == other.storage_key
			&& self.on_publish.is_some() == other.on_publish.is_some()
			&& self.title_h_rem == other.title_h_rem
			&& self.scroll_x == other.scroll_x
	}
}

/// The unit a layout is stored under, and the column count it is laid out in: phone, tablet,
/// desktop, at Bootstrap's `md`/`lg` boundaries (CSS px). The count scales with the band's design
/// width, so the *physical* step stays ~constant across devices and a phone's tiles stack instead of
/// shrinking. Within a band it is fixed and the grid stretches, so a layout looks the same at any
/// width its band covers.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize)]
pub enum Band {
	Sm,
	Md,
	#[default]
	Xl,
}

impl Band {
	/// Classify a container width (CSS px) into its band.
	pub(crate) fn of(width: f64) -> Self {
		match width {
			w if w < 768.0 => Self::Sm,
			w if w < 992.0 => Self::Md,
			_ => Self::Xl,
		}
	}

	/// Design width the band scales against — its upper edge, with `Xl` (open-ended) at 1600.
	const fn design(self) -> f64 {
		match self {
			Self::Sm => 768.0,
			Self::Md => 992.0,
			Self::Xl => 1600.0,
		}
	}

	/// Scale a desktop-tuned ([`Config::steps`]) column count down to this band (≥ 1).
	pub(crate) fn cols(self, base: u32) -> u32 {
		((base as f64 * self.design() / Self::Xl.design()).round() as u32).max(1)
	}
}

impl std::fmt::Display for Band {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Sm => "sm",
			Self::Md => "md",
			Self::Xl => "xl",
		})
	}
}
