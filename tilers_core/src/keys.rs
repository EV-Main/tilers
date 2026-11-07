//! tilers' commands over [`ev_lib::keymap`]: the default keymap is a root of undo/redo chords and one
//! leader scope. See the footprint invariant in `docs/ARCHITECTURE.md`.

use std::rc::Rc;

pub use ev_lib::keymap::{Binding, Chord, HelpRow, Hint, HintRow, KeyInput, Node, Trigger};

use crate::state::PackedState;

/// A host-registered command: arbitrary code over the live layout. `Rc` (not `Box`) so
/// [`Config`](crate::Config) stays `Clone` for a binding that hands it around by prop.
pub type Action = Rc<dyn Fn(&mut PackedState)>;

pub type Keymap = ev_lib::keymap::Keymap<Command>;

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

impl std::fmt::Display for Command {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::Builtin(b) => b.label(),
			Self::Custom { label, .. } => label,
		})
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

/// What a binding must do after a keydown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyOutcome {
	/// We acted on the key.
	pub prevent_default: bool,
	/// Anything a binding renders may differ; when `false`, the state is untouched.
	pub changed: bool,
}

/// The `?` modal: every bound leaf, and the pane it targets.
#[derive(Clone, Debug, PartialEq)]
pub struct Help {
	pub pane: Option<String>,
	pub rows: Vec<HelpRow>,
}

pub(crate) fn default_keymap() -> Keymap {
	let run = |on: Trigger, b: Builtin| Binding {
		on,
		node: Node::Run(Command::Builtin(b)),
	};
	let key = |key: &'static str| Trigger::from(Chord { key, .. });
	let cmd = |key: &'static str| Trigger {
		other: Chord { key, ctrl: true, .. },
		mac: Chord { key, meta: true, .. },
	};
	Keymap {
		root: vec![
			Binding {
				on: Trigger {
					other: Chord { key: "z", alt: true, .. },
					mac: Chord { key: "t", ctrl: true, .. },
				},
				node: Node::Scope {
					label: "leader".to_owned(),
					children: vec![
						run(key("u"), Builtin::Undo),
						run(key("U"), Builtin::Redo),
						run(key("f"), Builtin::Maximize),
						run(key("Backspace"), Builtin::Close),
						run(key("d"), Builtin::Inspect),
						run(key("s"), Builtin::Cache),
						run(key("S"), Builtin::Publish),
						run(key("?"), Builtin::Help),
					],
				},
			},
			run(cmd("z"), Builtin::Undo),
			run(Chord { key: "y", ctrl: true, .. }.into(), Builtin::Redo),
			run(cmd("Z"), Builtin::Redo),
		],
	}
}
