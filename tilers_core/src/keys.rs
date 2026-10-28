//! The keymap: a small root the framework claims globally, and a chord trie behind one leader.
//! "Flat" binds are the same trie with the leaves moved to [`Keymap::root`]. See the footprint
//! invariant in `docs/ARCHITECTURE.md`.

use std::rc::Rc;

use crate::state::PackedState;

/// A host-registered command: arbitrary code over the live layout. `Rc` (not `Box`) so
/// [`Config`](crate::Config) stays `Clone` for a binding that hands it around by prop.
pub type Action = Rc<dyn Fn(&mut PackedState)>;

/// One keydown. `key` is a lowercase letter, an uppercase one for Shift, a digit, an ASCII symbol
/// (`"?"`), or a named key (`"Backspace"`). Shift is never a flag: it is already in `key`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Chord {
	pub key: &'static str,
	pub ctrl: bool = false,
	pub alt: bool = false,
	pub meta: bool = false,
}

/// What fires a binding, per platform. Picked once, in [`PackedState::new`]: macOS takes Cmd where
/// others take Ctrl, and composes characters on Option, which makes Alt chords there unreliable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Trigger {
	pub other: Chord,
	pub mac: Chord,
}

impl From<Chord> for Trigger {
	fn from(c: Chord) -> Self {
		Self { other: c, mac: c }
	}
}

#[derive(Clone, PartialEq)]
pub struct Binding {
	pub on: Trigger,
	pub node: Node,
}

#[derive(Clone, PartialEq)]
#[non_exhaustive]
pub enum Node {
	Run(Command),
	Scope { label: &'static str, children: Vec<Binding> },
}

#[derive(Clone)]
#[non_exhaustive]
pub enum Command {
	Builtin(Builtin),
	Custom { label: &'static str, run: Action },
}

/// Closures have no identity, so a custom command is equal by its label.
impl PartialEq for Command {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Builtin(a), Self::Builtin(b)) => a == b,
			(Self::Custom { label: a, .. }, Self::Custom { label: b, .. }) => a == b,
			_ => false,
		}
	}
}

impl Command {
	pub(crate) fn label(&self) -> &'static str {
		match self {
			Self::Builtin(b) => b.label(),
			Self::Custom { label, .. } => label,
		}
	}
}

/// Close, maximize and inspect act on the focused pane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Builtin {
	Undo,
	Redo,
	Close,
	Maximize,
	Inspect,
	Help,
	/// Write this band's layout to `localStorage`. Unbound without [`Config::storage_key`](crate::Config::storage_key).
	Cache,
	/// [`Cache`](Self::Cache) when it is bound, then hand this band's layout to
	/// [`Config::on_publish`](crate::Config::on_publish), which unbinds it when `None`.
	Publish,
}

impl Builtin {
	fn label(self) -> &'static str {
		match self {
			Self::Undo => "Undo",
			Self::Redo => "Redo",
			Self::Close => "Close pane",
			Self::Maximize => "Maximize pane",
			Self::Inspect => "Inspect pane",
			Self::Help => "Keybinds",
			Self::Cache => "Cache layout",
			Self::Publish => "Cache and publish layout",
		}
	}
}

#[derive(Clone, PartialEq)]
pub struct Keymap {
	/// Opens [`scope`](Self::scope). Unbound when `scope` resolves empty.
	pub leader: Trigger,
	pub scope: Vec<Binding>,
	/// Claimed globally, without the leader.
	pub root: Vec<Binding>,
}

impl Default for Keymap {
	fn default() -> Self {
		let run = |on: Trigger, b: Builtin| Binding {
			on,
			node: Node::Run(Command::Builtin(b)),
		};
		let key = |key: &'static str| Trigger::from(Chord { key, .. });
		let cmd = |key: &'static str| Trigger {
			other: Chord { key, ctrl: true, .. },
			mac: Chord { key, meta: true, .. },
		};
		Self {
			leader: Trigger {
				other: Chord { key: "z", alt: true, .. },
				mac: Chord { key: "t", ctrl: true, .. },
			},
			scope: vec![
				run(key("u"), Builtin::Undo),
				run(key("U"), Builtin::Redo),
				run(key("f"), Builtin::Maximize),
				run(key("Backspace"), Builtin::Close),
				run(key("d"), Builtin::Inspect),
				run(key("s"), Builtin::Cache),
				run(key("S"), Builtin::Publish),
				run(key("?"), Builtin::Help),
			],
			root: vec![
				run(cmd("z"), Builtin::Undo),
				run(Chord { key: "y", ctrl: true, .. }.into(), Builtin::Redo),
				run(cmd("Z"), Builtin::Redo),
			],
		}
	}
}

/// A raw keydown, before normalization. Built from a `KeyboardEvent` by its `From` impl on wasm.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyInput {
	pub key: String,
	pub ctrl: bool,
	pub alt: bool,
	pub meta: bool,
	pub shift: bool,
	/// The event's innermost target takes text, so its undo and typing belong to the browser.
	pub editable: bool,
}

#[cfg(target_arch = "wasm32")]
impl From<&web_sys::KeyboardEvent> for KeyInput {
	fn from(e: &web_sys::KeyboardEvent) -> Self {
		use wasm_bindgen::JsCast;
		// `composed_path()[0]`, not `target()`: on `window`, an `<input>` inside a shadow root reports
		// as its host element.
		let editable =
			e.composed_path().get(0).dyn_into::<web_sys::Element>().is_ok_and(|el| {
				matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || el.dyn_ref::<web_sys::HtmlElement>().is_some_and(web_sys::HtmlElement::is_content_editable)
			});
		Self {
			key: e.key(),
			ctrl: e.ctrl_key(),
			alt: e.alt_key(),
			meta: e.meta_key(),
			shift: e.shift_key(),
			editable,
		}
	}
}

impl KeyInput {
	/// The key a [`Chord`] is matched against, or `None` for a modifier-only keydown. Only the `key`
	/// the layout produced, never the physical `code` (docs/ARCHITECTURE.md § Invariants). Letter case
	/// comes from Shift alone, so CapsLock doesn't turn undo into redo.
	pub(crate) fn normalized(&self) -> Option<String> {
		if matches!(self.key.as_str(), "Shift" | "Control" | "Alt" | "Meta" | "AltGraph" | "CapsLock" | "OS" | "Fn") {
			return None;
		}
		let mut chars = self.key.chars();
		Some(match (chars.next(), chars.next()) {
			(Some(c), None) if c.is_ascii_alphabetic() => (if self.shift { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).to_string(),
			_ => self.key.clone(),
		})
	}
}

/// What a binding must do after a keydown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyOutcome {
	/// We acted on the key.
	pub prevent_default: bool,
	/// Anything a binding renders may differ; when `false`, the state is untouched.
	pub changed: bool,
}

/// The `?` modal: every bound leaf as (full sequence, label), and the pane it targets.
#[derive(Clone, Debug, PartialEq)]
pub struct Help {
	pub pane: Option<String>,
	pub rows: Vec<(String, String)>,
}

/// The which-key popup for the scope a chord has opened.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
	pub prefix: String,
	pub rows: Vec<HintRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HintRow {
	pub chord: String,
	pub label: String,
	/// Pressing it opens another level rather than running something.
	pub scope: bool,
}

/// The keymap resolved for one platform and config, with the leader as a scope at the root.
pub(crate) struct Trie {
	root: Level,
	mac: bool,
}

type Level = Vec<(Chord, Item)>;

enum Item {
	Run(Command),
	Scope(&'static str, Level),
}

/// Where a keydown went.
pub(crate) enum Step {
	Opened,
	/// The command, and the sequence that reached it.
	Run(Command, String),
	Miss,
}

impl Trie {
	pub(crate) fn new(k: &Keymap, mac: bool, available: impl Fn(Builtin) -> bool) -> Self {
		let pick = |t: &Trigger| if mac { t.mac } else { t.other };
		fn unique(bs: &[Binding], pick: &dyn Fn(&Trigger) -> Chord) {
			for (i, b) in bs.iter().enumerate() {
				let c = pick(&b.on);
				assert!(!bs[..i].iter().any(|o| pick(&o.on) == c), "{c:?} is bound twice in one scope");
				if let Node::Scope { children, .. } = &b.node {
					unique(children, pick);
				}
			}
		}
		unique(&k.root, &pick);
		unique(&k.scope, &pick);
		assert!(!k.root.iter().any(|b| pick(&b.on) == pick(&k.leader)), "the leader {:?} is also a root chord", pick(&k.leader));

		fn level(bs: &[Binding], pick: &dyn Fn(&Trigger) -> Chord, available: &dyn Fn(Builtin) -> bool) -> Level {
			bs.iter()
				.filter_map(|b| {
					let item = match &b.node {
						Node::Run(Command::Builtin(x)) if !available(*x) => return None,
						Node::Run(c) => Item::Run(c.clone()),
						Node::Scope { label, children } => match level(children, pick, available) {
							l if l.is_empty() => return None,
							l => Item::Scope(label, l),
						},
					};
					Some((pick(&b.on), item))
				})
				.collect()
		}
		let mut root = level(&k.root, &pick, &available);
		let scope = level(&k.scope, &pick, &available);
		if !scope.is_empty() {
			root.push((pick(&k.leader), Item::Scope("leader", scope)));
		}
		Self { root, mac }
	}

	fn at(&self, path: &[usize]) -> &Level {
		path.iter().fold(&self.root, |l, &i| match &l[i].1 {
			Item::Scope(_, children) => children,
			Item::Run(_) => unreachable!("a pending path only ever descends into scopes"),
		})
	}

	/// Advance `pending` by `k`. A miss clears it; a re-pressed root scope chord (a key-repeating
	/// leader) restarts at that scope instead.
	pub(crate) fn step(&self, pending: &mut Vec<usize>, k: &KeyInput, key: &str) -> Step {
		let hit = |l: &Level| l.iter().position(|(c, _)| c.key == key && c.ctrl == k.ctrl && c.alt == k.alt && c.meta == k.meta);
		let i = match hit(self.at(pending)) {
			Some(i) => i,
			None => match hit(&self.root) {
				Some(i) if !pending.is_empty() && matches!(self.root[i].1, Item::Scope(..)) => {
					pending.clear();
					i
				}
				_ => {
					pending.clear();
					return Step::Miss;
				}
			},
		};
		pending.push(i);
		let seq = self.sequence(pending);
		match &self.at(&pending[..pending.len() - 1])[i].1 {
			Item::Scope(..) => Step::Opened,
			Item::Run(c) => {
				pending.clear();
				Step::Run(c.clone(), seq)
			}
		}
	}

	fn sequence(&self, path: &[usize]) -> String {
		let mut out = Vec::new();
		let mut l = &self.root;
		for &i in path {
			out.push(self.render(&l[i].0));
			if let Item::Scope(_, children) = &l[i].1 {
				l = children;
			}
		}
		out.join(" ")
	}

	fn render(&self, c: &Chord) -> String {
		let mut s = String::new();
		for (on, name) in [(c.ctrl, "Ctrl+"), (c.alt, "Alt+"), (c.meta, if self.mac { "Cmd+" } else { "Meta+" })] {
			if on {
				s.push_str(name);
			}
		}
		match c.key.chars().next() {
			Some(ch) if c.key.len() == 1 && ch.is_ascii_uppercase() => {
				s.push_str("Shift+");
				s.push(ch.to_ascii_lowercase());
			}
			_ => s.push_str(c.key),
		}
		s
	}

	pub(crate) fn hint(&self, pending: &[usize]) -> Option<Hint> {
		if pending.is_empty() {
			return None;
		}
		let rows = self
			.at(pending)
			.iter()
			.map(|(c, item)| HintRow {
				chord: self.render(c),
				label: label(item).to_string(),
				scope: matches!(item, Item::Scope(..)),
			})
			.collect();
		Some(Hint {
			prefix: self.sequence(pending),
			rows,
		})
	}

	/// Every leaf, depth-first, as (full sequence, label).
	pub(crate) fn rows(&self) -> Vec<(String, String)> {
		fn walk(t: &Trie, l: &Level, prefix: &str, out: &mut Vec<(String, String)>) {
			for (c, item) in l {
				let seq = if prefix.is_empty() { t.render(c) } else { format!("{prefix} {}", t.render(c)) };
				match item {
					Item::Run(_) => out.push((seq, label(item).to_string())),
					Item::Scope(_, children) => walk(t, children, &seq, out),
				}
			}
		}
		let mut out = Vec::new();
		walk(self, &self.root, "", &mut out);
		out
	}
}

fn label(item: &Item) -> &'static str {
	match item {
		Item::Run(c) => c.label(),
		Item::Scope(label, _) => label,
	}
}
