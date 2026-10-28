//! The packed-grid layout model. Packs fixed-size tiles onto an integer step grid:
//! panes have a starting size, snap to a step, never overlap, and leave whitespace
//! below. This is InsilicoTerminal's look (`docs/refs/insilico/`).
//!
//! Everything here is in **grid units** (1 unit = `STEP` px, owned by the render layer),
//! never pixels — so placement/resize are exact integer math, immune to sub-pixel drift.

use crate::model::{Group, GroupId, PanelId};

/// One grid unit — the minimum size/resize increment (`STEP` px each).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Step(pub u32);

impl From<Step> for u32 {
	fn from(s: Step) -> u32 {
		s.0
	}
}

/// What a panel needs to stay legible. `Steps` sizes the whole tile; `Px`/`Rem` size the panel's
/// slot below the title bar, so the engine adds the chrome it alone knows. Re-resolved against the live
/// step on every measure, so a px need holds however the view's height or font changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MinSize {
	Steps { w: Step, h: Step },
	Rem { w: f64, h: f64 },
	Px { w: f64, h: f64 },
}

impl MinSize {
	/// Whole steps, ceil'd so a length floor never rounds *down* below the content it describes.
	pub(crate) fn resolve(&self, step_w: f64, step_h: f64, rem_px: f64, title_h: f64) -> (u32, u32) {
		let (w_px, h_px) = match *self {
			MinSize::Steps { w, h } => return (w.0.max(1), h.0.max(1)),
			MinSize::Rem { w, h } => (w * rem_px, h * rem_px),
			MinSize::Px { w, h } => (w, h),
		};
		(((w_px / step_w).ceil() as u32).max(1), (((h_px + title_h) / step_h).ceil() as u32).max(1))
	}
}

/// A placed tile: its tab-group, its integer grid rect, and its floor.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Cell {
	pub group: Group,
	pub x: u32,
	pub y: u32,
	pub w: u32,
	pub h: u32,
	/// The max of the tabs' resolved [`MinSize`]s at the live step: derived, never layout.
	#[serde(skip)]
	pub min_w: u32,
	#[serde(skip)]
	pub min_h: u32,
	/// Pre-clamp width/left-edge/height, set by [`refit`](PackedGrid::refit)/[`place`](PackedGrid::place)
	/// only when the view (width) or a floor (height) forced the rect off what the tile wants. They
	/// carry the intent across the change, so the view or floor coming back restores the tile exactly
	/// (`None` once it fits again). Any explicit edit (resize/drop) clears them — the new geometry *is*
	/// the intent. `#[serde(default)]` keeps pre-field layouts loadable.
	#[serde(default)]
	pub desired_w: Option<u32>,
	#[serde(default)]
	pub desired_x: Option<u32>,
	#[serde(default)]
	pub desired_h: Option<u32>,
	/// Vertical stacking order, fixed at the last structural edit (see
	/// `reassign_ranks`). [`refit`](PackedGrid::refit) settles by this,
	/// not by the transient `y`, so collapsing tiles into a narrow band and back reproduces their
	/// exact order — a path-independent anchor the live `y` can't provide.
	#[serde(default)]
	pub rank: u32,
}

/// Whether a resize gesture is in flight: undo capture waits for it to end. The grid stays packed
/// ([`PackedGrid::assert_packed`]) in both states, since every resize step settles.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum GridState {
	#[default]
	Settled,
	Resizing,
}

#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PackedGrid {
	pub cells: Vec<Cell>,
	pub state: GridState,
	next_group_id: u64,
}

impl PackedGrid {
	pub fn mint_group_id(&mut self) -> GroupId {
		let id = GroupId(self.next_group_id);
		self.next_group_id += 1;
		id
	}

	fn locate(&self, id: GroupId) -> Option<usize> {
		self.cells.iter().position(|c| c.group.id == id)
	}

	/// Auto-pack `group` at the `pack`-chosen `(x, y)` with floor `min`. The starting size is a *want*,
	/// the min a floor — same rule as `resize`/`refit`: the floor wins, and the view caps the width.
	/// A clamped want is kept in `desired_*`, so a later widening or a lower floor restores it.
	pub fn place(&mut self, group: Group, w: u32, h: u32, min: (u32, u32), cols: u32) {
		let want_w = w.max(min.0);
		let eff_w = want_w.min(cols);
		let eff_h = h.max(min.1);
		let (x, y) = pack(&self.cells, eff_w, eff_h, cols);
		self.cells.push(Cell {
			group,
			x,
			y,
			w: eff_w,
			h: eff_h,
			min_w: min.0,
			min_h: min.1,
			desired_w: (eff_w != want_w).then_some(want_w),
			desired_x: None,
			desired_h: (eff_h != h).then_some(h),
			rank: 0,
		});
		self.reassign_ranks();
		debug_assert!(self.overlaps().is_none(), "place produced an overlap");
	}

	/// Record the current top-to-bottom stacking order onto each cell's [`rank`](Cell::rank), so a
	/// later [`refit`](Self::refit) can preserve it independently of the transient `y`. Called after
	/// every *structural* edit (place/resize/drop/close): the resulting visual order becomes the
	/// order a band change will reflow within. `(y, x)` is a total order over non-overlapping cells.
	pub(crate) fn reassign_ranks(&mut self) {
		let mut order: Vec<usize> = (0..self.cells.len()).collect();
		order.sort_by_key(|&i| (self.cells[i].y, self.cells[i].x));
		for (rank, &i) in order.iter().enumerate() {
			self.cells[i].rank = rank as u32;
		}
	}

	/// The one resize: tile `idx`'s vertical edges moved toward `left`/`right` and its height set to
	/// `h`, from this (packed) layout. An edge growing into a neighbour pushes it (see [`move_edge`]);
	/// the height grows freely and settling pushes the stack below down. A pure function of `(self, args)`, so a gesture replayed from its press layout is
	/// path-independent. Ranks are left as they were: the caller commits them with
	/// [`reassign_ranks`](Self::reassign_ranks) once the edit is final.
	pub fn resized(&self, idx: usize, left: u32, right: u32, h: u32, cols: u32) -> PackedGrid {
		self.assert_packed();
		let mut out = self.clone();
		let c = &self.cells[idx];
		move_edge(&mut out.cells, idx, false, c.x + c.w, right.min(cols));
		move_edge(&mut out.cells, idx, true, c.x, left.min(cols));
		let g = &mut out.cells[idx];
		g.h = h.max(g.min_h);
		// An explicit edit is fresh intent at this view; a tile it didn't touch keeps its pre-clamp memory.
		for (o, s) in out.cells.iter_mut().zip(&self.cells) {
			if (o.x, o.w, o.h) != (s.x, s.w, s.h) {
				(o.desired_w, o.desired_x, o.desired_h) = (None, None, None);
			}
		}
		gravity(&mut out.cells, Some(idx));
		out.assert_packed();
		out
	}

	/// Whether tile `idx`'s left edge has room to its left: off the view's edge, and no tile's right
	/// edge on it in shared rows.
	pub fn free_left(&self, idx: usize) -> bool {
		let c = &self.cells[idx];
		c.x > 0 && !self.cells.iter().any(|o| o.x + o.w == c.x && rows_meet(o, c))
	}

	/// Reflow every tile into a column count and its current floors, then settle by rank. Each tile
	/// aims for its *wanted* rect (`desired_*` if the view or a floor earlier forced it, else its
	/// current `w`/`x`/`h`), clamps that to `cols` and its floor, and re-stores any leftover want in
	/// `desired_*`. So the clamp is non-destructive: a view or floor coming back restores the tile
	/// exactly, making a measure round-trip a no-op.
	pub fn refit(&mut self, cols: u32) {
		for c in &mut self.cells {
			let want_w = c.desired_w.unwrap_or(c.w);
			let want_x = c.desired_x.unwrap_or(c.x);
			let want_h = c.desired_h.unwrap_or(c.h);
			// The floor is capped by the view: a min wider than `cols` fills it rather than spilling.
			let eff_w = want_w.clamp(c.min_w.min(cols).max(1), cols);
			let eff_x = want_x.min(cols - eff_w);
			let eff_h = want_h.max(c.min_h);
			c.desired_w = (eff_w != want_w).then_some(want_w);
			c.desired_x = (eff_x != want_x).then_some(want_x);
			c.desired_h = (eff_h != want_h).then_some(want_h);
			c.w = eff_w;
			c.x = eff_x;
			c.h = eff_h;
		}
		// Settle in the stored stacking order, not by the transient `y`: each tile rests on the skyline
		// of the lower-ranked tiles sharing its columns. `rank` is fixed until the next structural edit,
		// so this is a pure function of (rank, clamped widths, cols) — refitting to a band and back is a
		// no-op, which is exactly the round-trip property the fuzzer asserts.
		let mut order: Vec<usize> = (0..self.cells.len()).collect();
		order.sort_by_key(|&i| (self.cells[i].rank, i));
		for k in 0..order.len() {
			let i = order[k];
			let (x, w) = (self.cells[i].x, self.cells[i].w);
			self.cells[i].y = order[..k]
				.iter()
				.filter(|&&j| self.cells[j].x < x + w && x < self.cells[j].x + self.cells[j].w)
				.map(|&j| self.cells[j].y + self.cells[j].h)
				.max()
				.unwrap_or(0);
		}
		debug_assert!(self.overlaps().is_none(), "refit produced an overlap");
	}

	/// Close the active tab of a group; if that empties it, drop the tile and let the rest
	/// settle upward under `gravity` — a freed column's tiles float up as far as they can,
	/// matching insilico's pillar-removal.
	pub fn close_active(&mut self, id: GroupId) {
		let idx = self.locate(id).expect("close_active: group exists");
		let active = self.cells[idx].group.active_panel().clone();
		if self.cells[idx].group.remove_tab(&active) {
			self.cells.remove(idx);
			gravity(&mut self.cells, None);
			self.reassign_ranks();
			debug_assert!(self.overlaps().is_none(), "close left an overlap");
		}
	}

	/// First pair of overlapping tiles (by index), or `None` for a clean packing. Tile contents
	/// are positioned from these same rects, so this doubles as the content-overlap check.
	pub fn overlaps(&self) -> Option<(usize, usize)> {
		for i in 0..self.cells.len() {
			for j in i + 1..self.cells.len() {
				let (a, b) = (&self.cells[i], &self.cells[j]);
				if a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h {
					return Some((i, j));
				}
			}
		}
		None
	}

	/// First tile (by index) that floats: at `y > 0` yet resting on nothing — no other tile
	/// shares one of its columns with a bottom edge touching its top. `gravity` forbids this;
	/// a settled tile is at `y == 0` or sits on the skyline directly beneath it. A free-floating
	/// tile with a gap above it (nothing overlaps, but it should have risen) is caught here.
	pub fn unsupported(&self) -> Option<usize> {
		(0..self.cells.len()).find(|&i| {
			let c = &self.cells[i];
			c.y > 0 && !self.cells.iter().enumerate().any(|(j, o)| j != i && o.x < c.x + c.w && c.x < o.x + o.w && o.y + o.h == c.y)
		})
	}

	/// Invariant the grid always holds: no two tiles (hence no two contents) overlap, and no tile
	/// floats above the skyline.
	pub fn assert_packed(&self) {
		if let Some((i, j)) = self.overlaps() {
			panic!("overlap: {:?} vs {:?}", self.cells[i], self.cells[j]);
		}
		if let Some(i) = self.unsupported() {
			panic!("floating tile (gap above): {:?}", self.cells[i]);
		}
	}

	/// Open `panel` as a new tab in `id`'s group and activate it.
	pub fn add_tab(&mut self, id: GroupId, panel: PanelId) {
		let idx = self.locate(id).expect("add_tab: group exists");
		let group = &mut self.cells[idx].group;
		let at = group.tabs.len();
		group.insert_tab(panel, at);
	}

	/// Pure pointer → [`DropTarget`] in the packed root's px space (tile rect = `x·step …`,
	/// chrome band = `chrome` px), `x` clamped to `[0, cols−src_w]`:
	/// - hovered tile's header ⇒ [`Tab`](DropTarget::Tab) (join);
	/// - hovered tile's body ⇒ [`Displace`](DropTarget::Displace) at the row *below* that tile,
	///   so the stack under it shoves down while the hovered tile itself stays put — to displace
	///   a tile you point above it, never at it, so you can still aim its header to join;
	/// - clear of every tile ⇒ the pointer's own column/row: still [`Displace`](DropTarget::Displace) if tiles sit
	///   at/below that row in this span (so it works above and outside the tiles, past the
	///   container's edge), else [`Pack`](DropTarget::Pack) onto the skyline (empty below).
	/// `px`/`py` is the *center* of the block being dragged — it anchors the shadow's landing
	/// cell (top-left column/row = center minus half the block's own size). `mx`/`my` is the raw
	/// pointer: it decides *which* tile we join or displace, so you aim with the cursor (header to
	/// tab, body to push down) while the shadow stays centered on the block.
	pub fn resolve_target(&self, source: &DragSource, px: f64, py: f64, mx: f64, my: f64, step_w: f64, step_h: f64, chrome: f64, cols: u32, src_w: u32, src_h: u32) -> DropTarget {
		let col = (((px - src_w as f64 * step_w / 2.0) / step_w).round().max(0.0) as u32).min(cols.saturating_sub(src_w));
		for c in &self.cells {
			let (l, t) = (c.x as f64 * step_w, c.y as f64 * step_h);
			let (r, b) = ((c.x + c.w) as f64 * step_w, (c.y + c.h) as f64 * step_h);
			if mx >= l && mx < r && my >= t && my < b {
				// Aiming your own header is a join only when it can reorder — a tab torn from a
				// multi-tab group. A lone tab or a whole tile pointed at itself reads as a move,
				// so treat its header like its body and fall through to Displace (matches the
				// self-drop short-circuit in `drop`).
				let self_no_join = c.group.id == source.group()
					&& !matches!(source, DragSource::Tab { from, .. }
						if self.cells[self.locate(*from).expect("drag source group exists")].group.tabs.len() > 1);
				// Header ⇒ join; body ⇒ shadow at the row below this tile (so it stays put), but at
				// the block's own column — the x tracks the dragged block as it does in open space.
				return if my < t + chrome && !self_no_join {
					// Default to append; the render layer refines `index` from the live tab geometry.
					DropTarget::Tab {
						group: c.group.id,
						index: c.group.tabs.len(),
					}
				} else {
					DropTarget::Displace { x: col, y: c.y + c.h }
				};
			}
		}
		let row = ((py - src_h as f64 * step_h / 2.0) / step_h).round().max(0.0) as u32;
		if row < skyline(&self.cells, col, src_w) {
			DropTarget::Displace { x: col, y: row }
		} else {
			DropTarget::Pack { x: col }
		}
	}

	/// The single mutation writer for moves. Detach `source` (tear one tab → the origin
	/// group keeps the rest, pruned if emptied; a whole tile → remove its cell, capturing
	/// `(w, h, min)`), then re-home per `target`:
	/// - [`Tab`](DropTarget::Tab) → append the panel(s) into that group, then settle upward;
	/// - [`Displace`](DropTarget::Displace) → drop a cell at `(x, y)` and pin it, so everything
	///   at or below that row in its columns shoves down (the row resolves to *below* a hovered
	///   tile, so that tile stays — see [`resolve_target`](Self::resolve_target));
	/// - [`Pack`](DropTarget::Pack) → drop at `(x, skyline)` and settle.
	/// Dropping onto the source's own group is a no-op, *except* tearing one tab back into its own
	/// multi-tab group — that reorders it to `index` (remove-then-reinsert).
	pub fn drop(&mut self, source: DragSource, target: DropTarget, cols: u32) {
		if let DropTarget::Tab { group, .. } = target
			&& group == source.group()
		{
			let reorder = matches!(&source, DragSource::Tab { from, .. }
				if self.cells[self.locate(*from).expect("drop: source group exists")].group.tabs.len() > 1);
			if !reorder {
				return;
			}
		}
		let (group, w, h, min_w, min_h) = match source {
			DragSource::Tile(g) => {
				let idx = self.locate(g).expect("drop: source tile exists");
				let c = self.cells.remove(idx);
				(c.group, c.w, c.h, c.min_w, c.min_h)
			}
			DragSource::Tab { panel, from } => {
				let idx = self.locate(from).expect("drop: source group exists");
				let (w, h, min_w, min_h) = {
					let c = &self.cells[idx];
					(c.w, c.h, c.min_w, c.min_h)
				};
				if self.cells[idx].group.remove_tab(&panel) {
					self.cells.remove(idx);
				}
				(Group::new(self.mint_group_id(), panel), w, h, min_w, min_h)
			}
		};
		// Keep width bound to the view: a tile re-homed near the right edge clamps left rather
		// than spilling past `cols` (the same rule [`resize`] enforces).
		let clamp_x = |x: u32| x.min(cols.saturating_sub(w));
		match target {
			DropTarget::Tab { group: t, index } => {
				let dst = self.locate(t).expect("drop: tab target exists");
				for (at, p) in (index.min(self.cells[dst].group.tabs.len())..).zip(group.tabs) {
					self.cells[dst].group.insert_tab(p, at);
				}
				gravity(&mut self.cells, None);
			}
			DropTarget::Displace { x, y } => {
				self.cells.push(Cell {
					group,
					x: clamp_x(x),
					y,
					w,
					h,
					min_w,
					min_h,
					desired_w: None,
					desired_x: None,
					desired_h: None,
					rank: 0,
				});
				let pin = self.cells.len() - 1;
				gravity(&mut self.cells, Some(pin));
			}
			DropTarget::Pack { x } => {
				let x = clamp_x(x);
				let y = skyline(&self.cells, x, w);
				self.cells.push(Cell {
					group,
					x,
					y,
					w,
					h,
					min_w,
					min_h,
					desired_w: None,
					desired_x: None,
					desired_h: None,
					rank: 0,
				});
				gravity(&mut self.cells, None);
			}
		}
		self.reassign_ranks();
		debug_assert!(self.overlaps().is_none(), "drop produced an overlap");
	}
}

/// What the user picked up: a whole tile (drag its titlebar) or one torn-out tab (drag a
/// single tab from a multi-tab group; the origin keeps the rest).
#[derive(Clone, Debug, PartialEq)]
pub enum DragSource {
	Tile(GroupId),
	Tab { panel: PanelId, from: GroupId },
}

impl DragSource {
	/// The group the source currently lives in — used to short-circuit a drop onto itself.
	fn group(&self) -> GroupId {
		match self {
			DragSource::Tile(g) => *g,
			DragSource::Tab { from, .. } => *from,
		}
	}
}

/// Where a drag resolves to (see [`PackedGrid::resolve_target`]).
#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
	/// Join `group`, inserting the incoming tab(s) at `index` (the slot the cursor points at).
	Tab {
		group: GroupId,
		index: usize,
	},
	Displace {
		x: u32,
		y: u32,
	},
	Pack {
		x: u32,
	},
}

/// Choose `(x, y)` for a `w×h` tile: a skyline pack that grows the layout's lowest point
/// the least, leftmost on ties. For each candidate left edge `x`, the rest height `y` is
/// the max bottom (`c.y + c.h`) of cells overlapping columns `[x, x+w)`, else 0; pick the
/// `(y + h, x)`-minimal placement.
fn pack(cells: &[Cell], w: u32, h: u32, cols: u32) -> (u32, u32) {
	let mut best: Option<(u32, u32)> = None; // (x, y)
	for x in 0..=cols.saturating_sub(w) {
		let y = skyline(cells, x, w);
		best = Some(match best {
			Some(b) if (b.1 + h, b.0) <= (y + h, x) => b,
			_ => (x, y),
		});
	}
	best.unwrap_or((0, 0))
}

/// Rest height for a `w`-wide tile whose left edge is `x`: the max bottom (`c.y + c.h`) of
/// the cells overlapping columns `[x, x+w)`, else 0. The skyline both [`pack`] and
/// [`PackedGrid::drop`]'s `Pack` arm sit a new tile on.
fn skyline(cells: &[Cell], x: u32, w: u32) -> u32 {
	cells.iter().filter(|c| c.x < x + w && x < c.x + c.w).map(|c| c.y + c.h).max().unwrap_or(0)
}

fn rows_meet(a: &Cell, b: &Cell) -> bool {
	a.y < b.y + b.h && b.y < a.y + a.h
}

/// Move `cells[seed]`'s right edge (its left one if `left`) from column `from` toward `to`. Only that
/// tile is taken: growing, it pushes along in front of it the near edge of every tile it reaches in
/// its rows, shrinking them; shrinking, it leaves its neighbours where they are. The edge stops where
/// the tile giving way (a pushed one, or the grabbed one itself) reaches its floor (`min(min_w, w)` at
/// the start). Rows never change.
fn move_edge(cells: &mut [Cell], seed: usize, left: bool, from: u32, to: u32) {
	let n = cells.len();
	let mut pushed = vec![false; n];
	// The column count a tile keeps with the edge at `p`: `ahead` is the side the edge faces.
	let width = |c: &Cell, ahead: bool, p: u32| if ahead == left { p - c.x } else { c.x + c.w - p };
	let floor = |c: &Cell| c.min_w.min(c.w).max(1);
	let mut p = from;
	while p != to {
		if (to < p) == left {
			let col = if left { p - 1 } else { p };
			for (i, t) in cells.iter().enumerate() {
				if i != seed && !pushed[i] && t.x <= col && col < t.x + t.w && rows_meet(t, &cells[seed]) {
					assert_eq!(if left { t.x + t.w } else { t.x }, p, "a packed layout has nothing straddling the edge in its rows: {t:?}");
					pushed[i] = true;
				}
			}
			if (0..n).any(|i| pushed[i] && width(&cells[i], true, p) <= floor(&cells[i])) {
				break;
			}
		} else if width(&cells[seed], false, p) <= floor(&cells[seed]) {
			break;
		}
		p = if to < p { p - 1 } else { p + 1 };
	}
	for (i, c) in cells.iter_mut().enumerate() {
		match (i == seed, pushed[i]) {
			(true, _) if left => (c.x, c.w) = (p, c.x + c.w - p),
			(true, _) => c.w = p - c.x,
			(false, true) if left => c.w = p - c.x,
			(false, true) => (c.x, c.w) = (p, c.x + c.w - p),
			(false, false) => {}
		}
	}
}

/// Settle the grid by pulling every tile upward until it rests on the skyline of the tiles
/// above it in its columns — gravity toward the top. Processing top-to-bottom, each tile's new
/// `y` is the max bottom of the already-placed tiles sharing any of its columns (else 0); `x`/
/// size never change. Nothing is ever left hanging: a tile with empty space above it always
/// rises into it. `pin` (the tile under an active resize grip, or a freshly dropped shadow)
/// settles by this same rule — it is *not* held in place — but it sorts **ahead** of every tile
/// at the same starting `y` (before the `x` tiebreak), so it claims its row first and all its
/// row-peers settle onto *its* skyline. That ordering is the whole point of the pin: it makes a
/// `Displace` shadow shove its row's tiles below it (rather than yielding to them) and a resize
/// grip push the stack below it down as it grows. With `pin = None` (after a close) every tile
/// is equal and the stack just compacts up into the freed space.
fn gravity(cells: &mut [Cell], pin: Option<usize>) {
	let mut order: Vec<usize> = (0..cells.len()).collect();
	order.sort_by_key(|&i| (cells[i].y, (Some(i) != pin) as u8, cells[i].x));
	for k in 0..order.len() {
		let i = order[k];
		let (x, w) = (cells[i].x, cells[i].w);
		cells[i].y = order[..k]
			.iter()
			.filter(|&&j| cells[j].x < x + w && x < cells[j].x + cells[j].w)
			.map(|&j| cells[j].y + cells[j].h)
			.max()
			.unwrap_or(0);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn cell(x: u32, y: u32, w: u32, h: u32) -> Cell {
		Cell {
			group: Group::new(GroupId(0), PanelId("p".into())),
			x,
			y,
			w,
			h,
			min_w: 1,
			min_h: 1,
			desired_w: None,
			desired_x: None,
			desired_h: None,
			rank: 0,
		}
	}

	#[test]
	fn pack_skyline() {
		assert_eq!(pack(&[], 2, 2, 6), (0, 0), "empty grid → origin");

		// A tall cell on the left. A new tile nestles into the gap *beside* it (y=0 at the
		// first free column) rather than stacking below it (y=4), leftmost free column winning.
		let tall = [cell(0, 0, 2, 4)];
		assert_eq!(pack(&tall, 2, 2, 6), (2, 0), "nestle beside the tall cell, leftmost");

		// No horizontal room (the cell spans the full width) → forced to stack below it.
		let full = [cell(0, 0, 6, 4)];
		assert_eq!(pack(&full, 2, 2, 6), (0, 4), "stack below when nothing fits beside");
	}

	fn no_overlap(cells: &[Cell]) {
		for (i, a) in cells.iter().enumerate() {
			for b in &cells[i + 1..] {
				assert!(!(a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h), "tiles overlap: {a:?} vs {b:?}");
			}
		}
	}

	#[test]
	fn resize_pushes_stack_down() {
		// a left stack of three, plus a right-column tile sharing the top row.
		let g = PackedGrid {
			cells: vec![cell(0, 0, 2, 2), cell(0, 2, 2, 2), cell(0, 4, 2, 2), cell(2, 0, 2, 2)],
			..Default::default()
		};

		// Grow the top-left tile down: the stack below it cascades down, the anchor unmoved.
		let tall = g.resized(0, 0, 2, 4, 6);
		assert_eq!((tall.cells[0].y, tall.cells[0].h), (0, 4), "anchor stays, grows");
		assert_eq!((tall.cells[1].y, tall.cells[2].y), (4, 6), "the stack below is pushed down");

		// Back to its height from the grown layout: the stack rises onto it again.
		assert_eq!(tall.resized(0, 0, 2, 2, 6).cells, g.cells, "shrinking lets the stack rise back");
	}

	#[test]
	fn an_edge_takes_only_its_tile_and_pushes_what_it_reaches() {
		// Two stacked tiles left of one tall tile: column 2 is a boundary for all three.
		let g = PackedGrid {
			cells: vec![cell(0, 0, 2, 2), cell(0, 2, 2, 2), cell(2, 0, 3, 4)],
			..Default::default()
		};
		let rects = |g: &PackedGrid| g.cells.iter().map(|c| (c.x, c.y, c.w, c.h)).collect::<Vec<_>>();

		assert_eq!(
			rects(&g.resized(0, 0, 1, 2, 6)),
			[(0, 0, 1, 2), (0, 2, 2, 2), (2, 0, 3, 4)],
			"shrinking leaves the neighbours where they are"
		);
		assert_eq!(
			rects(&g.resized(0, 0, 4, 2, 6)),
			[(0, 0, 4, 2), (0, 2, 2, 2), (4, 0, 1, 4)],
			"growing pushes the tall tile, down to its floor"
		);
		assert_eq!(rects(&g.resized(0, 0, 6, 2, 6)), rects(&g.resized(0, 0, 4, 2, 6)), "the edge stops at the floor");
		assert!(!g.free_left(2) && !g.free_left(0), "a boundary has no left grip, nor does the view's edge");
		let gap = PackedGrid {
			cells: vec![cell(0, 0, 1, 2), cell(3, 0, 2, 2)],
			..Default::default()
		};
		assert!(gap.free_left(1), "room on the left is a left grip");
		assert_eq!(
			rects(&gap.resized(1, 0, 5, 2, 6)),
			[(0, 0, 1, 2), (1, 0, 4, 2)],
			"it crosses the gap, then stops at a floor it reaches"
		);
	}

	/// Build a grid whose cells carry distinct group ids, so drops can address them by id.
	fn grid_with(cells: Vec<Cell>) -> PackedGrid {
		let mut g = PackedGrid { cells, ..Default::default() };
		for (n, c) in g.cells.iter_mut().enumerate() {
			c.group.id = GroupId(n as u64);
		}
		g.next_group_id = g.cells.len() as u64;
		g
	}

	#[test]
	fn drop_displaces_stack() {
		// left column A/B/C, plus a right-column tile D on the top row.
		let mut g = grid_with(vec![cell(0, 0, 2, 2), cell(0, 2, 2, 2), cell(0, 4, 2, 2), cell(2, 0, 2, 2)]);
		let y = |g: &PackedGrid, id: u64| g.cells.iter().find(|c| c.group.id == GroupId(id)).expect("cell present").y;

		// Drag D onto B's body: D takes (0,2); B and everything below it shove down.
		g.drop(DragSource::Tile(GroupId(3)), DropTarget::Displace { x: 0, y: 2 }, 6);
		no_overlap(&g.cells);
		assert_eq!(y(&g, 0), 0, "A untouched at the top");
		assert_eq!(y(&g, 3), 2, "D (shadow) lands on the hovered cell");
		assert_eq!(y(&g, 1), 4, "B shoved below the shadow");
		assert_eq!(y(&g, 2), 6, "C follows B down");
	}

	#[test]
	fn drop_tears_tab_into_new_tile() {
		// one 2-tab group occupying the top-left.
		let mut g = grid_with(vec![cell(0, 0, 2, 2)]);
		g.cells[0].group.insert_tab(PanelId("q".into()), 1);
		assert_eq!(g.cells[0].group.tabs.len(), 2);

		// tear "q" out to an empty column → a fresh tile inheriting (w,h); origin keeps "p".
		g.drop(
			DragSource::Tab {
				panel: PanelId("q".into()),
				from: GroupId(0),
			},
			DropTarget::Pack { x: 3 },
			6,
		);
		no_overlap(&g.cells);
		assert_eq!(g.cells.len(), 2, "torn tab forms a second tile");
		let origin = g.cells.iter().find(|c| c.group.id == GroupId(0)).expect("origin survives");
		assert_eq!(origin.group.tabs, vec![PanelId("p".into())], "origin keeps the remainder");
		let torn = g.cells.iter().find(|c| c.group.tabs.contains(&PanelId("q".into()))).expect("torn tile");
		assert_eq!((torn.x, torn.w, torn.h), (3, 2, 2), "new tile at the column, inheriting size");
	}

	#[test]
	fn drop_packs_at_column() {
		// a wide tile across columns 0..4, and a small tile to repack.
		let mut g = grid_with(vec![cell(0, 0, 4, 2), cell(4, 0, 2, 2)]);
		// pack the small tile (id 1) onto column 0 → it sits below the wide tile's skyline.
		g.drop(DragSource::Tile(GroupId(1)), DropTarget::Pack { x: 0 }, 6);
		no_overlap(&g.cells);
		let c = g.cells.iter().find(|c| c.group.id == GroupId(1)).expect("cell present");
		assert_eq!((c.x, c.y), (0, 2), "packed at the hovered column, on its skyline");
	}

	#[test]
	fn refit_clamps_into_narrower_band() {
		// A layout built on a 12-col band: a right-edge tile reaching col 12.
		let mut g = PackedGrid {
			cells: vec![cell(0, 0, 4, 2), cell(8, 0, 4, 2)],
			..Default::default()
		};

		g.refit(6);
		no_overlap(&g.cells);
		for c in &g.cells {
			assert!(c.x + c.w <= 6, "tile spills past the narrowed band: {c:?}");
		}
	}

	#[test]
	fn close_compacts_upward() {
		let mut g = PackedGrid::default();
		// left column: A on top, B below it (the "pillar" we close), C below B.
		// right column: D, with nothing above it.
		let mut cells = vec![cell(0, 0, 2, 2), cell(0, 2, 2, 2), cell(0, 4, 2, 2), cell(2, 4, 2, 2)];
		for (n, c) in cells.iter_mut().enumerate() {
			c.group.id = GroupId(n as u64);
		}
		g.cells = cells;

		g.close_active(GroupId(1));
		no_overlap(&g.cells);
		let y = |id: u64| g.cells.iter().find(|c| c.group.id == GroupId(id)).expect("cell present").y;
		assert_eq!(y(2), 2, "C rises only as far as the tile above it in its column");
		assert_eq!(y(3), 0, "D's column is free, so it floats all the way up");
	}
}
