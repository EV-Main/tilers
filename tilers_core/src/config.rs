//! Host-supplied configuration for the packed layout, passed once into
//! [`PackedState`](crate::PackedState).

use std::rc::Rc;

use crate::keys::Keymap;

#[derive(Clone, Default)]
pub struct Config {
	pub keymap: Keymap,
	/// Desktop (`Xl`) column count: how many grid steps span the container's width on a wide screen.
	/// Smaller [`Breakpoint`]s scale this down so the *physical* step stays ~constant and tiles reflow
	/// instead of shrinking (see `Breakpoint::scale_cols`). The rendered horizontal step is
	/// `container_width / cols`, so within a band the layout still stretches to fill. A finer grid
	/// (more steps) gives smaller resize/placement increments.
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
	/// fit its tab text) rather than the viewport, which the grid already handles via [`Breakpoint`].
	/// Emitted as `--tl-title-h` on the root and resolved to px for the content overlay's offset.
	/// The corner resize handle is pinned to half of it.
	pub title_h_rem: f64 = 2.0,
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
	}
}

/// Responsive width bands — Bootstrap's xs/sm/md/lg/xl boundaries (CSS px). The grid's column and
/// row counts are derived per band so the *physical* step size stays ~constant across devices: a
/// phone gets fewer steps than a desktop, so the same tiles reflow and stack down instead of
/// shrinking to illegibility. The count is fixed within a band (the grid still stretches to fill),
/// so a layout has one stable signature per band. Layouts are stored per [`Band`], not per
/// breakpoint: neighbouring bands differ only in column count, and a phone-sized arrangement is
/// worth keeping apart from a desktop one, a 16-column difference is not.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Breakpoint {
	Xs,
	Sm,
	Md,
	Lg,
	#[default]
	Xl,
}

impl Breakpoint {
	/// Classify a container width (CSS px) into its band (Bootstrap's boundaries).
	pub(crate) fn of(width: f64) -> Self {
		match width {
			w if w < 576.0 => Self::Xs,
			w if w < 768.0 => Self::Sm,
			w if w < 992.0 => Self::Md,
			w if w < 1200.0 => Self::Lg,
			_ => Self::Xl,
		}
	}

	/// Design width the band scales against — its upper edge (the next band's [`of`](Self::of)
	/// threshold), with `Xl` (open-ended) capped at 1600.
	const fn design(self) -> f64 {
		match self {
			Self::Xs => 576.0,
			Self::Sm => 768.0,
			Self::Md => 992.0,
			Self::Lg => 1200.0,
			Self::Xl => 1600.0,
		}
	}

	/// Scale a desktop-tuned ([`Config`]) column count down to this band (≥ 1). `base · design /
	/// design(Xl)` holds the horizontal step's physical px ~constant — scaling the count by the
	/// band's width gives the same step size, hence the reflow rather than a shrink. The vertical
	/// step only stretches from [`Config::row_h_rem`].
	pub(crate) fn scale_cols(self, base: u32) -> u32 {
		((base as f64 * self.design() / Self::Xl.design()).round() as u32).max(1)
	}

	pub(crate) const fn band(self) -> Band {
		match self {
			Self::Xs | Self::Sm => Band::Sm,
			Self::Md => Band::Md,
			Self::Lg | Self::Xl => Band::Xl,
		}
	}
}

/// The unit a layout is stored under: phone, tablet, desktop. Coarser than [`Breakpoint`], which
/// exists to keep the *step* physically constant and would otherwise force a separate saved layout
/// for a 200px width difference nobody rearranges tiles over.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum Band {
	Sm,
	Md,
	#[default]
	Xl,
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

impl std::fmt::Display for Breakpoint {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Xs => "xs",
			Self::Sm => "sm",
			Self::Md => "md",
			Self::Lg => "lg",
			Self::Xl => "xl",
		})
	}
}
