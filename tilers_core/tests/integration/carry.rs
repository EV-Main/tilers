//! A native drag carrying a host's panel ([`PackedState::carry`]) lands where it is dropped, through
//! the same targeting a tile drag uses; one that never drops leaves the layout as it was.

use tilers_core::{Config, Group, MinSize, PackedState, PanelId};

const WIDTH: f64 = 1600.0;

fn id(s: &str) -> PanelId {
	PanelId(s.into())
}

/// Two half-width tiles side by side; `new` is declared but not hosted.
fn seeded() -> PackedState {
	let mut s = PackedState::new(Config::default());
	s.set_mins(["a", "b", "new"].into_iter().map(|p| (id(p), MinSize::Px { w: 0.0, h: 0.0 })).collect());
	s.set_measure((0.0, 0.0), (WIDTH, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	for p in ["a", "b"] {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, id(p)), 32, 4);
	}
	assert_eq!(s.grid().cells.len(), 2);
	s
}

fn tile_of<'a>(s: &'a PackedState, panel: &str) -> &'a tilers_core::model::packed::Cell {
	let p = id(panel);
	let mut hosts = s.grid().cells.iter().filter(|c| c.group.tabs.contains(&p));
	let cell = hosts.next().unwrap_or_else(|| panic!("{panel} is hosted"));
	assert!(hosts.next().is_none(), "{panel} is hosted once");
	cell
}

#[test]
fn a_carry_dropped_over_the_right_half_lands_there() {
	let mut s = seeded();
	let b = tile_of(&s, "b").clone();
	s.carry(id("new"), 16, 4);
	s.drag_move((WIDTH * 0.75, 400.0)); // `b`'s body: below it, in the columns the pointer is over
	s.drag_release();
	let new = tile_of(&s, "new");
	assert!(new.x >= 32, "dropped over the right half, landed at column {}", new.x);
	assert!(new.y >= b.y + b.h, "dropped over b's body lands below it, not at {}", new.y);
	assert!(s.grid().overlaps().is_none());
}

#[test]
fn a_carry_dropped_on_a_header_joins_that_tile() {
	let mut s = seeded();
	s.carry(id("new"), 16, 4);
	s.drag_move((100.0, 10.0));
	s.drag_release();
	assert_eq!(s.grid().cells.len(), 2, "joined as a tab, no tile of its own");
	assert!(tile_of(&s, "new").group.tabs.contains(&id("a")));
}

#[test]
fn a_carried_panel_already_hosted_moves() {
	let mut s = seeded();
	s.carry(id("a"), 16, 4);
	s.drag_move((WIDTH * 0.75, 10.0));
	s.drag_release();
	assert_eq!(s.grid().cells.len(), 1, "a's tile went with it");
	assert!(tile_of(&s, "a").group.tabs.contains(&id("b")));
}

#[test]
fn a_carry_that_never_drops_leaves_the_layout() {
	let mut s = seeded();
	let before = s.save();
	s.carry(id("new"), 16, 4);
	s.drag_move((WIDTH * 0.75, 400.0));
	assert!(s.carrying());
	s.carry_out();
	s.drag_release(); // a drop elsewhere: never this dock's, but nothing may land if one arrives
	assert_eq!(s.save(), before);

	s.carry(id("new"), 16, 4);
	s.drag_move((WIDTH * 0.75, 400.0));
	s.drag_cancel();
	assert!(!s.carrying());
	assert_eq!(s.save(), before);
}
