//! The lock scope: one switch for every dock inside an area.

use std::rc::Rc;

use dioxus::prelude::*;
use tilers_core::CSS;

/// The area a lock governs: every [`PackedArea`](crate::PackedArea) inside it, nested docks included, is
/// locked while `locked` or an outer scope's is. A locked dock's user can't change which tiles exist, or
/// where or how large they are; maximizing stays open, and the host's calls still act.
#[component]
pub fn LockScope(locked: Signal<bool>, children: Element) -> Element {
	use_hook(|| {
		let outer = try_consume_context::<Scope>().map(Rc::new);
		provide_context(Scope { own: locked, outer })
	});
	children
}

/// The switch of the [`LockScope`] it sits in, disabled while an outer scope locks. Panics outside one.
#[component]
pub fn LockToggle() -> Element {
	let mut scope = use_hook(|| try_consume_context::<Scope>().expect("a LockToggle sits in the LockScope it flips"));
	let (locked, held) = (scope.locked(), scope.held());
	let title = match (held, locked) {
		(true, _) => "Locked by the area around it",
		(false, true) => "Unlock the layout",
		(false, false) => "Lock the layout",
	};
	rsx! {
		style { dangerous_inner_html: CSS }
		button {
			r#type: "button",
			class: "tl-action tl-lock",
			title,
			"aria-label": title,
			"aria-pressed": if locked { "true" } else { "false" },
			disabled: held,
			onclick: move |_| scope.own.toggle(),
		}
	}
}

/// The nearest [`LockScope`], as context.
#[derive(Clone)]
pub(crate) struct Scope {
	own: Signal<bool>,
	outer: Option<Rc<Scope>>,
}
impl Scope {
	pub(crate) fn locked(&self) -> bool {
		(self.own)() || self.held()
	}

	fn held(&self) -> bool {
		self.outer.as_ref().is_some_and(|o| o.locked())
	}
}
