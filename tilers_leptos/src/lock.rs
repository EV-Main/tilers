//! The lock scope: one switch for every dock inside an area.

use std::sync::Arc;

use leptos::{context::Provider, prelude::*};
use tilers_core::CSS;

/// The area a lock governs: every [`PackedArea`](crate::PackedArea) inside it, nested docks included, is
/// locked while `locked` or an outer scope's is. A locked dock's user can't change which tiles exist, or
/// where or how large they are; maximizing stays open, and the host's calls still act.
#[component]
pub fn LockScope(locked: RwSignal<bool>, children: Children) -> impl IntoView {
	let scope = Scope {
		own: locked,
		outer: use_context::<Scope>().map(Arc::new),
	};
	view! { <Provider value=scope>{children()}</Provider> }
}

/// The switch of the [`LockScope`] it sits in, disabled while an outer scope locks. Panics outside one.
#[component]
pub fn LockToggle() -> impl IntoView {
	let scope = use_context::<Scope>().expect("a LockToggle sits in the LockScope it flips");
	let own = scope.own;
	let state = Memo::new(move |_| (scope.held(), scope.locked()));
	let title = move || match state.get() {
		(true, _) => "Locked by the area around it",
		(false, true) => "Unlock the layout",
		(false, false) => "Lock the layout",
	};
	view! {
		<style>{CSS}</style>
		<button
			type="button"
			class="tl-action tl-lock"
			title=title
			aria-label=title
			aria-pressed=move || if state.get().1 { "true" } else { "false" }
			disabled=move || state.get().0
			on:click=move |_| own.update(|l| *l = !*l)
		></button>
	}
}

/// The nearest [`LockScope`], as context.
#[derive(Clone)]
pub(crate) struct Scope {
	own: RwSignal<bool>,
	outer: Option<Arc<Scope>>,
}
impl Scope {
	pub(crate) fn locked(&self) -> bool {
		self.own.get() || self.held()
	}

	fn held(&self) -> bool {
		self.outer.as_ref().is_some_and(|o| o.locked())
	}
}
