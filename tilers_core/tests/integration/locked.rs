//! A locked panel can't be closed or moved by the user: its tile doesn't drag, its tab doesn't tear,
//! the close and float keys pass it by, a carry of it lands nowhere. Resizing, collapsing and others
//! joining it stay open, and the host keeps every power.

use std::collections::HashMap;

use tilers_core::{Config, Declared, Group, GroupId, MinSize, PackedState, PanelId, keys::KeyInput};

fn id(s: &str) -> PanelId {
	PanelId(s.into())
}

/// `a` (locked, with `c` as a second tab) beside `b`.
fn seeded() -> (PackedState, GroupId, GroupId) {
	let mut s = PackedState::new(Config::default());
	let min = MinSize::Px { w: 0.0, h: 0.0 };
	s.set_panels(["a", "b", "c"].into_iter().map(|p| (id(p), Declared { min, locked: p == "a" })).collect());
	s.set_measure((0.0, 0.0), (1600.0, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	let gids = ["a", "b"].map(|p| {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, id(p)), 32, 4);
		gid
	});
	s.add_tab(gids[0], id("c"));
	(s, gids[0], gids[1])
}

fn keys(s: &mut PackedState, key: &str) {
	for (alt, key, code) in [(true, "z", "KeyZ"), (false, key, key)] {
		s.on_key(KeyInput {
			key: key.into(),
			code: code.into(),
			ctrl: false,
			alt,
			meta: false,
			shift: false,
			repeat: false,
			composing: false,
			editable: Some(false),
		});
	}
}

#[test]
fn a_locked_tile_stays_where_it_is() {
	let (mut s, a, _) = seeded();
	let before = s.grid().clone();
	s.press_tile(a, (400.0, 8.0), (400.0, 8.0));
	s.drag_move((1200.0, 600.0));
	assert!(s.ghost(&HashMap::new()).is_none(), "nothing lifts");
	s.drag_release();
	s.press_tab(id("a"), a, (20.0, 8.0), (20.0, 8.0));
	s.drag_move((1200.0, 600.0));
	s.drag_release();
	s.carry(id("a"), 4, 4);
	s.drag_move((1200.0, 600.0));
	s.drag_release();
	s.focus(a);
	keys(&mut s, "Backspace");
	keys(&mut s, "z");
	let mut expect = before;
	expect.cells[0].group.active = 0; // the tab press was a click on it
	assert_eq!(s.grid(), &expect);
	let frame = s.frames(&HashMap::new()).into_iter().find(|f| f.gid == a).expect("a renders");
	assert!(frame.locked, "what a binding hides");
	assert!(frame.tabs[0].locked, "what a binding hides");
	assert!(!frame.tabs[1].locked, "what a binding hides");
}

#[test]
fn its_unlocked_neighbour_tears_out_and_others_join_it() {
	let (mut s, a, b) = seeded();
	s.press_tab(id("c"), a, (100.0, 8.0), (20.0, 8.0));
	s.drag_move((110.0, 8.0));
	s.drag_move((1300.0, 600.0));
	s.drag_release();
	assert_eq!(s.grid().cells.iter().find(|c| c.group.id == a).expect("a stays").group.tabs, [id("a")]);
	s.press_tile(b, (1200.0, 8.0), (400.0, 8.0));
	s.drag_move((1210.0, 8.0));
	s.drag_move((100.0, 16.0)); // a's header
	s.drag_release();
	assert!(s.grid().cells.iter().find(|c| c.group.id == a).expect("a stays").group.tabs.contains(&id("b")), "b joined it");
}

#[test]
fn resize_and_collapse_stay_open_and_the_host_keeps_every_power() {
	let (mut s, a, _) = seeded();
	s.resize(0, 20, 6);
	assert_eq!((s.grid().cells[0].w, s.grid().cells[0].h), (20, 6));
	s.collapse(a);
	assert!(s.grid().cells[0].collapsed.is_some());
	s.collapse(a);
	s.toggle_float(a);
	assert_eq!(s.grid().floats.len(), 1, "the host floats it");
	s.close(&id("a"));
	assert!(!s.grid().floats.iter().any(|f| f.group.tabs.contains(&id("a"))), "the host closes it");
}
