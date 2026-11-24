//! A tile's tabs: closing one keeps the tile on the one that shows.

use std::collections::HashMap;

use tilers_core::{Config, Group, MinSize, PackedState, PanelId};

#[test]
fn closing_a_tab_keeps_the_one_shown() {
	let id = |p: &str| PanelId(p.into());
	let mut s = PackedState::new(Config::default());
	s.set_panels(HashMap::from(["a", "c"].map(|p| (id(p), MinSize::Px { w: 0.0, h: 0.0 }))));
	s.set_measure((0.0, 0.0), (1600.0, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	let a = s.mint_group_id();
	s.place(Group::new(a, id("a")), 32, 4);
	s.add_tab(a, id("c"));
	s.close(&id("a"));
	assert_eq!(s.grid().cells[0].group.tabs, [id("c")]);
	assert_eq!(s.grid().cells[0].group.active_panel(), &id("c"));
}
