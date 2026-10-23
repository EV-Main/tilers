//! The gesture reducer: every field the old Signal-driven renderer split across ~14 reactive
//! cells lives here on one [`PackedState`] as a plain field, and every event handler / effect is
//! a `&mut self` method whose inputs are already-extracted primitives (the binding does the DOM
//! read, passes plain numbers/strings in). So the whole drag/resize/undo/fit machine is ordinary
//! Rust — unit-testable without a DOM, and shared verbatim by the Dioxus and Leptos bindings.
//!
//! The imperative host API the old `PackedApi` exposed ([`place`](PackedState::place) /
//! [`save`](PackedState::save) / …) folds into methods here too; a binding wraps a mutable
//! accessor to its reactive cell to hand the host something to drive.
//!
//! View-model getters ([`frames`](PackedState::frames), [`content_slots`](PackedState::content_slots),
//! [`ghost`](PackedState::ghost), …) return plain data — ids and inline-style strings — that a
//! template renders 1:1, so all the `rsx!`/`view!`-adjacent computation lives here once.

use std::collections::{HashMap, HashSet};

use crate::{
	config::{Band, Breakpoint, Config},
	keys::{Builtin, Command, Help, Hint, KeyInput, KeyOutcome, Step, Trie},
	model::{
		Group, GroupId, PanelId,
		packed::{DragSource, DropTarget, GridState, MinSize, PackedGrid},
		serial,
	},
	persist,
};

/// Root font size assumed before the first measure, and on a non-DOM target (the fuzzer). Every
/// browser's default.
const REM_PX: f64 = 16.0;
/// A press promotes to a drag only past this many px, so a click still activates a tab.
const DRAG_THRESHOLD: f64 = 4.0;

/// Everything the packed layout needs to render and mutate itself, on one struct. The engine's
/// grid plus the live view geometry (`cols`/`step_px`/`breakpoint`/`root_*`, set by the measure),
/// the in-flight gestures (`drag`/`resize`), the keyboard-driven pane state
/// (`focused`/`maximized`/`help`/`popups`/`pending`/`notice`), the undo history, the `shown` band latch, and
/// the host `cfg`.
pub struct PackedState {
	grid: PackedGrid,
	cols: u32,
	/// Live px-per-step on each axis (`width / cols`, `Config::row_h_rem`); `(0, 0)` until first measure.
	step_px: (f64, f64),
	/// The document's actual root font size, re-read at every measure. Bridges rem ⇄ px for the
	/// chrome height and a rem-expressed [`MinSize`].
	rem_px: f64,
	/// Every panel's declared min, mirrored from the binding's panel list. Host intent, not layout:
	/// cell floors are re-derived from it ([`refloor`](Self::refloor)) whenever the step or a tile's tabs change.
	mins: HashMap<PanelId, MinSize>,
	breakpoint: Breakpoint,
	drag: Option<Drag>,
	resize: Option<ResizeStart>,
	root_origin: (f64, f64),
	root_size: (f64, f64),
	/// The root's scroll offset: the tiles live in its scrolled content, the pointer in client px.
	scroll: f64,
	/// The view's edges may lie under a host's grips (see [`framed`]).
	framed: bool,
	/// The pane keyboard ops act on: set when a tile's header/tab/body is pressed. `maximized` is a
	/// pure view toggle (no model mutation) — the focused tile fills the container, the rest hide.
	focused: Option<GroupId>,
	maximized: Option<GroupId>,
	help: bool,
	popups: HashSet<GroupId>,
	/// `cfg.keymap` resolved for this platform and config.
	keys: Trie,
	/// The path into `keys` a chord has opened; empty when none is.
	pending: Vec<usize>,
	notice: Option<Notice>,
	notices: u32,
	undo: UndoHistory,
	/// The [`Band`] whose layout is currently on screen — `None` until the first real measure, and
	/// reset to the new band by [`take_band`](PackedState::take_band) on every crossing.
	shown: Option<Band>,
	/// This instance's `.tl-packed` element — DOM reads stay inside it, never document-wide.
	#[cfg(target_arch = "wasm32")]
	root: Option<web_sys::Element>,
	cfg: Config,
}

impl PackedState {
	pub fn new(cfg: Config) -> Self {
		let cols = cfg.steps.max(1);
		#[cfg(target_arch = "wasm32")]
		let mac = {
			let p = web_sys::window().expect("a browser window").navigator().platform().expect("navigator.platform never throws");
			// iPadOS composes on Option too, and reports `iPad`/`MacIntel`.
			p.starts_with("Mac") || p.starts_with("iP")
		};
		#[cfg(not(target_arch = "wasm32"))]
		let mac = false;
		let keys = Trie::new(&cfg.keymap, mac, |b| match b {
			Builtin::Cache => cfg.storage_key.is_some(),
			Builtin::Publish => cfg.on_publish.is_some(),
			_ => true,
		});
		Self {
			grid: PackedGrid::default(),
			cols,
			step_px: (0.0, 0.0),
			rem_px: REM_PX,
			mins: HashMap::new(),
			breakpoint: Breakpoint::default(),
			drag: None,
			resize: None,
			root_origin: (0.0, 0.0),
			root_size: (0.0, 0.0),
			scroll: 0.0,
			framed: false,
			focused: None,
			maximized: None,
			help: false,
			popups: HashSet::new(),
			keys,
			pending: Vec::new(),
			notice: None,
			notices: 0,
			undo: UndoHistory::default(),
			shown: None,
			#[cfg(target_arch = "wasm32")]
			root: None,
			cfg,
		}
	}

	#[cfg(target_arch = "wasm32")]
	pub fn set_root(&mut self, el: web_sys::Element) {
		self.root = Some(el);
	}

	/// Insertion slot for cursor x `mx` among `group`'s tabs in the live preview DOM: the count of
	/// *real* tabs whose horizontal center is left of `mx` (so left-half ⇒ before, right-half ⇒ after).
	/// `dragged` are the panel ids the source carries — the preview already inserts them as the floating
	/// ghost's tab(s), so they're skipped, leaving the source-free order this index addresses (identical
	/// math whether re-homing into another group or reordering within one). The one place this layer
	/// measures, because the model has no tab widths; `None` on a non-wasm target, caller keeps the
	/// append default.
	#[cfg(target_arch = "wasm32")]
	fn tab_drop_index(&self, mx: f64, group: u64, dragged: &[String]) -> Option<usize> {
		use wasm_bindgen::JsCast;
		let root = self.root.as_ref().expect("binding sets the root at mount, before any drag");
		let nodes = root.query_selector_all(&format!("[data-dvg=\"{group}\"] .tl-tab")).expect("numeric-only selector");
		let mut idx = 0;
		for i in 0..nodes.length() {
			let el = nodes.get(i).expect("index below length").dyn_into::<web_sys::Element>().expect("selector matches elements");
			// Every `.tl-tab` carries `data-panel`; default "" just classes a (nonexistent) attr-less tab as real.
			if dragged.iter().any(|p| *p == el.get_attribute("data-panel").unwrap_or_default()) {
				continue;
			}
			let r = el.get_bounding_client_rect();
			if mx > r.x() + r.width() / 2.0 {
				idx += 1;
			}
		}
		Some(idx)
	}

	#[cfg(not(target_arch = "wasm32"))]
	fn tab_drop_index(&self, _: f64, _: u64, _: &[String]) -> Option<usize> {
		None
	}

	// ------- imperative host API (folded from the old `PackedApi`) -------

	/// Auto-pack `group` at a wanted `w×h`, floored by its tabs' declared mins.
	pub fn place(&mut self, group: Group, w: u32, h: u32) {
		assert!(self.step_px.0 > 0.0, "place before the first measure: no step size yet");
		let (floor, cols) = (self.floor(&group.tabs), self.cols);
		self.base().place(group, w, h, floor, cols);
		self.follow();
	}

	pub fn add_tab(&mut self, group: GroupId, panel: PanelId) {
		self.base().add_tab(group, panel);
		self.refloor();
	}

	pub fn close_active(&mut self, group: GroupId) {
		self.base().close_active(group);
		self.refloor();
	}

	/// The layout an edit lands on: while a resize is in flight the live grid is derived from its
	/// press layout, so the edit goes there and [`follow`](Self::follow) re-derives over it.
	fn base(&mut self) -> &mut PackedGrid {
		match &mut self.resize {
			Some(r) => &mut r.press,
			None => &mut self.grid,
		}
	}

	/// Re-derive the live grid from the press layout at the last cursor, so a gesture stays a function
	/// of (press layout, grip, cursor) whatever edits landed on that layout meanwhile.
	fn follow(&mut self) {
		let Some(r) = &self.resize else { return };
		let Some(idx) = r.press.cells.iter().position(|c| c.group.id == r.gid) else {
			// The grabbed tile was closed under the gesture: there is nothing left to resize.
			self.grid = self.resize.take().expect("matched above").press;
			return;
		};
		let (sw, sh) = self.step_px;
		let shift = |at: u32, from: f64, to: f64, step: f64| (at as i64 + ((to - from) / step).round() as i64).max(0) as u32;
		let c = &r.press.cells[idx];
		let (mut left, mut right, mut h) = (c.x, c.x + c.w, c.h);
		match r.grip {
			Grip::Left => left = shift(left, r.press_at.0, r.cursor.0, sw),
			Grip::Right | Grip::Wall => right = shift(right, r.press_at.0, r.cursor.0, sw),
			Grip::Bottom => h = shift(h, r.press_at.1, r.cursor.1, sh),
			// Above the fold the tile's bottom is where it shows: pulling up brings it to the pointer.
			Grip::Fold if shift(h, r.press_at.1, r.cursor.1, sh) < h => {
				let row = ((r.cursor.1 - self.root_origin.1 + self.scroll) / sh).round() as i64;
				h = (row - c.y as i64).clamp(1, h as i64) as u32;
			}
			Grip::Fold => {}
			Grip::Corner => (right, h) = (shift(right, r.press_at.0, r.cursor.0, sw), shift(h, r.press_at.1, r.cursor.1, sh)),
		}
		self.grid = r.press.resized(idx, left, right, h.max(1), self.cols);
		self.grid.state = GridState::Resizing;
	}

	/// Replace every panel's declared min — the binding mirrors its panel list here on each change.
	pub fn set_mins(&mut self, mins: HashMap<PanelId, MinSize>) {
		self.mins = mins;
		self.refloor();
	}

	/// The floor a tile hosting `tabs` gets at the live step: the max of their resolved mins.
	fn floor(&self, tabs: &[PanelId]) -> (u32, u32) {
		let (sw, sh) = self.step_px;
		tabs.iter()
			.map(|p| {
				self.mins
					.get(p)
					.unwrap_or_else(|| panic!("hosted panel {p:?} has no declared min: the binding's panels list every panel the layout hosts"))
					.resolve(sw, sh, self.rem_px, self.title_h())
			})
			.fold((0, 0), |(w, h), (pw, ph)| (w.max(pw), h.max(ph)))
	}

	/// Re-derive every cell's floor at the live step, then reflow onto it. A no-op before the first
	/// measure: there is no step to resolve against, and `set_measure` refloors once there is.
	fn refloor(&mut self) {
		if self.step_px.0 <= 0.0 {
			return;
		}
		let base = self.resize.as_ref().map_or(&self.grid, |r| &r.press);
		let floors: Vec<(u32, u32)> = base.cells.iter().map(|c| self.floor(&c.group.tabs)).collect();
		let cols = self.cols;
		let base = self.base();
		for (c, (w, h)) in base.cells.iter_mut().zip(floors) {
			(c.min_w, c.min_h) = (w, h);
		}
		base.refit(cols);
		self.follow();
	}

	/// Tile `idx`'s right edge to `x + new_w` and its height to `new_h`: the grips' resize, at once.
	pub fn resize(&mut self, idx: usize, new_w: u32, new_h: u32) {
		let cols = self.cols;
		let base = self.base();
		let c = &base.cells[idx];
		*base = base.resized(idx, c.x, c.x + new_w, new_h, cols);
		base.reassign_ranks();
		self.follow();
	}

	/// Serialize the current layout (see [`serial`]).
	pub fn save(&self) -> String {
		serial::save(&self.grid)
	}

	/// Replace the layout with a previously [`save`](Self::save)d one; errors (not a silent reset)
	/// on a corrupt or future-version payload. The file may have been saved in another band, so it
	/// is settled into the live geometry — every panel it hosts must already have a declared min.
	pub fn load(&mut self, json: &str) -> Result<(), serial::LoadError> {
		self.grid = serial::load(json)?;
		self.drag = None;
		self.resize = None;
		self.refloor();
		Ok(())
	}

	pub fn mint_group_id(&mut self) -> GroupId {
		let id = self.base().mint_group_id();
		self.follow();
		id
	}

	pub fn cols(&self) -> u32 {
		self.cols
	}

	/// Whole rows the container shows: what a tile placed at the top spans to fill its height.
	pub fn rows(&self) -> u32 {
		assert!(self.step_px.1 > 0.0, "rows before the first measure: no step size yet");
		(self.root_size.1 / self.step_px.1) as u32
	}

	/// The band the layout is stored under — coarser than the [`Breakpoint`] driving `cols`.
	pub fn band(&self) -> Band {
		self.breakpoint.band()
	}

	/// Title-bar height in px — the one resolution of [`Config::title_h_rem`], used for the content
	/// overlay's chrome offset and the header's drop hit-test. The stylesheet sizes the same bar from
	/// the `--tl-title-h` the binding emits, so they agree by construction.
	pub fn title_h(&self) -> f64 {
		self.cfg.title_h_rem * self.rem_px
	}

	/// Read access to the live grid — for a host rebuilding its panel list from a restored layout.
	pub fn grid(&self) -> &PackedGrid {
		&self.grid
	}

	/// Wipe the layout back to empty (a host re-seeding a fresh band). Any gesture in flight goes
	/// with it: `drag`/`resize` hold cell *indices*, which address nothing once the cells are gone.
	pub fn reset(&mut self) {
		self.grid = PackedGrid::default();
		self.drag = None;
		self.resize = None;
	}

	// ------- effects, folded to methods -------

	/// Store the root's origin (to map pointer→grid space) and its content box, then reflow the
	/// grid to fill it: classify the width into a responsive band, stretch `cols` across the
	/// container's width (a horizontal step is `width / cols`, cols scaled per band; a vertical one is
	/// [`Config::row_h_rem`]), and refloor tiles onto the new step. `framed`: see [`framed`].
	pub fn set_measure(&mut self, origin: (f64, f64), size: (f64, f64), framed: bool) {
		self.root_origin = origin;
		self.root_size = size;
		self.framed = framed;
		self.rem_px = root_font_px();
		let (width, height) = size;
		if width <= 0.0 || height <= 0.0 {
			return;
		}
		let steps = self.cfg.steps.max(1);
		let bp = Breakpoint::of(width);
		let c = bp.scale_cols(steps);
		self.breakpoint = bp;
		self.cols = c;
		self.step_px = (width / c as f64, self.cfg.row_h_rem * self.rem_px);
		self.refloor();
	}

	pub fn set_scroll(&mut self, y: f64) {
		self.scroll = y;
	}

	/// Whether [`capture_undo`](Self::capture_undo) would actually push. A binding that captures via
	/// a single-signal effect reads this first (to subscribe) and only writes when it's `true`, so
	/// the resulting write doesn't re-fire the effect into a loop.
	pub fn wants_undo_snapshot(&self) -> bool {
		self.drag.is_none() && self.grid.state == GridState::Settled && !self.grid.cells.is_empty() && self.undo.states.get(self.undo.cursor) != Some(&self.grid)
	}

	/// Snapshot a *solid* layout into the undo history: only when the grid is at rest — no drag in
	/// flight, not mid-resize — and differs from the cursor's snapshot. `cells.is_empty()` skips
	/// the default grid before the host's seed lands, so undo can't walk back to a blank layout.
	pub fn capture_undo(&mut self) {
		if self.drag.is_some() {
			return;
		}
		if self.grid.state != GridState::Settled || self.grid.cells.is_empty() {
			return;
		}
		if self.undo.states.get(self.undo.cursor) == Some(&self.grid) {
			return;
		}
		self.undo.push(self.grid.clone());
	}

	/// Band-resolution latch: `Some(restored)` the first time a real step size lands, and again on
	/// every band change. `true` ⇒ this band's cached layout was restored from `localStorage`;
	/// `false` ⇒ nothing usable was cached and the grid has been reset — the host must place its
	/// tiles (or load its own default). Called by the binding after every measure; it then invokes
	/// the host's `on_band` hook, and only then [`set_mins`](Self::set_mins): a restored layout may
	/// host panels the host only lists once `on_band` has rebuilt them, so its floors wait for that.
	pub fn take_band(&mut self) -> Option<bool> {
		let band = self.band();
		if self.step_px.0 <= 0.0 || self.shown == Some(band) {
			return None;
		}
		self.shown = Some(band);
		self.reset();
		let Some(key) = self.cfg.storage_key.as_ref().map(|k| format!("{k}-{band}")) else {
			return Some(false);
		};
		let Some(json) = persist::read(&key) else { return Some(false) };
		match serial::load(&json) {
			// An empty payload parses fine and renders a blank dock, which reads as a broken app rather
			// than as a cache to fall past — so it is a miss, like a corrupt one.
			Ok(grid) if grid.cells.is_empty() => {
				report(&format!("tilers: cached layout at {key:?} is empty; re-seeding"));
				Some(false)
			}
			Ok(grid) => {
				self.grid = grid;
				Some(true)
			}
			Err(e) => {
				report(&format!("tilers: cached layout at {key:?} is unusable ({e:?}); re-seeding"));
				Some(false)
			}
		}
	}

	// ------- gestures -------

	/// Pick up a whole tile by its header (empty area): arm a `Drag` and focus it. `start`/`grab`
	/// are client px + the pointer's offset within the header.
	pub fn press_tile(&mut self, gid: GroupId, start: (f64, f64), grab: (f64, f64)) {
		if self.resize.is_some() {
			return; // one gesture at a time: a drag would drop the cells the resize addresses
		}
		self.focus(gid);
		let (w, h) = self.grid.cells.iter().find(|c| c.group.id == gid).map(|c| (c.w, c.h)).expect("press_tile: group exists");
		self.drag = Some(Drag {
			source: DragSource::Tile(gid),
			src_w: w,
			src_h: h,
			start,
			grab,
			cursor: start,
			armed: false,
			target: None,
		});
	}

	/// Tear a tab out of its group: arm a `Drag` and focus the origin.
	pub fn press_tab(&mut self, panel: PanelId, from: GroupId, start: (f64, f64), grab: (f64, f64)) {
		if self.resize.is_some() {
			return; // one gesture at a time: a drag would drop the cells the resize addresses
		}
		self.focus(from);
		let (w, h) = self.grid.cells.iter().find(|c| c.group.id == from).map(|c| (c.w, c.h)).expect("press_tab: group exists");
		self.drag = Some(Drag {
			source: DragSource::Tab { panel, from },
			src_w: w,
			src_h: h,
			start,
			grab,
			cursor: start,
			armed: false,
			target: None,
		});
	}

	/// Advance an in-flight drag: arm past the `DRAG_THRESHOLD`, then resolve the live
	/// [`DropTarget`], refining a `Tab` slot from the preview's tab geometry (via `tab_drop_index`,
	/// a no-op without a DOM).
	pub fn drag_move(&mut self, cursor: (f64, f64)) {
		let Some(mut d) = self.drag.clone() else { return };
		d.cursor = cursor;
		if !d.armed {
			let (dx, dy) = (cursor.0 - d.start.0, cursor.1 - d.start.1);
			// Below the threshold the ghost isn't shown yet, so the discarded cursor update is moot.
			if (dx * dx + dy * dy).sqrt() <= DRAG_THRESHOLD {
				return;
			}
			d.armed = true;
		}
		let (ox, oy) = self.root_origin;
		let (sw, sh) = self.step_px;
		// Reference the moving block's center (ghost top-left + half its size), not the raw pointer —
		// the cell the block visibly covers is where it lands.
		let cx = cursor.0 - d.grab.0 + d.src_w as f64 * sw / 2.0;
		let cy = cursor.1 - d.grab.1 + d.src_h as f64 * sh / 2.0;
		let mut t = self.grid.resolve_target(
			&d.source,
			cx - ox,
			cy - oy + self.scroll,
			cursor.0 - ox,
			cursor.1 - oy + self.scroll,
			sw,
			sh,
			self.title_h(),
			self.cols,
			d.src_w,
			d.src_h,
		);
		// The model can only append (it has no tab geometry); refine the slot from the live preview's
		// tab rects, skipping the ghost's own tab(s) so we read the source-free order.
		if let DropTarget::Tab { group, index } = t {
			let dragged: Vec<String> = match &d.source {
				DragSource::Tab { panel, .. } => vec![panel.0.clone()],
				DragSource::Tile(g) => self
					.grid
					.cells
					.iter()
					.find(|c| c.group.id == *g)
					.expect("drag source group exists")
					.group
					.tabs
					.iter()
					.map(|p| p.0.clone())
					.collect(),
			};
			t = DropTarget::Tab {
				group,
				index: self.tab_drop_index(cursor.0, group.0, &dragged).unwrap_or(index),
			};
		}
		d.target = Some(t);
		self.drag = Some(d);
	}

	/// Commit a drag: an armed release runs the same [`drop`](PackedGrid::drop) the preview used; an
	/// unarmed tab release is a plain click that just activates that tab.
	pub fn drag_release(&mut self) {
		let Some(d) = self.drag.take() else { return };
		if d.armed {
			if let Some(t) = d.target {
				self.grid.drop(d.source, t, self.cols);
				self.refloor();
			}
		} else if let DragSource::Tab { panel, from } = &d.source
			&& let Some(c) = self.grid.cells.iter_mut().find(|c| c.group.id == *from)
			&& let Some(i) = c.group.tabs.iter().position(|p| p == panel)
		{
			c.group.active = i;
		}
	}

	pub fn drag_cancel(&mut self) {
		self.drag = None;
	}

	/// Begin a resize of tile `idx` from `grip` at the pointer `(px, py)`.
	pub fn resize_start(&mut self, idx: usize, grip: Grip, px: f64, py: f64) {
		if self.drag.is_some() {
			return; // one gesture at a time
		}
		self.resize_end(); // a second pointer on another grip: the first gesture ends where it is
		assert!(grip != Grip::Left || self.grid.free_left(idx), "a left grip on a boundary: bindings render `Frame::grips`");
		self.resize = Some(ResizeStart {
			gid: self.grid.cells[idx].group.id,
			grip,
			press: self.grid.clone(),
			press_at: (px, py),
			cursor: (px, py),
		});
		self.grid.state = GridState::Resizing;
	}

	pub fn resize_move(&mut self, cursor: (f64, f64)) {
		let Some(r) = &mut self.resize else { return };
		r.cursor = cursor;
		self.follow();
	}

	pub fn resize_end(&mut self) {
		if self.resize.take().is_none() {
			return;
		}
		self.grid.state = GridState::Settled;
		self.grid.reassign_ranks();
	}

	/// A body/tab press focuses the pane so keybinds act on whichever pane you last touched. A press
	/// is the user moving on, so it drops a pending chord.
	pub fn focus(&mut self, gid: GroupId) {
		self.focused = Some(gid);
		self.pending.clear();
	}

	/// `focused`/`maximized` filtered to a group the layout still holds. They're set by a press and
	/// never unset by one, but a group can vanish without any event to hear it on — a drag can tear
	/// the focused pane's last tab into a fresh group, or re-home it into another. Resolving liveness
	/// at every read is why nothing downstream has to remember to clear them.
	fn live(&self, gid: Option<GroupId>) -> Option<GroupId> {
		gid.filter(|g| self.grid.cells.iter().any(|c| c.group.id == *g))
	}

	/// Walk the undo history by `dir` and settle whatever comes back into the live geometry. A
	/// snapshot may have been taken at another step, so it needs the same refloor a
	/// [`load`](Self::load)ed layout does — restoring one verbatim would spill past `cols` or its floors.
	fn restore(&mut self, dir: i32) -> bool {
		let Some(g) = self.undo.step(dir) else { return false };
		self.grid = g;
		self.refloor();
		true
	}

	pub fn close_help(&mut self) {
		self.help = false;
	}

	/// The per-tile `?`: the help, aimed at that pane.
	pub fn open_help(&mut self, gid: GroupId) {
		self.focus(gid);
		self.help = true;
	}

	/// The window lost focus: the keys that would finish the chord go elsewhere.
	pub fn cancel_chord(&mut self) {
		self.pending.clear();
	}

	// ------- view flags -------

	pub fn dragging(&self) -> bool {
		self.drag.is_some()
	}

	/// The grip of the resize in flight.
	pub fn resizing(&self) -> Option<Grip> {
		self.resize.as_ref().map(|r| r.grip)
	}

	// ------- keyboard -------

	/// One keydown. Esc is hard-wired: it closes the help, the inspect popups and a pending chord.
	pub fn on_key(&mut self, k: KeyInput) -> KeyOutcome {
		let was_pending = !self.pending.is_empty();
		if k.editable {
			self.pending.clear();
			return KeyOutcome {
				prevent_default: false,
				changed: was_pending,
			};
		}
		let Some(key) = k.normalized() else {
			return KeyOutcome {
				prevent_default: false,
				changed: false,
			};
		};
		if key == "Escape" && (self.help || !self.popups.is_empty() || was_pending) {
			self.help = false;
			self.popups.clear();
			self.pending.clear();
			return KeyOutcome {
				prevent_default: true,
				changed: true,
			};
		}
		let (cmd, seq) = match self.keys.step(&mut self.pending, &k, &key) {
			Step::Opened => {
				self.notice = None; // the hint takes its corner
				return KeyOutcome {
					prevent_default: true,
					changed: true,
				};
			}
			Step::Miss => {
				return KeyOutcome {
					prevent_default: false,
					changed: was_pending,
				};
			}
			Step::Run(cmd, seq) => (cmd, seq),
		};
		// Logged on recognition, before the command runs: a console line with no visible effect means
		// the command had nothing to act on, not that the keydown went unseen.
		#[cfg(target_arch = "wasm32")]
		web_sys::console::debug_1(&format!("tilers keybind: {seq} → {}", cmd.label()).into());
		#[cfg(not(target_arch = "wasm32"))]
		let _ = seq; // the console is the only consumer
		let acted = self.run(cmd);
		KeyOutcome {
			prevent_default: acted,
			changed: acted || was_pending,
		}
	}

	/// Whether `cmd` acted. Commands that replace or drop cells wait out a gesture: `drag` and
	/// `resize` address cells that would vanish under them.
	fn run(&mut self, cmd: Command) -> bool {
		let gesture = self.drag.is_some() || self.resize.is_some();
		let b = match cmd {
			Command::Builtin(b) => b,
			Command::Custom { run, .. } => {
				run(self);
				return true;
			}
		};
		match b {
			Builtin::Undo => !gesture && self.restore(-1),
			Builtin::Redo => !gesture && self.restore(1),
			Builtin::Close => match self.live(self.focused) {
				Some(g) if !gesture => {
					self.close_active(g);
					true
				}
				_ => false,
			},
			Builtin::Maximize => match self.live(self.focused) {
				Some(f) => {
					self.maximized = if self.maximized == Some(f) { None } else { Some(f) };
					true
				}
				None => false,
			},
			Builtin::Inspect => match self.live(self.maximized).or(self.live(self.focused)) {
				Some(g) => {
					if !self.popups.remove(&g) {
						self.popups.insert(g);
					}
					true
				}
				None => false,
			},
			Builtin::Help => {
				self.help = !self.help;
				true
			}
			Builtin::Cache => {
				let band = self.band();
				let k = self.cfg.storage_key.as_ref().expect("Cache resolves only with a storage key");
				persist::write(&format!("{k}-{band}"), &self.save());
				self.notify(format!("Layout cached in this browser ({band})"));
				true
			}
			Builtin::Publish => {
				if self.cfg.storage_key.is_some() {
					self.run(Command::Builtin(Builtin::Cache));
				}
				let hook = self.cfg.on_publish.clone().expect("Publish resolves only with on_publish");
				let (band, json) = (self.band(), self.save());
				// after this keydown releases the state, so the hook may `notify` back into it
				#[cfg(target_arch = "wasm32")]
				{
					use wasm_bindgen::{JsCast, closure::Closure};
					let run = Closure::once_into_js(move || hook(band, json));
					web_sys::window().expect("a browser window").queue_microtask(run.unchecked_ref());
				}
				#[cfg(not(target_arch = "wasm32"))]
				hook(band, json);
				true
			}
		}
	}

	// ------- view-model getters -------

	/// The grid the skeleton/content render: the live preview while an armed drag is in flight
	/// (other tiles settled under a cloned `drop`), else the real grid. One `drop` path.
	fn preview(&self) -> PackedGrid {
		let g = self.grid.clone();
		match &self.drag {
			Some(d) if d.armed => match &d.target {
				Some(t) => {
					let mut p = g;
					p.drop(d.source.clone(), t.clone(), self.cols);
					p
				}
				None => g,
			},
			_ => g,
		}
	}

	/// The tile skeleton: one [`Frame`] per rendered cell (a maximized pane hides the rest), in the
	/// preview's order. `titles` maps panel ids to their tab labels (owned by the binding).
	pub fn frames(&self, titles: &HashMap<PanelId, String>) -> Vec<Frame> {
		let g = self.preview();
		let (sw, sh) = self.step_px;
		let maximized = self.live(self.maximized);
		let mut out = Vec::new();
		for (idx, cell) in g.cells.iter().enumerate() {
			let gid = cell.group.id;
			// Maximize is a view-only override: the focused tile fills the container, others omitted.
			if let Some(mg) = maximized
				&& mg != gid
			{
				continue;
			}
			let style = if maximized == Some(gid) {
				"left:0; top:0; width:100%; height:100%;".to_string()
			} else {
				format!(
					"left:{}px; top:{}px; width:{}px; height:{}px;",
					cell.x as f64 * sw,
					cell.y as f64 * sh,
					cell.w as f64 * sw,
					cell.h as f64 * sh
				)
			};
			let (shadow, tab_drop) = self.drop_flags(gid, cell);
			if shadow {
				// The drag's landing cell: a detail-less grey placeholder, no chrome, no content.
				out.push(Frame {
					gid,
					idx,
					style,
					shadow: true,
					tab_drop: false,
					tabs: Vec::new(),
					grips: Vec::new(),
				});
				continue;
			}
			let tabs = Tab::strip(&cell.group, titles);
			out.push(Frame {
				gid,
				idx,
				style,
				shadow: false,
				tab_drop,
				tabs,
				grips: self.grips(&g, idx, maximized == Some(gid)),
			});
		}
		out
	}

	/// Tile `idx`'s grips in paint order (the corner last, over the edges' ends) with their inline
	/// styles. A boundary between two tiles is the left one's right grip alone, so a left grip exists
	/// only on a free left edge. In a [`framed`] view, a grip that would lie on the view's own edge,
	/// under a host's grip, keeps clear of it: the right one of a tile
	/// on the view's right edge is a [`Grip::Wall`], the bottom one of a tile reaching that band a
	/// [`Grip::Fold`], which the stylesheet places from `--tl-fold`, the view's bottom
	/// below the tile's top.
	fn grips(&self, g: &PackedGrid, idx: usize, maximized: bool) -> Vec<(Grip, String)> {
		let c = &g.cells[idx];
		let sh = self.step_px.1;
		let view_bottom = self.scroll + self.root_size.1;
		let (top, bottom) = (c.y as f64 * sh, (c.y + c.h) as f64 * sh);
		let grip = self.title_h() / 8.0; // the stylesheet's edge-grip thickness
		// A host's grip is guessed at most twice ours: its root font may be larger.
		let folded = self.framed && !maximized && top < view_bottom && bottom > view_bottom - 2.0 * grip;
		[
			g.free_left(idx).then_some((Grip::Left, String::new())),
			Some((if self.framed && !maximized && c.x + c.w == self.cols { Grip::Wall } else { Grip::Right }, String::new())),
			Some(if folded { (Grip::Fold, format!("--tl-fold:{}px;", view_bottom - top)) } else { (Grip::Bottom, String::new()) }),
			Some((Grip::Corner, String::new())),
		]
		.into_iter()
		.flatten()
		.collect()
	}

	/// While a drag is armed, does this preview cell render as the source's grey landing shadow, and
	/// is its header the `Tab` drop site?
	fn drop_flags(&self, gid: GroupId, cell: &crate::model::packed::Cell) -> (bool, bool) {
		match &self.drag {
			Some(d) if d.armed => match &d.target {
				Some(DropTarget::Tab { group, .. }) => (false, *group == gid),
				Some(DropTarget::Displace { .. }) | Some(DropTarget::Pack { .. }) => {
					let src = match &d.source {
						DragSource::Tile(g) => *g == gid,
						DragSource::Tab { panel, .. } => cell.group.tabs.iter().any(|p| p == panel),
					};
					(src, false)
				}
				None => (false, false),
			},
			_ => (false, false),
		}
	}

	/// The flat, id-keyed content overlay: for every panel the layout hosts, its inline style
	/// (positioned over its tile's body from the same grid rect, `display:none` unless active /
	/// hidden while carried by the ghost) and its host group (a body click focuses it). Panels the
	/// binding knows but the layout doesn't simply aren't in the map — the binding hides them.
	pub fn content_slots(&self) -> HashMap<PanelId, ContentSlot> {
		let g = self.preview();
		let (sw, sh) = self.step_px;
		let hidden = self.ghost_hidden(&g);
		let maximized = self.live(self.maximized);
		let mut map = HashMap::new();
		for cell in &g.cells {
			let active = cell.group.active_panel().clone();
			for id in &cell.group.tabs {
				let style = if hidden.contains(id) {
					"display:none;".to_string()
				} else {
					slot_style(maximized, cell.group.id, cell.x, cell.y, cell.w, cell.h, id == &active, sw, sh, self.title_h())
				};
				map.insert(id.clone(), ContentSlot { style, group: cell.group.id });
			}
		}
		map
	}

	/// Panels carried by the floating ghost: hidden in the overlay so their live content rides the
	/// cursor's ghost, not the snapped grey shadow at the landing cell.
	fn ghost_hidden(&self, preview: &PackedGrid) -> HashSet<PanelId> {
		match &self.drag {
			Some(d) if d.armed => match &d.source {
				DragSource::Tile(g) => preview
					.cells
					.iter()
					.find(|c| c.group.id == *g)
					.map(|c| c.group.tabs.iter().cloned().collect())
					.unwrap_or_default(),
				DragSource::Tab { panel, .. } => std::iter::once(panel.clone()).collect(),
			},
			_ => HashSet::new(),
		}
	}

	/// The floating ghost of whatever is being dragged: it tracks the pointer 1:1 (`cursor − grab`),
	/// keeping the grabbed point under the cursor, while its shadow snaps to the landing cell.
	pub fn ghost(&self, titles: &HashMap<PanelId, String>) -> Option<Ghost> {
		let d = self.drag.as_ref().filter(|d| d.armed)?;
		let tabs = match &d.source {
			DragSource::Tile(g) => Tab::strip(&self.grid.cells.iter().find(|c| c.group.id == *g).expect("drag source group exists").group, titles),
			DragSource::Tab { panel, .. } => vec![Tab {
				id: panel.clone(),
				title: titles.get(panel).cloned().unwrap_or_default(),
				active: true,
			}],
		};
		let (sw, sh) = self.step_px;
		Some(Ghost {
			tabs,
			left: d.cursor.0 - d.grab.0,
			top: d.cursor.1 - d.grab.1,
			w: d.src_w as f64 * sw,
			h: d.src_h as f64 * sh,
		})
	}

	/// One translucent `WxH` box per open inspect popup, placed from the same grid rect the tile
	/// uses. Dead/closed groups drop out of the filter (the set may keep stale ids).
	pub fn inspect(&self) -> Vec<(String, String)> {
		let g = self.preview();
		let (sw, sh) = self.step_px;
		g.cells
			.iter()
			.filter(|c| self.popups.contains(&c.group.id))
			.map(|c| {
				(
					format!(
						"left:{}px; top:{}px; width:{}px; height:{}px;",
						c.x as f64 * sw,
						c.y as f64 * sh,
						c.w as f64 * sw,
						c.h as f64 * sh
					),
					format!("{}x{}", c.w, c.h),
				)
			})
			.collect()
	}

	/// The `?` modal, while open. `titles` names the pane it targets.
	pub fn help(&self, titles: &HashMap<PanelId, String>) -> Option<Help> {
		self.help.then(|| Help {
			pane: self.live(self.focused).and_then(|g| {
				let c = self.grid.cells.iter().find(|c| c.group.id == g).expect("live() checked the group exists");
				titles.get(c.group.active_panel()).cloned()
			}),
			rows: self.keys.rows(),
		})
	}

	/// The which-key popup, while a chord is pending.
	pub fn hint(&self) -> Option<Hint> {
		self.keys.hint(&self.pending)
	}

	/// Say `text` in the dock's toast; the latest replaces any before it.
	pub fn notify(&mut self, text: String) {
		self.notices += 1;
		self.notice = Some(Notice { seq: self.notices, text });
	}

	/// The last [`notify`](Self::notify). It stays set: its fade-out is presentation, in CSS.
	pub fn notice(&self) -> Option<Notice> {
		self.notice.clone()
	}
}

/// A toast. `seq` keys its element, so a repeat remounts it and replays the fade.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
	pub seq: u32,
	pub text: String,
}

/// One tile in the skeleton view-model: its group id, position style, the drop-feedback flags
/// (`shadow` ⇒ render the grey landing placeholder, `tab_drop` ⇒ highlight the header), and its
/// tabs (empty when `shadow`).
pub struct Frame {
	pub gid: GroupId,
	/// Index into the (preview) grid's cells — a resize gesture never overlaps a drag, so the
	/// preview equals the real grid then and this addresses the right cell for [`resize_start`](PackedState::resize_start).
	pub idx: usize,
	pub style: String,
	pub shadow: bool,
	pub tab_drop: bool,
	pub tabs: Vec<Tab>,
	/// The grips to render, in paint order (none on a shadow), each with its inline style.
	pub grips: Vec<(Grip, String)>,
}
/// One tab in a [`Frame`]'s header: its panel id (render key + `data-panel`), label, and whether
/// it's the active tab.
pub struct Tab {
	pub id: PanelId,
	pub title: String,
	pub active: bool,
}
impl Tab {
	fn strip(group: &Group, titles: &HashMap<PanelId, String>) -> Vec<Self> {
		group
			.tabs
			.iter()
			.enumerate()
			.map(|(i, id)| Self {
				id: id.clone(),
				title: titles.get(id).cloned().unwrap_or_default(),
				active: i == group.active,
			})
			.collect()
	}
}
/// A panel's slot in the content overlay: its inline style and host group.
pub struct ContentSlot {
	pub style: String,
	pub group: GroupId,
}
/// The floating drag ghost: the header it carries, top-left in client px, and its size in px.
pub struct Ghost {
	pub tabs: Vec<Tab>,
	pub left: f64,
	pub top: f64,
	pub w: f64,
	pub h: f64,
}
/// Where a tile is taken to resize it. `Left`/`Right` move that edge of its tile alone, pushing the
/// tiles it grows into ([`PackedGrid::resized`]); `Bottom` sets the height; the corner is `Right` and
/// `Bottom` at once. In a framed view, where the right or bottom grip would reach into the band a
/// host's grip may cover, `Wall` and `Fold` stand in for them just before it, and only shrink the tile. The edges are
/// thin strips over the content's own; nothing is taken from it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Grip {
	Corner,
	Left,
	Right,
	Bottom,
	Fold,
	Wall,
}
impl Grip {
	pub fn class(self) -> &'static str {
		match self {
			Self::Corner => "tl-resize-handle",
			Self::Left => "tl-resize-left",
			Self::Right => "tl-resize-right",
			Self::Bottom => "tl-resize-bottom",
			Self::Fold => "tl-resize-fold",
			Self::Wall => "tl-resize-wall",
		}
	}

	pub fn cursor(self) -> &'static str {
		match self {
			Self::Corner => "nwse-resize",
			Self::Left | Self::Right => "ew-resize",
			Self::Bottom => "ns-resize",
			Self::Fold => "n-resize",
			Self::Wall => "w-resize",
		}
	}
}

/// Whether a dock's view edges may lie under a host's grips: its document is framed (a cross-origin
/// frame hides its element, which is still a frame), or its root sits in another dock's tile. Read
/// by the bindings for [`PackedState::set_measure`].
#[cfg(target_arch = "wasm32")]
pub fn framed(root: &web_sys::Element) -> bool {
	let win = web_sys::window().expect("a mounted root has a window");
	!matches!(win.frame_element(), Ok(None))
		|| root.parent_element().is_some_and(|p| p.closest(".tl-tile").expect("a constant, valid selector").is_some())
}

/// A cached layout that couldn't be used. Loud on purpose: a seed cache that silently wipes itself
/// is indistinguishable from one that was never written.
fn report(msg: &str) {
	#[cfg(target_arch = "wasm32")]
	web_sys::console::error_1(&msg.into());
	#[cfg(not(target_arch = "wasm32"))]
	eprintln!("{msg}");
}

/// Inline style placing a panel's content over its tile's body from the grid rect — the same math
/// the skeleton uses, so the two cannot drift apart. Inactive tabs stay mounted but `display:none`;
/// a maximized group's active panel fills the container below the chrome, all others hidden.
fn slot_style(maximized: Option<GroupId>, group: GroupId, x: u32, y: u32, w: u32, h: u32, active: bool, step_w: f64, step_h: f64, chrome_h: f64) -> String {
	if let Some(mg) = maximized {
		if group != mg || !active {
			return "display:none;".into();
		}
		return format!("display:block; left:0; top:{chrome_h}px; width:100%; height:calc(100% - {chrome_h}px);");
	}
	if !active {
		return "display:none;".into();
	}
	// Tiles scale with the step px; the chrome band is a font-relative constant, so the content's
	// top/height bridge the two with `calc`.
	let (left, top) = (x as f64 * step_w, y as f64 * step_h);
	let (width, height) = (w as f64 * step_w, h as f64 * step_h);
	format!("display:block; left:{left}px; top:calc({top}px + {chrome_h}px); width:{width}px; height:calc({height}px - {chrome_h}px);")
}

/// The document's root font size in px. Only ever reached from a live measure of a mounted element,
/// so every step here is guaranteed — a failure means the DOM isn't what it claims, which would
/// silently mis-scale every tile's chrome, so it panics instead.
#[cfg(target_arch = "wasm32")]
fn root_font_px() -> f64 {
	let win = web_sys::window().expect("a browser window");
	let root = win.document().expect("a document").document_element().expect("a document element");
	let style = win
		.get_computed_style(&root)
		.expect("our own document element is never cross-origin")
		.expect("a rendered root has a style declaration");
	// A *computed* font-size is always an absolute px length, whatever the author wrote.
	style
		.get_property_value("font-size")
		.expect("property lookup never throws")
		.trim_end_matches("px")
		.parse()
		.expect("computed font-size is an absolute px length")
}
#[cfg(not(target_arch = "wasm32"))]
fn root_font_px() -> f64 {
	REM_PX
}

/// An in-flight reposition: what was picked up, where the press began (client px, to measure the
/// `DRAG_THRESHOLD`), the live pointer (to drag the ghost naturally), and — once `armed` — the
/// live [`DropTarget`]. `src_w`/`src_h` size both the ghost and a `Pack` target's column clamp;
/// `grab` is the pointer's offset within the picked-up element, so the ghost rides under the same
/// point you grabbed.
#[derive(Clone)]
struct Drag {
	source: DragSource,
	src_w: u32,
	src_h: u32,
	start: (f64, f64),
	grab: (f64, f64),
	cursor: (f64, f64),
	armed: bool,
	target: Option<DropTarget>,
}

/// A resize in flight: the grabbed tile and grip, the layout and pointer at the press, and the
/// live pointer. The live grid is always `press` resized by `cursor − press_at`.
struct ResizeStart {
	gid: GroupId,
	grip: Grip,
	press: PackedGrid,
	press_at: (f64, f64),
	cursor: (f64, f64),
}

/// Linear undo history of settled layouts with a cursor into it. `push` appends a fresh edit,
/// dropping any redo branch ahead of the cursor; `step` walks the cursor by ±1 and returns the
/// snapshot to restore (or `None` at an end).
// ponytail: linear, not a branching tree — two keys (undo/redo) can only express a line. Promote to
// a real tree once there's UI to pick a branch.
#[derive(Default)]
struct UndoHistory {
	states: Vec<PackedGrid>,
	cursor: usize,
}

impl UndoHistory {
	fn push(&mut self, g: PackedGrid) {
		if !self.states.is_empty() {
			self.states.truncate(self.cursor + 1);
		}
		self.states.push(g);
		self.cursor = self.states.len() - 1;
	}

	fn step(&mut self, dir: i32) -> Option<PackedGrid> {
		let next = self.cursor.checked_add_signed(dir as isize)?;
		let g = self.states.get(next)?.clone();
		self.cursor = next;
		Some(g)
	}
}
