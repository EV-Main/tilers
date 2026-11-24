//! The keyboard surface, through the same `on_key` a binding feeds: what the framework claims
//! globally, what waits behind the leader, and how a keystroke is identified across layouts.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use tilers_core::{
	Config, Grip, Group, GroupId, MinSize, PackedState, PanelId, Step,
	keys::{Binding, Chord, Command, HelpRow, KeyInput, KeyOutcome, Keymap, Node},
	model::packed::PackedGrid,
};

/// `"Alt+z s Shift+u !Ctrl+z"`: space-separated keydowns, `+`-joined modifiers, a leading `!` aims
/// the key at an editable. A bare `Shift` is a modifier-only keydown; an uppercase letter is shifted.
fn seq(s: &str) -> Vec<KeyInput> {
	s.split_whitespace()
		.map(|tok| {
			let (editable, tok) = tok.strip_prefix('!').map_or((Some(false), tok), |t| (Some(true), t));
			let mut parts: Vec<&str> = tok.split('+').collect();
			let key = parts.pop().expect("split yields at least one part");
			let has = |m: &str| parts.contains(&m);
			let shift = has("Shift") || (key.len() == 1 && key.chars().all(|c| c.is_ascii_uppercase()));
			let key = if shift && key.len() == 1 { key.to_ascii_uppercase() } else { key.to_string() };
			let code = match key.chars().next() {
				Some(c) if key.len() == 1 && c.is_ascii_alphabetic() => format!("Key{}", c.to_ascii_uppercase()),
				_ => key.clone(),
			};
			KeyInput {
				key,
				code,
				ctrl: has("Ctrl"),
				alt: has("Alt"),
				meta: has("Meta"),
				shift,
				repeat: false,
				composing: false,
				editable,
			}
		})
		.collect()
}

/// Two tiles, tile 0 focused, and one structural edit in the undo history — fed the way a binding's
/// undo effect feeds it. Returns the layout before the edit, which is where an undo lands.
fn edited(cfg: Config) -> (PackedState, PackedGrid) {
	let mut s = seeded(cfg);
	let before = s.grid().clone();
	s.resize(0, 20, 12);
	s.capture_undo();
	assert_ne!(s.grid(), &before, "the edit must be visible for an undo to be observable");
	(s, before)
}

fn seeded(cfg: Config) -> PackedState {
	let mut s = PackedState::new(cfg);
	let min = MinSize::Steps { w: Step(1), h: Step(1) };
	s.set_panels((0..2).map(|i| (PanelId(format!("p{i}")), min)).collect());
	s.set_measure((0.0, 0.0), (1600.0, 900.0), false);
	assert_eq!(s.take_band(), Some(false), "first measure is a miss natively");
	for i in 0..2 {
		let gid = s.mint_group_id();
		s.place(Group::new(gid, PanelId(format!("p{i}"))), 10, 8);
	}
	s.capture_undo();
	s.focus(GroupId(0));
	s
}

fn press(s: &mut PackedState, keys: Vec<KeyInput>) -> Vec<KeyOutcome> {
	keys.into_iter().map(|k| s.on_key(k)).collect()
}

#[derive(Debug, PartialEq)]
enum Lands {
	Edited,
	Undone,
}

/// Press `keys` on the [`edited`] fixture and assert where the layout ends up, and whether the last
/// keydown was `preventDefault`ed.
#[track_caller]
fn check(cfg: Config, keys: Vec<KeyInput>, lands: Lands, prevented: bool) {
	let (mut s, before) = edited(cfg);
	let after_edit = s.grid().clone();
	let outs = press(&mut s, keys);
	let got = match s.grid() {
		g if *g == before => Lands::Undone,
		g if *g == after_edit => Lands::Edited,
		g => panic!("landed on a layout that was never captured: {:?}", g.cells),
	};
	assert_eq!(got, lands);
	assert_eq!(outs.last().expect("at least one key").prevent_default, prevented, "last keydown's preventDefault");
}

fn help_rows(s: &mut PackedState) -> Vec<HelpRow> {
	if s.help(&HashMap::new()).is_none() {
		press(s, seq("Alt+z ?"));
	}
	s.help(&HashMap::new()).expect("leader ? opens the help").rows
}

fn flat() -> Keymap {
	let mut k = Config::default().keymap;
	let Node::Scope { children, .. } = k.root.remove(0).node else {
		unreachable!("the default keymap's first binding is the leader scope")
	};
	k.root.extend(children);
	k
}

#[test]
fn bare_keys_are_not_ours() {
	for keys in ["u", "U", "f", "d", "?", "Backspace", "Alt+s", "Alt+S", "Shift U"] {
		let (mut s, _) = edited(Config::default());
		let grid = s.grid().clone();
		for out in press(&mut s, seq(keys)) {
			assert!(!out.prevent_default, "{keys:?} must reach the host untouched");
			assert!(!out.changed, "{keys:?} must reach the host untouched");
		}
		assert_eq!(s.grid(), &grid, "{keys:?}");
		assert!(s.help(&HashMap::new()).is_none(), "{keys:?}");
		assert!(s.hint().is_none(), "{keys:?}");
		assert!(s.inspect().is_empty(), "{keys:?}");
	}
}

#[test]
fn root_undo() {
	check(Config::default(), seq("Alt+z u"), Lands::Undone, true);
	check(Config::default(), seq("!Alt+z !u"), Lands::Undone, true);
	check(Config::default(), seq("Ctrl+z"), Lands::Undone, true);
	check(Config::default(), seq("Ctrl+z Ctrl+Z"), Lands::Edited, true);
	check(Config::default(), seq("Ctrl+z Ctrl+y"), Lands::Edited, true);

	let (mut s, _) = edited(Config::default());
	let out = press(&mut s, seq("!Ctrl+z")).remove(0);
	assert!(!out.prevent_default, "text undo inside an editable stays the browser's");
	assert!(!out.changed, "text undo inside an editable stays the browser's");

	let mut s = seeded(Config::default());
	let out = press(&mut s, seq("Ctrl+z")).remove(0);
	assert!(!out.prevent_default, "nothing to undo: the keystroke stays the host's");
}

#[test]
fn saving_tracks_what_the_host_wired() {
	let (mut s, _) = edited(Config::default());
	for keys in ["Alt+z s", "Alt+z S"] {
		assert!(!press(&mut s, seq(keys))[1].prevent_default, "no storage, no hook: {keys} is unbound");
	}
	assert!(
		!help_rows(&mut s).iter().any(|r| r.sequence == "Alt+z s" || r.sequence == "Alt+z Shift+s"),
		"{:?}",
		help_rows(&mut s)
	);

	let cached = Config {
		storage_key: Some("t".into()),
		..Default::default()
	};
	let (mut s, _) = edited(cached);
	let rows = help_rows(&mut s);
	assert!(rows.iter().any(|r| r.sequence == "Alt+z s"), "{rows:?}");
	assert!(!rows.iter().any(|r| r.sequence == "Alt+z Shift+s"), "publish needs `on_publish`: {rows:?}");
	let outs = press(&mut s, seq("Alt+z S"));
	assert!(!outs[1].prevent_default, "an unbound publish is inert");
	assert!(s.notice().is_none(), "an unbound publish says nothing");
	press(&mut s, seq("Alt+z s"));
	let cached = s.notice().expect("caching says so without any host code");
	assert!(cached.text.contains(&s.band().to_string()), "{cached:?}");
	press(&mut s, seq("Alt+z"));
	assert!(s.notice().is_none(), "an opened chord's hint takes the notice's corner");
	press(&mut s, seq("s"));
	assert_ne!(s.notice().expect("cached again").seq, cached.seq, "a repeat is a new notice, so it shows again");

	let got = Rc::new(RefCell::new(Vec::new()));
	let sink = got.clone();
	let hooked = Config {
		on_publish: Some(Rc::new(move |band, json| sink.borrow_mut().push((band, json)))),
		..Default::default()
	};
	let (mut s, _) = edited(hooked.clone());
	assert!(press(&mut s, seq("Alt+z S"))[1].prevent_default);
	match got.borrow().as_slice() {
		[(band, json)] => assert_eq!((*band, json), (s.band(), &s.save())),
		_ => panic!("publish hands exactly one layout to the host"),
	}
	assert!(!press(&mut s, seq("Alt+z s"))[1].prevent_default, "no storage key: caching would write nothing");
	assert!(s.notice().is_none(), "no storage key: nothing cached, and a publish's outcome is the host's to say");

	let (mut s, _) = edited(Config {
		storage_key: Some("t".into()),
		..hooked
	});
	press(&mut s, seq("Alt+z S"));
	assert!(s.notice().is_some(), "publishing caches first");
	assert_eq!(got.borrow().len(), 2, "and still hands the layout to the host");
}

#[test]
fn flat_keymap_has_no_leader() {
	let cfg = || Config {
		keymap: flat(),
		..Default::default()
	};
	check(cfg(), seq("u"), Lands::Undone, true);
	let (mut s, _) = edited(cfg());
	let out = press(&mut s, seq("Alt+z")).remove(0);
	assert!(!out.prevent_default, "an empty scope leaves the leader unbound");
	assert!(s.hint().is_none(), "an empty scope leaves the leader unbound");
}

#[test]
fn custom_command_in_a_nested_scope() {
	let ran = Rc::new(RefCell::new(0));
	let count = ran.clone();
	let mut keymap = Config::default().keymap;
	let Node::Scope { children, .. } = &mut keymap.root[0].node else {
		unreachable!("the default keymap's first binding is the leader scope")
	};
	children.push(Binding {
		on: Chord { key: "x", .. }.into(),
		node: Node::Scope {
			label: "extra".to_owned(),
			children: vec![Binding {
				on: Chord { key: "y", .. }.into(),
				node: Node::Run {
					id: "mark".to_owned(),
					command: Command::Custom {
						label: "Mark",
						run: Rc::new(move |s: &mut PackedState| {
							assert!(s.grid().cells.len() == 2, "runs against the live layout");
							*count.borrow_mut() += 1;
						}),
					},
				},
			}],
		},
	});
	let (mut s, _) = edited(Config { keymap, ..Default::default() });
	assert!(press(&mut s, seq("Alt+z x y")).iter().all(|o| o.prevent_default));
	assert_eq!(*ran.borrow(), 1);
	assert!(help_rows(&mut s).contains(&HelpRow {
		id: "mark".into(),
		sequence: "Alt+z x y".into(),
		label: "Mark".into()
	}));
}

#[test]
fn help_names_the_pane_it_was_opened_on() {
	let (mut s, _) = edited(Config::default());
	s.open_help(GroupId(1));
	let titles = HashMap::from([(PanelId("p1".into()), "Orders".to_string())]);
	assert_eq!(s.help(&titles).expect("open_help opens it").pane.as_deref(), Some("Orders"));
	press(&mut s, seq("Escape"));
	assert!(s.help(&titles).is_none());
}

#[test]
fn inspect_and_maximize_act_on_the_focused_pane() {
	let (mut s, _) = edited(Config::default());
	press(&mut s, seq("Alt+z d"));
	assert_eq!(s.inspect().len(), 1, "inspect opens on the focused pane");
	press(&mut s, seq("Alt+z f"));
	assert_eq!(s.frames(&HashMap::new()).len(), 1, "maximize renders the focused pane alone");
}

#[test]
fn undo_mid_resize_leaves_the_gesture_intact() {
	let (mut s, _) = edited(Config::default());
	let g = s.grid().cells[1].group.id;
	s.resize_start(g, Grip::Corner, 500.0, 500.0);
	s.resize_move((560.0, 540.0));
	let mid = s.grid().clone();
	for keys in ["Ctrl+z", "Alt+z u", "Alt+z Backspace"] {
		let outs = press(&mut s, seq(keys));
		assert!(!outs.last().expect("one key").prevent_default, "{keys:?} mid-resize must not act");
		assert_eq!(s.grid(), &mid, "{keys:?}");
	}
	s.resize_move((620.0, 580.0));
	s.resize_end();
	assert!(s.grid().overlaps().is_none());
}

#[test]
fn only_the_dock_last_pressed_in_answers() {
	let (mut outer, _) = edited(Config::default());
	let (mut inner, _) = edited(Config::default());
	assert!(press(&mut outer, seq("Alt+z"))[0].prevent_default, "before any press every un-nested dock answers");
	press(&mut outer, seq("Escape"));
	inner.claim_keys();
	assert!(!press(&mut outer, seq("Alt+z"))[0].prevent_default, "the other dock was pressed last");
	assert!(press(&mut inner, seq("Alt+z"))[0].prevent_default);
	outer.claim_keys();
	let left = press(&mut inner, seq("u")).remove(0);
	assert!(!left.prevent_default, "a chord open in a dock pressed away from closes, and the key passes");
	assert!(left.changed, "a chord open in a dock pressed away from closes, and the key passes");
	drop(outer);
	assert!(press(&mut inner, seq("Alt+z"))[0].prevent_default, "the owner is gone: un-nested docks answer again");
}
