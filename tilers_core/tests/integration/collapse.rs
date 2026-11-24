//! A collapsed tile is a spine as wide as the title bar is tall: it keeps the width it opens back to,
//! takes a drop anywhere along it, and a click opens it.

use tilers_core::{Config, Declared, Group, GroupId, MinSize, PackedState, PanelId};

const WIDTH: f64 = 1600.0;

fn id(s: &str) -> PanelId {
	PanelId(s.into())
}

/// Two half-width tiles side by side, `a` then `b`; `c` is declared but not hosted.
fn seeded() -> (PackedState, GroupId, GroupId) {
	let mut s = PackedState::new(Config::default());
	s.set_panels(
		["a", "b", "c"]
			.into_iter()
			.map(|p| {
				(
					id(p),
					Declared {
						min: MinSize::Px { w: 0.0, h: 0.0 },
						locked: false,
					},
				)
			})
			.collect(),
	);
	s.set_measure((0.0, 0.0), (WIDTH, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	let gids = ["a", "b"].map(|p| {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, id(p)), 32, 4);
		gid
	});
	(s, gids[0], gids[1])
}

fn tile(s: &PackedState, gid: GroupId) -> &tilers_core::model::packed::Cell {
	s.grid().cells.iter().find(|c| c.group.id == gid).expect("the tile is in the layout")
}

#[test]
fn a_spine_takes_a_drop_along_its_length() {
	let (mut s, a, b) = seeded();
	s.collapse(a);
	let spine = (s.title_h() / (WIDTH / s.cols() as f64)).ceil() as u32;
	assert_eq!((tile(&s, a).w, tile(&s, a).collapsed), (spine, Some(32)));

	s.press_tile(b, (WIDTH * 0.75, 10.0), (10.0, 10.0));
	s.drag_move((10.0, 600.0)); // the spine's body, far below its title band
	s.drag_release();
	assert_eq!(s.grid().cells.len(), 1, "b joined the spine as a tab");
	assert_eq!(tile(&s, a).group.tabs, [id("a"), id("b")]);
	assert_eq!(tile(&s, a).collapsed, Some(32), "joining a spine leaves it one");
}

#[test]
fn a_click_opens_a_spine_to_the_width_it_kept() {
	let (mut s, a, _) = seeded();
	s.collapse(a);
	s.press_tab(id("a"), a, (10.0, 10.0), (5.0, 5.0));
	s.drag_release();
	assert_eq!((tile(&s, a).w, tile(&s, a).collapsed), (32, None));
}

#[test]
fn closing_a_tab_keeps_the_one_shown() {
	let (mut s, a, _) = seeded();
	s.add_tab(a, id("c"));
	s.close(&id("a"));
	assert_eq!(tile(&s, a).group.tabs, [id("c")]);
	assert_eq!(tile(&s, a).group.active_panel(), &id("c"));
}
