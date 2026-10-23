//! A panel's content min holds in px across every measure, not only the one it was first resolved at.

use std::collections::HashMap;

use tilers_core::{Config, Group, GroupId, MinSize, PackedState, PanelId};

const ORIGIN: (f64, f64) = (0.0, 0.0);
const WIDTH: f64 = 1600.0;

/// The rendered tile's px height, read off the frame a binding paints.
fn tile_px_h(state: &PackedState) -> f64 {
	let frames = state.frames(&HashMap::new());
	let style = &frames.first().expect("one tile is placed").style;
	let at = style.find("height:").expect("a placed tile has a px height") + "height:".len();
	style[at..style[at..].find("px").expect("px unit") + at].parse().expect("a number")
}

#[test]
fn content_min_holds_as_the_view_shrinks() {
	let content_h = 100.0;
	let mut state = PackedState::new(Config::default());
	let panel = PanelId("table".into());
	state.set_mins(HashMap::from([(panel.clone(), MinSize::Px { w: 0.0, h: content_h })]));
	state.set_measure(ORIGIN, (WIDTH, 974.0), false);
	assert_eq!(state.take_band(), Some(false), "first measure is a miss natively");
	state.place(Group::new(GroupId(0), panel), 8, 4);
	let wanted = tile_px_h(&state);
	assert!(wanted >= content_h + state.title_h(), "placed below its min: {wanted}px");

	state.set_measure(ORIGIN, (WIDTH, 400.0), false);
	let shrunk = tile_px_h(&state);
	assert!(shrunk >= content_h + state.title_h(), "a shorter view squeezed the content: {shrunk}px < {content_h}px + chrome");

	state.set_measure(ORIGIN, (WIDTH, 974.0), false);
	assert_eq!(tile_px_h(&state), wanted, "the tall view didn't restore the wanted height");
}
