//! A layout shorter than the view ends at its bottom; a deeper one keeps the least row and scrolls.

use std::collections::HashMap;

use tilers_core::{Config, Grip, Group, GroupId, MinSize, PackedState, PanelId};

const ORIGIN: (f64, f64) = (0.0, 0.0);
const WIDTH: f64 = 1600.0;
const HEIGHT: f64 = 900.0;

/// Each rendered tile's px `(top, height)`, read off the frames a binding paints.
fn tiles(state: &PackedState) -> Vec<(f64, f64)> {
	let px = |style: &str, key: &str| -> f64 {
		let at = style.find(key).unwrap_or_else(|| panic!("a placed tile has a px {key}")) + key.len();
		style[at..style[at..].find("px").expect("px unit") + at].parse().expect("a number")
	};
	state.frames(&HashMap::new()).iter().map(|f| (px(&f.style, "top:"), px(&f.style, "height:"))).collect()
}

fn bottom(state: &PackedState) -> f64 {
	tiles(state).iter().map(|(t, h)| t + h).fold(0.0, f64::max)
}

fn seeded(sizes: &[(u32, u32)]) -> PackedState {
	let mut state = PackedState::new(Config::default());
	let panels: Vec<PanelId> = (0..sizes.len()).map(|i| PanelId(format!("p{i}"))).collect();
	state.set_mins(panels.iter().map(|p| (p.clone(), MinSize::Px { w: 0.0, h: 0.0 })).collect());
	state.set_measure(ORIGIN, (WIDTH, HEIGHT), false);
	assert_eq!(state.take_band(), Some(false), "first measure is a miss natively");
	for (i, (p, &(w, h))) in panels.into_iter().zip(sizes).enumerate() {
		state.place(Group::new(GroupId(i as u64), p), w, h);
	}
	state
}

#[test]
fn a_short_layout_ends_at_the_views_bottom() {
	let mut state = seeded(&[(32, 4), (32, 3), (32, 2)]);
	let end = bottom(&state);
	assert!(end <= HEIGHT && HEIGHT - end < 1.0, "the layout ends at {end}px of a {HEIGHT}px view");

	state.set_measure(ORIGIN, (WIDTH, 600.0), false);
	let end = bottom(&state);
	assert!(end <= 600.0 && 600.0 - end < 1.0, "after a resize the layout ends at {end}px of 600px");
}

#[test]
fn a_deep_layout_keeps_the_least_row_and_scrolls() {
	let mut state = seeded(&[(64, 10)]);
	state.set_measure(ORIGIN, (WIDTH, 100.0), false);
	let (_, h) = tiles(&state)[0];
	assert!(h > 100.0, "a 10-row tile squeezed into a 100px view: {h}px");
	state.set_measure(ORIGIN, (WIDTH, 200.0), false);
	assert_eq!(tiles(&state)[0].1, h, "a view still shorter than the layout changed its rows");
}

#[test]
fn rows_hold_still_under_a_resize_and_fill_after_it() {
	let mut state = seeded(&[(32, 6), (32, 3)]);
	let before = tiles(&state);
	let (top, h) = before[0];
	state.resize_start(0, Grip::Bottom, 10.0, top + h);
	state.resize_move((10.0, top + h / 2.0));
	let during = tiles(&state);
	assert_eq!(during[1], before[1], "the other tile moved while the pointer held the grip");
	assert!(during[0].1 < h, "the grabbed tile did not follow the pointer");
	state.resize_end();
	let end = bottom(&state);
	assert!(end <= HEIGHT && HEIGHT - end < 1.0, "the settled layout ends at {end}px of a {HEIGHT}px view");
}
