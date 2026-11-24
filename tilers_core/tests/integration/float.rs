//! A float is a group above the grid, never a cell: lifting a tile out lets the grid settle, docking
//! lands it where it shows, its rect lives in the view and is held inside it at render, and the user
//! moves it by its header and docks it by its tab.

use std::collections::HashMap;

use tilers_core::{
	Config, Group, GroupId, MinSize, PackedState, PanelId,
	keys::KeyInput,
	model::{packed::Floating, serial},
};

const WIDTH: f64 = 1600.0;
const ROW: f64 = 32.0;
const COL: f64 = WIDTH / 64.0;

fn id(s: &str) -> PanelId {
	PanelId(s.into())
}

/// Two half-width tiles side by side, `a` then `b`; `c` is declared but not hosted.
fn seeded() -> (PackedState, GroupId, GroupId) {
	let mut s = PackedState::new(Config::default());
	s.set_panels(["a", "b", "c"].into_iter().map(|p| (id(p), MinSize::Px { w: 0.0, h: 0.0 })).collect());
	s.set_measure((0.0, 0.0), (WIDTH, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	let gids = ["a", "b"].map(|p| {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, id(p)), 32, 4);
		gid
	});
	(s, gids[0], gids[1])
}

fn float(s: &PackedState, gid: GroupId) -> &Floating {
	s.grid().floats.iter().find(|f| f.group.id == gid).expect("the group floats")
}

/// The px rect `[left, top, width, height]` a group's frame renders at.
fn shown(s: &PackedState, gid: GroupId) -> [f64; 4] {
	let frames = s.frames(&HashMap::new());
	let style = &frames.iter().find(|f| f.gid == gid).expect("the group renders").style;
	["left:", "top:", "width:", "height:"].map(|k| {
		let at = style.find(k).expect("a px rect") + k.len();
		style[at..at + style[at..].find("px").expect("px")].parse().expect("a number")
	})
}

fn leader_then(key: &str) -> [KeyInput; 2] {
	let input = |alt: bool, key: &str| KeyInput {
		key: key.into(),
		code: format!("Key{}", key.to_ascii_uppercase()),
		ctrl: false,
		alt,
		meta: false,
		shift: false,
		repeat: false,
		composing: false,
		editable: Some(false),
	};
	[input(true, "z"), input(false, key)]
}

#[test]
fn floating_a_tile_out_and_docking_it_where_it_shows_round_trips() {
	let (mut s, a, b) = seeded();
	let before = s.grid().clone();
	s.toggle_float(b);
	assert_eq!(s.grid().cells.iter().map(|c| c.group.id).collect::<Vec<_>>(), [a], "the grid settled without it");
	let f = float(&s, b);
	assert_eq!((f.x, f.y, f.w, f.h), (32, 0, 32, 4), "lifted at where it showed, at its size");
	s.toggle_float(b);
	assert_eq!(s.grid(), &before, "docked where it showed, it is the tile it was");
}

#[test]
fn opening_a_float_leaves_the_grid_be() {
	let (mut s, ..) = seeded();
	let cells = s.grid().cells.clone();
	let c = s.mint_group_id();
	s.float(Group::new(c, id("c")), 20, 10);
	assert_eq!(s.grid().cells, cells);
	let f = float(&s, c);
	assert_eq!((f.x, f.y), (22, 9), "centered in the 64x28-step view");
	let d = s.mint_group_id();
	s.set_panels(["a", "b", "c", "d"].into_iter().map(|p| (id(p), MinSize::Px { w: 0.0, h: 0.0 })).collect());
	s.float(Group::new(d, id("d")), 20, 10);
	assert_eq!((float(&s, d).x, float(&s, d).y), (23, 10), "a step past the one already there");
	assert_eq!(s.frames(&HashMap::new()).last().map(|f| f.gid), Some(d), "the newest is on top");
	s.focus(c);
	assert_eq!(s.frames(&HashMap::new()).last().map(|f| f.gid), Some(c), "a press raises it");
}

#[test]
fn the_view_holds_a_float_in_without_moving_where_it_is_kept() {
	let (mut s, ..) = seeded();
	let c = s.mint_group_id();
	s.float(Group::new(c, id("c")), 20, 10);
	let at = shown(&s, c);
	assert_eq!(at, [22.0 * COL, 9.0 * ROW, 20.0 * COL, 10.0 * ROW]);
	s.set_measure((0.0, 0.0), (WIDTH, 200.0), false);
	assert_eq!(shown(&s, c), [22.0 * COL, 0.0, 20.0 * COL, 6.0 * ROW], "capped to the 6 rows that show, pulled up into them");
	s.set_measure((0.0, 0.0), (WIDTH, 900.0), false);
	assert_eq!(shown(&s, c), at, "the view growing back restores it");
}

#[test]
fn a_header_drag_moves_a_float_within_the_view_and_a_cancel_puts_it_back() {
	let (mut s, ..) = seeded();
	let c = s.mint_group_id();
	s.float(Group::new(c, id("c")), 20, 10);
	let [l, t, ..] = shown(&s, c);
	let press = (l + 100.0, t + 8.0);
	s.press_tile(c, press, (100.0, 8.0));
	s.drag_move((press.0 + 10.0 * COL, press.1 + 2.0 * ROW));
	assert_eq!((float(&s, c).x, float(&s, c).y), (32, 11));
	assert!(s.ghost(&HashMap::new()).is_none(), "a float is its own ghost");
	s.drag_move((press.0 + 10_000.0, press.1 + 10_000.0));
	assert_eq!((float(&s, c).x, float(&s, c).y), (44, 18), "held in the view");
	s.drag_cancel();
	assert_eq!((float(&s, c).x, float(&s, c).y), (22, 9));

	s.press_tile(c, press, (100.0, 8.0));
	s.drag_move((press.0 - 5.0 * COL, press.1));
	s.drag_release();
	assert_eq!((float(&s, c).x, float(&s, c).y), (17, 9));
	assert_eq!(s.grid().cells.len(), 2, "the grid never noticed");
}

#[test]
fn dragging_a_floats_tab_docks_it_where_it_drops() {
	let (mut s, a, _) = seeded();
	let c = s.mint_group_id();
	s.float(Group::new(c, id("c")), 20, 10);
	let [l, t, ..] = shown(&s, c);
	s.press_tab(id("c"), c, (l + 20.0, t + 8.0), (20.0, 8.0));
	s.drag_move((l + 40.0, t + 8.0));
	s.drag_move((100.0, 16.0)); // a's header
	s.drag_release();
	assert!(s.grid().floats.is_empty());
	let tile = s.grid().cells.iter().find(|c| c.group.id == a).expect("a stays");
	assert_eq!(tile.group.tabs, [id("a"), id("c")], "joined a as a tab");
}

#[test]
fn leader_z_floats_the_focused_pane_and_docks_it_back() {
	let (mut s, _, b) = seeded();
	s.focus(b);
	for k in leader_then("z") {
		s.on_key(k);
	}
	assert_eq!(s.grid().floats.len(), 1);
	for k in leader_then("z") {
		s.on_key(k);
	}
	assert!(s.grid().floats.is_empty());
	assert_eq!(s.grid().cells.len(), 2);
}

#[test]
fn floats_survive_a_save_and_a_layout_from_before_them_loads() {
	let (mut s, _, b) = seeded();
	s.toggle_float(b);
	let json = s.save();
	let (mut fresh, ..) = seeded();
	fresh.load(&json).expect("our own save loads");
	assert_eq!(fresh.grid(), s.grid());
	let mut v3: serde_json::Value = serde_json::from_str(&json).expect("a save is JSON");
	v3["version"] = 3.into();
	v3["grid"].as_object_mut().expect("the grid is an object").remove("floats");
	assert!(serial::load(&v3.to_string()).expect("a v3 layout loads").floats.is_empty());
}
