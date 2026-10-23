//! State-aware action enum + generator. Every action is an *input event* — a measure, a pointer
//! path, a keystroke — driven at the same entry points a binding calls, so `resolve_target`, the
//! drag threshold, the preview, the ghost, the breakpoint math and the undo history are all on the
//! fuzzed path. The generator enumerates only valid targets each step (matklad's "non-crashed
//! replicas only") so no entropy is spent on impossible picks.
//!
//! Pointer paths are *aimed*, not uniform: uniform pixels would essentially never land on a 32px
//! header, so a drag would only ever resolve to `Pack`. The generator picks a live aim point (a
//! body centre, a header, open space, off-grid) and interpolates to it, always with one
//! sub-threshold move first so the arming branch runs on every drag.

use std::collections::HashMap;

use tilers_core::{
	Grip, Group, GroupId, PackedState, PanelId, Step,
	keys::{Binding, Builtin, Chord, Command, Help, Hint, KeyInput, Keymap, Node},
	model::packed::{Cell, DragSource, DropTarget, MinSize, PackedGrid},
};
use v_utils::fuzz::Frng;

use crate::{
	oracle,
	sim::{EDGES, MAX_H, MAX_W, MIN_H, MIN_W, World},
};

/// Action kinds, in the order their swarm weights are drawn.
pub const N_KINDS: usize = 10;
/// Height of the tile chrome: `Config::title_h_rem`'s default resolved against the 16px root font a
/// non-DOM target assumes. Mirrored here only to aim at headers and to re-run the pure
/// `resolve_target` for the coverage tally.
const CHROME_H: f64 = 32.0;

const MEASURE: usize = 0;
const PLACE: usize = 1;
const ADD_TAB: usize = 2;
const FOCUS: usize = 3;
const DRAG: usize = 4;
const RESIZE: usize = 5;
const KEY: usize = 6;
const SAVE_LOAD: usize = 7;
const SET_MINS: usize = 8;
const SCROLL: usize = 9;

#[derive(Clone, Debug)]
pub enum Action {
	/// A container resize, through `set_measure` → `Breakpoint` → `scale_cols` → `refit`.
	Measure {
		w: f64,
		h: f64,
	},
	Place {
		w: u32,
		h: u32,
		min: MinSize,
	},
	AddTab {
		group: GroupId,
		min: MinSize,
	},
	Focus {
		group: GroupId,
	},
	/// press → N pointer moves → release (`commit`) or cancel.
	Drag {
		grab: Grab,
		path: Vec<(f64, f64)>,
		commit: bool,
	},
	/// A resize from one of the tile's grips; `commit: false` leaves the gesture in flight, which is a
	/// state a user can genuinely rest in.
	Resize {
		idx: usize,
		grip: Grip,
		start: (f64, f64),
		path: Vec<(f64, f64)>,
		commit: bool,
	},
	/// A keystroke sequence reaching `aim`, unless [`noisy`] inputs mixed into it stop it short.
	Key {
		inputs: Vec<KeyInput>,
		aim: Aim,
	},
	/// `save()` → a fresh state measured the same → `load()` → must reproduce the layout.
	SaveLoad,
	/// The root scrolled: where a tile shows, and so where a tile running past the view's bottom has
	/// its fold grip.
	Scroll {
		y: f64,
	},
	/// A host re-declaring a panel's min in its panel list. The floor is unbounded by the tile's
	/// current span, which is the case `resize` never produces.
	SetMins {
		panel: PanelId,
		min: MinSize,
	},
}

/// What a press picks up, with the press point and the pointer's offset within the pressed element.
#[derive(Clone, Debug)]
pub enum Grab {
	Tile { gid: GroupId, start: (f64, f64), grab: (f64, f64) },
	Tab { panel: PanelId, from: GroupId, start: (f64, f64), grab: (f64, f64) },
}

#[derive(Clone, Copy, Debug)]
pub enum Aim {
	Builtin(Builtin),
	Escape,
}

/// An unbound key or an editable target: either cancels whatever the sequence had opened.
pub fn noisy(inputs: &[KeyInput]) -> bool {
	inputs.iter().any(|k| k.editable || matches!(k.key.as_str(), "j" | "ж"))
}
/// Apply `action`, returning the invariant it violated. Several invariants are only expressible
/// *around* an action (a cancel is a no-op, an unarmed release only activates), so they live here
/// rather than in the oracle. `mid` fires after every individual pointer move that passed the
/// in-flight oracle, so an observer sees a gesture as the several events it is.
pub fn apply(action: &Action, world: &mut World, mid: &mut dyn FnMut(&World)) -> Result<(), String> {
	match action {
		Action::Measure { w, h } => return world.measure(*w, *h),
		Action::Place { w, h, min } => {
			world.structural_edit();
			let gid = world.state.mint_group_id();
			let panel = world.mint_panel(*min);
			world.state.place(Group::new(gid, panel), *w, *h);
		}
		Action::AddTab { group, min } => {
			world.structural_edit();
			let panel = world.mint_panel(*min);
			world.state.add_tab(*group, panel);
		}
		Action::Focus { group } => world.state.focus(*group),
		Action::Drag { grab, path, commit } => return drag(grab, path, *commit, world, mid),
		Action::Resize { idx, grip, start, path, commit } => {
			world.structural_edit();
			world.state.resize_start(*idx, *grip, start.0, start.1);
			let press = world.state.grid().cells.clone();
			// Only the grabbed edge moves; another tile moves only by being pushed along in front of it.
			let edge = |c: &Cell| match grip {
				Grip::Left => Some(c.x),
				Grip::Right | Grip::Wall | Grip::Corner => Some(c.x + c.w),
				Grip::Bottom | Grip::Fold => None,
			};
			let near = |c: &Cell| if *grip == Grip::Left { c.x + c.w } else { c.x };
			let ahead = |a: u32, b: u32| if *grip == Grip::Left { a <= b } else { a >= b };
			let g = &press[*idx];
			for p in path {
				world.cursor = *p;
				world.state.resize_move(*p);
				oracle::check_in_flight(world)?;
				let now = &world.state.grid().cells;
				for (j, (a, b)) in press.iter().zip(now).enumerate() {
					if matches!(grip, Grip::Left | Grip::Right | Grip::Wall) && a.h != b.h {
						return Err(format!("the {grip:?} grip changed tile {j}'s height: {a:?} → {b:?}"));
					}
					if matches!(grip, Grip::Bottom | Grip::Fold) && (a.x, a.w) != (b.x, b.w) {
						return Err(format!("the {grip:?} grip changed tile {j}'s columns: {a:?} → {b:?}"));
					}
					if matches!(grip, Grip::Fold | Grip::Wall) && j == *idx && (b.w > a.w || b.h > a.h) {
						return Err(format!("the {grip:?} grip grew its tile: {a:?} → {b:?}"));
					}
				}
				if let (Some(e0), Some(e)) = (edge(g), edge(&now[*idx])) {
					for (j, (a, b)) in press.iter().zip(now).enumerate() {
						let pushed = a.y < g.y + g.h && g.y < a.y + a.h && ahead(near(a), e0) && !ahead(near(a), e) && near(b) == e;
						if j != *idx && (a.x, a.w) != (b.x, b.w) && !pushed {
							return Err(format!("the {grip:?} grip moved tile {j}, which its edge never reached: {a:?} → {b:?} (grabbed {:?})", now[*idx]));
						}
					}
				}
				mid(world);
			}
			// A gesture is a function of where the pointer is, not of how it got there.
			if let Some(&last) = path.last() {
				let there = world.state.grid().cells.clone();
				world.state.resize_move(*start);
				if world.state.grid().cells != press {
					return Err(format!("back at the press point, the {grip:?} grip left {:?}, not {press:?}", world.state.grid().cells));
				}
				world.state.resize_move(last);
				if world.state.grid().cells != there {
					return Err(format!("returning to {last:?} via the press point gave {:?}, not {there:?}", world.state.grid().cells));
				}
			}
			if *commit {
				world.state.resize_end();
			}
		}
		Action::Key { inputs, aim } => return key(inputs, *aim, world),
		Action::SaveLoad => return save_load(world),
		Action::SetMins { panel, min } => {
			world.structural_edit();
			world.mins.insert(panel.clone(), *min);
			world.state.set_mins(world.mins.clone());
		}
		Action::Scroll { y } => {
			world.scroll = *y;
			world.state.set_scroll(*y);
		}
	}
	Ok(())
}

/// Pick a valid action for the current `world`, weighting kinds by the per-run `weights` (swarm).
pub fn generate(frng: &mut Frng, world: &World, weights: &[u32; N_KINDS]) -> Option<Action> {
	let cells = &world.state.grid().cells;

	let mut avail = vec![MEASURE, PLACE, KEY, SCROLL];
	if !cells.is_empty() {
		avail.push(ADD_TAB);
		avail.push(FOCUS);
		avail.push(DRAG);
		// `resize_start` indexes the cell list, so an empty grid has no grip to take.
		avail.push(RESIZE);
		avail.push(SET_MINS);
		// A mid-resize grid is deliberately unsettled and `load` refits, so the two can't agree yet.
		if world.state.resizing().is_none() {
			avail.push(SAVE_LOAD);
		}
	}

	let ws: Vec<u32> = avail.iter().map(|&k| weights[k].max(1)).collect();
	let kind = avail[frng.weighted(&ws)];
	let (sw, sh) = world.step_px();
	let (ox, oy) = (world.origin.0, world.origin.1 - world.scroll);

	Some(match kind {
		MEASURE => {
			// Half the draws land on a band boundary, half anywhere — so `Breakpoint::of`'s edges are hit
			// exactly instead of being left to a 1-in-1600 chance.
			let w = if frng.below(2) == 0 {
				EDGES[frng.below(EDGES.len() as u32) as usize]
			} else {
				frng.span(MIN_W, MAX_W)
			};
			Action::Measure { w, h: frng.span(MIN_H, MAX_H) }
		}
		PLACE => {
			// Sized as a fraction of what shows, so a tile stays a pane rather than a sliver.
			let w = 1 + frng.below(world.state.cols() / 2);
			let h = 1 + frng.below(world.state.rows() / 2);
			Action::Place { w, h, min: min(frng, w, h) }
		}
		ADD_TAB => Action::AddTab {
			group: pick_group(frng, world),
			min: min(frng, world.state.cols(), world.state.rows()),
		},
		FOCUS => Action::Focus { group: pick_group(frng, world) },
		DRAG => {
			let c = &cells[frng.below(cells.len() as u32) as usize];
			let (left, top) = (ox + c.x as f64 * sw, oy + c.y as f64 * sh);
			let width = c.w as f64 * sw;
			// Half the time tear a tab (aim at its slot in the strip), else grab the header's empty area.
			let grab = if frng.below(2) == 0 {
				let i = frng.below(c.group.tabs.len() as u32);
				let x = (i as f64 * 40.0 + 12.0).min((width - 2.0).max(0.0));
				Grab::Tab {
					panel: c.group.tabs[i as usize].clone(),
					from: c.group.id,
					start: (left + x, top + 8.0),
					grab: (x, 8.0),
				}
			} else {
				let x = width * 0.8;
				Grab::Tile {
					gid: c.group.id,
					start: (left + x, top + 8.0),
					grab: (x, 8.0),
				}
			};
			let start = match &grab {
				Grab::Tile { start, .. } | Grab::Tab { start, .. } => *start,
			};
			let to = aim(frng, world);
			// One drag in four never leaves the threshold, so the click path (activate, don't move) runs.
			let arm = frng.below(4) != 0;
			Action::Drag {
				path: path(frng, start, to, arm),
				grab,
				commit: frng.below(4) != 0,
			}
		}
		RESIZE => {
			// Only the grips a binding renders: a frame offers a left grip only on a free left edge.
			let frames = world.state.frames(&HashMap::new());
			let f = &frames[frng.below(frames.len() as u32) as usize];
			let (idx, grip) = (f.idx, f.grips[frng.below(f.grips.len() as u32) as usize].0);
			let c = &cells[idx];
			// Where the grip sits on the tile: its corner, or the middle of the edge it is.
			let (r, b) = (ox + (c.x + c.w) as f64 * sw, oy + (c.y + c.h) as f64 * sh);
			let mid_y = oy + (c.y as f64 + c.h as f64 / 2.0) * sh;
			let start = match grip {
				Grip::Corner => (r, b),
				Grip::Left => (ox + c.x as f64 * sw, mid_y),
				Grip::Right => (r, mid_y),
				Grip::Wall => (ox + world.size.0 - 1.0 - world.state.title_h() * 3.0 / 16.0, mid_y),
				Grip::Bottom => (ox + (c.x as f64 + c.w as f64 / 2.0) * sw, b),
				// Its middle, as the stylesheet places it: one grip's thickness (an eighth of the title bar)
				// clear of the view's bottom, inside the tile's 1px border.
				Grip::Fold => (ox + (c.x as f64 + c.w as f64 / 2.0) * sw, world.origin.1 + world.size.1 - 1.0 - world.state.title_h() * 3.0 / 16.0),
			};
			let to = aim(frng, world);
			Action::Resize {
				idx,
				grip,
				start,
				path: path(frng, start, to, true),
				// A gesture left in flight is legal; when one already is, always close it out so a run can't
				// spend its whole tail unsettled.
				commit: world.state.resizing().is_some() || frng.below(4) != 0,
			}
		}
		KEY => {
			let seqs = sequences();
			let (mut inputs, aim) = match seqs.get(frng.below(seqs.len() as u32 + 1) as usize) {
				Some((chords, b)) => (chords.iter().map(|c| input(*c)).collect::<Vec<_>>(), Aim::Builtin(*b)),
				None => (vec![input(Chord { key: "Escape", .. })], Aim::Escape),
			};
			// Each draws once per sequence, so most sequences stay clean and actually reach their aim.
			if frng.below(4) == 0 {
				let i = frng.below(inputs.len() as u32 + 1) as usize;
				inputs.insert(
					i,
					KeyInput {
						shift: true,
						..input(Chord { key: "Shift", .. })
					},
				);
			}
			if frng.below(6) == 0 {
				let i = frng.below(inputs.len() as u32) as usize;
				inputs[i] = input(Chord { key: "j", .. });
			}
			if frng.below(6) == 0 {
				let i = frng.below(inputs.len() as u32) as usize;
				inputs[i].editable = true;
			}
			// A Cyrillic layout: a letter key sends a non-Latin `key`, which nothing binds.
			if frng.below(6) == 0 {
				let i = frng.below(inputs.len() as u32) as usize;
				if inputs[i].key.len() == 1 && inputs[i].key.chars().all(|c| c.is_ascii_alphabetic()) {
					inputs[i].key = "ж".into();
				}
			}
			Action::Key { inputs, aim }
		}
		SAVE_LOAD => Action::SaveLoad,
		SCROLL => {
			// Anywhere the content reaches, as a browser clamps it.
			let bottom = cells.iter().map(|c| c.y + c.h).max().unwrap_or(0) as f64 * sh;
			Action::Scroll {
				y: frng.below((bottom - world.size.1).max(0.0) as u32 + 1) as f64,
			}
		}
		SET_MINS => {
			let c = &cells[frng.below(cells.len() as u32) as usize];
			Action::SetMins {
				panel: c.group.tabs[frng.below(c.group.tabs.len() as u32) as usize].clone(),
				// Drawn against the whole band, not the tile — a floor wider than what's left of the row
				// is the case only a re-floor can reach, and the one that must clamp rather than spill.
				min: min(frng, world.state.cols(), world.state.rows()),
			}
		}
		_ => unreachable!("kind is one of the constants pushed into `avail`"),
	})
}

/// Every leaf of the default keymap as the chords reaching it, so a new bind is fuzzed without
/// anyone remembering to list it. Platform `other`: natively that is what the state resolves.
fn sequences() -> Vec<(Vec<Chord>, Builtin)> {
	fn walk(bs: &[Binding], prefix: &[Chord], out: &mut Vec<(Vec<Chord>, Builtin)>) {
		for b in bs {
			let seq = [prefix, &[b.on.other]].concat();
			match &b.node {
				Node::Run(Command::Builtin(x)) => out.push((seq, *x)),
				Node::Scope { children, .. } => walk(children, &seq, out),
				_ => unreachable!("the default keymap holds only builtins and scopes"),
			}
		}
	}
	let k = Keymap::default();
	let mut out = Vec::new();
	walk(&k.root, &[], &mut out);
	walk(&k.scope, &[k.leader.other], &mut out);
	out
}

/// The keydown a browser reports for `c` on a US layout.
fn input(c: Chord) -> KeyInput {
	let letter = c.key.len() == 1 && c.key.chars().all(|ch| ch.is_ascii_alphabetic());
	KeyInput {
		key: c.key.to_string(),
		ctrl: c.ctrl,
		alt: c.alt,
		meta: c.meta,
		shift: letter && c.key.chars().all(|ch| ch.is_ascii_uppercase()),
		editable: false,
	}
}

/// Every `MinSize` arm, `Steps` bounded by `w×h`: the length arms are the ones that re-resolve
/// against the live step, and `Px` is a content size that must hold through every measure.
fn min(frng: &mut Frng, w: u32, h: u32) -> MinSize {
	match frng.below(3) {
		0 => MinSize::Steps {
			w: Step(1 + frng.below(w)),
			h: Step(1 + frng.below(h)),
		},
		1 => MinSize::Rem {
			w: (1 + frng.below(20)) as f64,
			h: (1 + frng.below(20)) as f64,
		},
		_ => MinSize::Px {
			w: frng.below(600) as f64,
			h: frng.below(400) as f64,
		},
	}
}

/// One whole pointer gesture, plus the three invariants that only exist around it: a press never
/// touches the model, a cancel is a no-op, and an unarmed release is pure tab activation.
fn drag(grab: &Grab, path: &[(f64, f64)], commit: bool, world: &mut World, mid: &mut dyn FnMut(&World)) -> Result<(), String> {
	let titles = HashMap::new();
	let before = world.state.grid().clone();
	let source = match grab {
		Grab::Tile { gid, start, grab } => {
			world.state.press_tile(*gid, *start, *grab);
			DragSource::Tile(*gid)
		}
		Grab::Tab { panel, from, start, grab } => {
			world.state.press_tab(panel.clone(), *from, *start, *grab);
			DragSource::Tab { panel: panel.clone(), from: *from }
		}
	};
	if world.state.grid() != &before {
		return Err("a press mutated the layout".to_string());
	}

	world.resolved.clear();
	let mut armed = false;
	for p in path {
		world.cursor = *p;
		world.state.drag_move(*p);
		oracle::check_in_flight(world)?;
		// The ghost is the public witness of arming, so the fuzzer never has to know the threshold.
		if world.state.ghost(&titles).is_some() {
			armed = true;
			let t = resolve(world, &source, grab_offset(grab), *p);
			world.resolved.push(t);
		}
		mid(world);
	}

	if !commit {
		world.state.drag_cancel();
		return if world.state.grid() == &before {
			Ok(())
		} else {
			Err(format!("drag_cancel changed the layout: {:?} vs {:?}", world.state.grid().cells, before.cells))
		};
	}

	world.structural_edit();
	world.state.drag_release();
	if !armed {
		// A press that never armed is a click: at most it activates the tab it landed on.
		let mut expect = before.clone();
		for (e, a) in expect.cells.iter_mut().zip(&world.state.grid().cells) {
			e.group.active = a.group.active;
		}
		if world.state.grid() != &expect {
			return Err(format!("unarmed release changed more than the active tab: {:?} vs {:?}", world.state.grid().cells, before.cells));
		}
	}
	Ok(())
}

/// Re-run the pure `resolve_target` the way `drag_move` does, purely to tally what the pointer
/// actually reached. It mirrors the reducer's block-centre math; drift makes the *tally* wrong, not
/// the run — no invariant reads it.
fn resolve(world: &World, source: &DragSource, grab: (f64, f64), cursor: (f64, f64)) -> DropTarget {
	let grid = world.state.grid();
	let (sw, sh) = world.step_px();
	let (ox, oy) = (world.origin.0, world.origin.1 - world.scroll);
	let c = grid
		.cells
		.iter()
		.find(|c| c.group.id == source_group(source))
		.expect("the drag source is live for the whole gesture");
	let (cx, cy) = (cursor.0 - grab.0 + c.w as f64 * sw / 2.0, cursor.1 - grab.1 + c.h as f64 * sh / 2.0);
	grid.resolve_target(source, cx - ox, cy - oy, cursor.0 - ox, cursor.1 - oy, sw, sh, CHROME_H, world.state.cols(), c.w, c.h)
}

fn source_group(source: &DragSource) -> GroupId {
	match source {
		DragSource::Tile(g) => *g,
		DragSource::Tab { from, .. } => *from,
	}
}

fn grab_offset(grab: &Grab) -> (f64, f64) {
	match grab {
		Grab::Tile { grab, .. } | Grab::Tab { grab, .. } => *grab,
	}
}

/// Everything a binding renders that the keyboard can reach.
#[derive(PartialEq)]
struct Seen {
	grid: PackedGrid,
	hint: Option<Hint>,
	help: Option<Help>,
	inspect: Vec<(String, String)>,
	frames: Vec<String>,
}

fn seen(s: &PackedState) -> Seen {
	let titles = HashMap::new();
	Seen {
		grid: s.grid().clone(),
		hint: s.hint(),
		help: s.help(&titles),
		inspect: s.inspect(),
		frames: s.frames(&titles).into_iter().map(|f| f.style).collect(),
	}
}

/// One keystroke sequence, plus the bounds on what its aim is allowed to touch. Noise only ever
/// stops a sequence short, so each bound holds whether or not the aim was reached.
fn key(inputs: &[KeyInput], aim: Aim, world: &mut World) -> Result<(), String> {
	let before = world.state.grid().clone();
	let help_was = world.state.help(&HashMap::new()).is_some();
	for k in inputs {
		let was = seen(&world.state);
		let out = world.state.on_key(k.clone());
		let now = seen(&world.state);
		// Only the pending chord (the hint) may move: text undo and typing stay the browser's.
		if k.editable && (out.prevent_default || now.grid != was.grid || now.help != was.help || now.inspect != was.inspect || now.frames != was.frames) {
			return Err(format!("{k:?} aimed at an editable acted: {out:?}"));
		}
		// A binding re-renders only on `changed`, so anything else that moved is a stale screen.
		if !out.changed && now != was {
			return Err(format!("{k:?} changed what renders but reported `changed: false`"));
		}
	}
	// Help always acts, so it is the one aim whose arrival is observable: a clean sequence must get there.
	if matches!(aim, Aim::Builtin(Builtin::Help)) && !noisy(inputs) && world.state.help(&HashMap::new()).is_some() == help_was {
		return Err(format!("{inputs:?} never reached Help"));
	}
	match aim {
		Aim::Escape | Aim::Builtin(Builtin::Maximize | Builtin::Help | Builtin::Inspect | Builtin::Cache | Builtin::Publish) =>
			if world.state.grid() != &before {
				return Err(format!("{aim:?} is view-only but changed the layout: {:?} vs {:?}", world.state.grid().cells, before.cells));
			},
		Aim::Builtin(Builtin::Undo | Builtin::Redo) => {
			world.structural_edit();
			// A restored snapshot is settled into the live geometry first, so the match is against each
			// capture as it would land here — not against the raw capture.
			let mut landed = false;
			for g in &world.undo_log {
				landed |= &world.settled(g)? == world.state.grid();
			}
			if world.state.grid() != &before && !landed {
				return Err(format!("{aim:?} produced a layout that was never captured: {:?}", world.state.grid().cells));
			}
		}
		Aim::Builtin(_) => world.structural_edit(),
	}
	// A key mid-resize must leave the gesture addressing live cells.
	if world.state.resizing().is_some() {
		world.cursor.0 += 7.0;
		world.state.resize_move(world.cursor);
		oracle::check_in_flight(world)?;
	}
	Ok(())
}

fn save_load(world: &mut World) -> Result<(), String> {
	let fresh = world.settled(world.state.grid())?;
	if &fresh != world.state.grid() {
		return Err(format!("save/load changed the layout: {:?} vs {:?}", fresh.cells, world.state.grid().cells));
	}
	Ok(())
}

/// A live client-px point worth aiming a gesture at. Uniform pixels would almost never hit a header
/// or a tile edge, so each interesting site is drawn with its own probability instead.
fn aim(frng: &mut Frng, world: &World) -> (f64, f64) {
	let cells = &world.state.grid().cells;
	let (sw, sh) = world.step_px();
	let (ox, oy) = (world.origin.0, world.origin.1 - world.scroll);
	match frng.below(5) {
		// A tile's body: resolves to a displace at the row below it.
		0 if !cells.is_empty() => {
			let c = &cells[frng.below(cells.len() as u32) as usize];
			(ox + (c.x as f64 + c.w as f64 / 2.0) * sw, oy + (c.y as f64 + c.h as f64 / 2.0) * sh)
		}
		// A tile's header: resolves to a join.
		1 if !cells.is_empty() => {
			let c = &cells[frng.below(cells.len() as u32) as usize];
			(ox + (c.x as f64 + 0.5) * sw, oy + c.y as f64 * sh + CHROME_H / 2.0)
		}
		// Open space in-band: displace or pack, depending on the skyline under it.
		2 => {
			let col = frng.below(world.state.cols());
			(ox + (col as f64 + 0.5) * sw, oy + frng.below(24) as f64 * sh)
		}
		3 => (ox - (1 + frng.below(200)) as f64, oy + frng.below(400) as f64),
		_ => (ox + world.size.0 + frng.below(200) as f64, oy + world.size.1 + frng.below(200) as f64),
	}
}

/// A pointer path from `start` towards `to`. The first move is always sub-threshold, so the arming
/// branch runs on every drag; `arm: false` keeps the whole path there, so the release is a click.
fn path(frng: &mut Frng, start: (f64, f64), to: (f64, f64), arm: bool) -> Vec<(f64, f64)> {
	let mut out = vec![(start.0 + 0.5, start.1 + 0.5)];
	if !arm {
		out.push((start.0 + 1.0, start.1));
		return out;
	}
	let n = 2 + frng.below(3);
	for i in 1..=n {
		let t = i as f64 / n as f64;
		out.push((start.0 + (to.0 - start.0) * t, start.1 + (to.1 - start.1) * t));
	}
	out
}

fn pick_group(frng: &mut Frng, world: &World) -> GroupId {
	let cells = &world.state.grid().cells;
	cells[frng.below(cells.len() as u32) as usize].group.id
}
