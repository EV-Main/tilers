//! A layout shorter than the view ends at its bottom; a deeper one keeps the least row and scrolls.
//! Sideways a band's layout spans its view at any width the band covers, and a `scroll_x` grid
//! behaves as rows do: a row of tiles at their mins stays one row, ends at the right edge when it is
//! narrower than the view, and runs past it when wider.

use std::collections::HashMap;

use tilers_core::{Config, Grip, Group, GroupId, MinSize, PackedState, PanelId};

const ORIGIN: (f64, f64) = (0.0, 0.0);
const WIDTH: f64 = 1600.0;
const HEIGHT: f64 = 900.0;

fn px(style: &str, key: &str) -> f64 {
	let at = style.find(key).unwrap_or_else(|| panic!("a placed tile has a px {key}")) + key.len();
	let s = style[at..].trim_start_matches("calc("); // inset by the gap
	s[..s.find("px").expect("px unit")].parse().expect("a number")
}

/// Each rendered tile's px `(top, height)`, read off the frames a binding paints.
fn tiles(state: &PackedState) -> Vec<(f64, f64)> {
	state.frames(&HashMap::new()).iter().map(|f| (px(&f.style, "top:"), px(&f.style, "height:"))).collect()
}

/// Each rendered tile's px `(left, top, width)`.
fn spans(state: &PackedState) -> Vec<(f64, f64, f64)> {
	state
		.frames(&HashMap::new())
		.iter()
		.map(|f| (px(&f.style, "left:"), px(&f.style, "top:"), px(&f.style, "width:")))
		.collect()
}

fn right(state: &PackedState) -> f64 {
	spans(state).iter().map(|(l, _, w)| l + w).fold(0.0, f64::max)
}

/// `n` tiles of a 248px min each, placed at their least width, in a `scroll_x` dock `width` wide.
fn columns(n: usize, width: f64) -> PackedState {
	let mut state = PackedState::new(Config {
		scroll_x: true,
		..Default::default()
	});
	let panels: Vec<PanelId> = (0..n).map(|i| PanelId(format!("col{i}"))).collect();
	state.set_panels(panels.iter().map(|p| (p.clone(), MinSize::Px { w: 248.0, h: 0.0 })).collect());
	state.set_measure(ORIGIN, (width, HEIGHT), false);
	assert_eq!(state.take_band(), Some(false), "first measure is a miss natively");
	let rows = state.rows();
	for (i, p) in panels.into_iter().enumerate() {
		state.place(Group::new(GroupId(i as u64), p), 1, rows);
	}
	state
}

fn bottom(state: &PackedState) -> f64 {
	tiles(state).iter().map(|(t, h)| t + h).fold(0.0, f64::max)
}

fn seeded(sizes: &[(u32, u32)]) -> PackedState {
	let mut state = PackedState::new(Config::default());
	let panels: Vec<PanelId> = (0..sizes.len()).map(|i| PanelId(format!("p{i}"))).collect();
	state.set_panels(panels.iter().map(|p| (p.clone(), MinSize::Px { w: 0.0, h: 0.0 })).collect());
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
	assert!(end <= HEIGHT, "the layout ends at {end}px of a {HEIGHT}px view");
	assert!(HEIGHT - end < 1.0, "the layout ends at {end}px of a {HEIGHT}px view");

	state.set_measure(ORIGIN, (WIDTH, 600.0), false);
	let end = bottom(&state);
	assert!(end <= 600.0, "after a resize the layout ends at {end}px of 600px");
	assert!(600.0 - end < 1.0, "after a resize the layout ends at {end}px of 600px");
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
	let g = state.grid().cells[0].group.id;
	state.resize_start(g, Grip::Bottom, 10.0, top + h);
	state.resize_move((10.0, top + h / 2.0));
	let during = tiles(&state);
	assert_eq!(during[1], before[1], "the other tile moved while the pointer held the grip");
	assert!(during[0].1 < h, "the grabbed tile did not follow the pointer");
	state.resize_end();
	let end = bottom(&state);
	assert!(end <= HEIGHT, "the settled layout ends at {end}px of a {HEIGHT}px view");
	assert!(HEIGHT - end < 1.0, "the settled layout ends at {end}px of a {HEIGHT}px view");
}

#[test]
fn a_layout_spans_every_width_its_band_covers() {
	let mut state = PackedState::new(Config::default());
	state.set_panels([("a", 0.0), ("b", 0.0)].into_iter().map(|(p, w)| (PanelId(p.into()), MinSize::Px { w, h: 0.0 })).collect());
	state.set_measure(ORIGIN, (1000.0, HEIGHT), false);
	assert_eq!(state.take_band(), Some(false), "first measure is a miss natively");
	let half = state.cols().div_euclid(2);
	for (i, p) in ["a", "b"].into_iter().enumerate() {
		state.place(Group::new(GroupId(i as u64), PanelId(p.into())), half, 4);
	}
	let json = state.save(); // laid out at the band's narrow end
	for width in [1000.0, 1199.0, 1200.0, 2400.0] {
		state.set_measure(ORIGIN, (width, HEIGHT), false);
		state.load(&json).expect("our own save loads");
		let end = right(&state);
		assert!((width - end).abs() < 1.0, "a layout spanning its band ends at {end}px of a {width}px view");
	}
}

#[test]
fn columns_at_their_mins_stay_one_row_and_run_past_a_narrow_view() {
	let state = columns(8, 1200.0);
	let spans = spans(&state);
	assert!(spans.iter().all(|&(_, top, _)| top == 0.0), "a column wrapped: {spans:?}");
	assert!(spans.iter().all(|&(_, _, w)| w >= 248.0), "a column squeezed below its min: {spans:?}");
	assert!(right(&state) > 1200.0, "eight 248px columns fit a 1200px view: {spans:?}");
}

#[test]
fn a_stretched_column_shrinks_down_to_its_min_as_rendered() {
	let mut state = columns(3, 1600.0);
	let (left, top, w) = spans(&state)[1];
	assert!(w > 2.0 * 248.0, "three columns stretched across 1600px: {w}px");
	let g = state.grid().cells[1].group.id;
	state.resize_start(g, Grip::Right, left + w, top + 100.0);
	state.resize_move((left + 260.0, top + 100.0));
	state.resize_end();
	let shrunk = spans(&state)[1].2;
	assert!(shrunk < 300.0, "dragged from {w}px to 260px, it renders {shrunk}px");
	assert!(shrunk >= 248.0, "shrunk below its 248px min: {shrunk}px");
}

#[test]
fn columns_narrower_than_the_view_end_at_its_right_edge() {
	let mut state = columns(3, 1600.0);
	let end = right(&state);
	assert!(end <= 1600.0, "three columns end at {end}px of a 1600px view");
	assert!(1600.0 - end < 1.0, "three columns end at {end}px of a 1600px view");
	state.set_measure(ORIGIN, (2000.0, HEIGHT), false);
	let end = right(&state);
	assert!(end <= 2000.0, "widened, they end at {end}px of 2000px");
	assert!(2000.0 - end < 1.0, "widened, they end at {end}px of 2000px");
}
