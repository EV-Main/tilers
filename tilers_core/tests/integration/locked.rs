//! A locked dock holds its user: no tile moves, tears, closes, floats, docks or lands from a carry, and
//! undo waits. Resizing, collapsing, maximizing and opening tabs stay open, and the host keeps every power.

use std::collections::HashMap;

use tilers_core::{Config, Grip, Group, GroupId, MinSize, PackedState, PanelId, keys::KeyInput};

fn id(s: &str) -> PanelId {
	PanelId(s.into())
}

/// `a` (with `c` as a second tab) beside `b`, `d` floating; `new` is declared and not hosted.
fn seeded() -> (PackedState, GroupId, GroupId, GroupId) {
	let mut s = PackedState::new(Config::default());
	let min = MinSize::Px { w: 0.0, h: 0.0 };
	s.set_panels(["a", "b", "c", "d", "new"].into_iter().map(|p| (id(p), min)).collect());
	s.set_measure((0.0, 0.0), (1600.0, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	let [a, b] = ["a", "b"].map(|p| {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, id(p)), 32, 4);
		gid
	});
	s.add_tab(a, id("c"));
	let d = s.mint_group_id();
	s.float(Group::new(d, id("d")), 8, 4);
	s.capture_undo();
	(s, a, b, d)
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

/// Every gesture that would change which tiles exist or where they are.
fn rearrange(s: &mut PackedState, a: GroupId, b: GroupId, d: GroupId) {
	let drag = |s: &mut PackedState, to: (f64, f64)| {
		s.drag_move((to.0 - 10.0, to.1));
		s.drag_move(to);
		assert!(s.ghost(&HashMap::new()).is_none(), "nothing lifts");
		assert!(s.frames(&HashMap::new()).iter().all(|f| !f.shadow && !f.tab_drop), "nothing shows a landing");
		s.drag_release();
	};
	s.press_tile(b, (1200.0, 8.0), (400.0, 8.0));
	assert!(!s.dragging(), "a press on its header is nothing to hold");
	drag(s, (100.0, 600.0));
	s.press_tile(b, (1200.0, 8.0), (400.0, 8.0));
	drag(s, (100.0, 16.0)); // a's header
	s.press_tile(d, (800.0, 450.0), (40.0, 8.0));
	drag(s, (200.0, 200.0));
	s.press_tab(id("a"), a, (20.0, 8.0), (20.0, 8.0));
	drag(s, (1200.0, 600.0));
	for panel in ["a", "new"] {
		s.carry(id(panel), 4, 4);
		assert!(!s.carrying(), "a drop stays the content's");
		drag(s, (1200.0, 600.0));
	}
	for key in ["Backspace", "z"] {
		for gid in [a, b, d] {
			s.focus(gid);
			keys(s, key);
		}
	}
	keys(s, "u");
}

#[test]
fn a_locked_dock_is_where_it_was() {
	let (mut s, a, b, d) = seeded();
	s.drag_enter_tab(a, &id("a")); // the tab press below is a click on it
	s.resize(1, 20, 4);
	s.capture_undo(); // something to undo
	let before = s.grid().clone();
	s.set_locked(true);
	rearrange(&mut s, a, b, d);
	assert_eq!(s.grid(), &before);
	assert!(s.frames(&HashMap::new()).iter().all(|f| f.locked), "what a binding hides");
}

#[test]
fn locking_drops_the_drag_in_flight() {
	let (mut s, _, b, d) = seeded();
	let before = s.grid().clone();
	for (gid, at) in [(b, (1200.0, 8.0)), (d, (800.0, 450.0))] {
		s.press_tile(gid, at, (40.0, 8.0));
		s.drag_move((at.0 - 100.0, at.1 + 100.0));
		s.drag_move((300.0, 500.0));
		s.set_locked(true);
		s.drag_release();
		assert_eq!(s.grid(), &before);
		s.set_locked(false);
	}
}

#[test]
fn resize_collapse_and_maximize_stay_open() {
	let (mut s, a, b, _) = seeded();
	s.set_locked(true);
	let w = s.grid().cells[0].w;
	s.resize_start(a, Grip::Right, 800.0, 100.0);
	s.resize_move((600.0, 100.0));
	s.resize_end();
	assert!(s.grid().cells[0].w < w, "the grip resizes it");
	let spine = |s: &PackedState| s.frames(&HashMap::new()).iter().find(|f| f.gid == a).expect("a renders").collapsed;
	s.collapse(a);
	assert!(spine(&s));
	s.press_tile(a, (8.0, 200.0), (8.0, 200.0));
	s.drag_release();
	assert!(!spine(&s), "a click on a spine opens it");
	s.focus(b);
	keys(&mut s, "f");
	assert_eq!(s.frames(&HashMap::new()).iter().filter(|f| f.maximized).map(|f| f.gid).collect::<Vec<_>>(), [b]);
	keys(&mut s, "f");
	s.press_tab(id("a"), a, (20.0, 8.0), (20.0, 8.0));
	s.drag_release();
	assert_eq!(s.grid().cells[0].group.active, 0, "a click opens a tab");
}

#[test]
fn the_host_keeps_every_power() {
	let (mut s, a, b, d) = seeded();
	s.set_locked(true);
	let gid = s.mint_group_id();
	s.place(Group::new(gid, id("new")), 8, 4);
	assert!(s.grid().cells.iter().any(|c| c.group.id == gid), "it places");
	s.resize(0, 20, 6);
	assert_eq!((s.grid().cells[0].w, s.grid().cells[0].h), (20, 6));
	s.toggle_float(a);
	assert_eq!(s.grid().floats.len(), 2, "it floats");
	s.toggle_float(d);
	assert_eq!(s.grid().floats.len(), 1, "it docks");
	s.close(&id("b"));
	assert!(!s.grid().cells.iter().any(|c| c.group.id == b), "it closes");
}

#[test]
fn unlocking_restores_the_gestures() {
	let (mut s, a, b, _) = seeded();
	s.set_locked(true);
	s.set_locked(false);
	assert!(s.frames(&HashMap::new()).iter().all(|f| !f.locked));
	s.press_tab(id("c"), a, (100.0, 8.0), (20.0, 8.0));
	s.drag_move((110.0, 8.0));
	s.drag_move((1300.0, 700.0));
	s.drag_release();
	assert_eq!(s.grid().cells.iter().find(|c| c.group.id == a).expect("a stays").group.tabs, [id("a")], "c tore out");
	s.carry(id("new"), 4, 4);
	s.drag_move((100.0, 16.0)); // a's header
	s.drag_release();
	assert!(
		s.grid().cells.iter().find(|c| c.group.id == a).expect("a stays").group.tabs.contains(&id("new")),
		"the carry landed"
	);
	s.focus(b);
	keys(&mut s, "Backspace");
	assert!(!s.grid().cells.iter().any(|c| c.group.id == b), "the key closes it");
}
