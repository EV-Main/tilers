//! The Dioxus view over [`PackedState`]: one root-scope `Signal<PackedState>` replacing the old
//! ~14 separate signals, three `rsx!` blocks (root / frame / content) rendered straight from the
//! core view-model getters, and event handlers that each read one DOM primitive then forward it to
//! a reducer method. No layout logic lives here — only the reactive cell, the templates, and the
//! DOM reads the core can't do itself.

use std::collections::HashMap;

use dioxus::{html::input_data::MouseButton, prelude::*};
use tilers_core::{Band, CSS, Config, Declared, Frame, Group, GroupId, PackedState, PanelId};

use crate::panel::DockPanel;

/// Imperative handle over the packed layout, handed to the host via `on_band`: a thin wrapper
/// over the root-scope `Signal<PackedState>` so the host can script tiles, save/load, and read the
/// live band from *outside* [`PackedArea`]'s subtree. Every method is a `read`/`write` of the one
/// cell; the reducer does the work.
#[derive(Clone, Copy, PartialEq)]
pub struct PackedApi {
	state: Signal<PackedState>,
	panels: Signal<Vec<DockPanel>>,
}

impl PackedApi {
	/// `group`'s panels must already be in `panels`: their declared mins floor the tile.
	pub fn place(&mut self, group: Group, w: u32, h: u32) {
		let declared = declared(&self.panels.peek());
		let mut s = self.state.write();
		s.set_panels(declared);
		s.place(group, w, h);
	}

	/// Open `group` floating above the grid at a wanted `w×h`: centered in the view, on top, focused.
	/// Its panels must already be in `panels`.
	pub fn float(&mut self, group: Group, w: u32, h: u32) {
		let declared = declared(&self.panels.peek());
		let mut s = self.state.write();
		s.set_panels(declared);
		s.float(group, w, h);
	}

	/// Float tile `group` out where it shows, or dock a float where it shows.
	pub fn toggle_float(&mut self, group: GroupId) {
		self.state.write().toggle_float(group);
	}

	pub fn add_tab(&mut self, group: GroupId, panel: PanelId) {
		let declared = declared(&self.panels.peek());
		let mut s = self.state.write();
		s.set_panels(declared);
		s.add_tab(group, panel);
	}

	/// A native drag the host started carries `panel`: dropped on this dock it lands where it drops, a
	/// `w×h` tile or a tab of the strip it drops on; a panel already hosted moves there. Call it from
	/// the drag source's `ondragstart`. `panel` must already be in `panels`.
	pub fn carry(&mut self, panel: PanelId, w: u32, h: u32) {
		let declared = declared(&self.panels.peek());
		let mut s = self.state.write();
		s.set_panels(declared);
		s.carry(panel, w, h);
	}

	/// Close `panel`'s tab, and its tile with its last one. A locked panel too: the lock holds the user, not the host.
	pub fn close(&mut self, panel: &PanelId) {
		self.state.write().close(panel);
	}

	/// Toggle `group` filling the dock.
	pub fn maximize(&mut self, group: GroupId) {
		self.state.write().maximize(group);
	}

	/// Toggle `group` between its tile and a spine as wide as the title bar is tall, which keeps the
	/// width it opens back to. A click on a spine opens it too.
	pub fn collapse(&mut self, group: GroupId) {
		self.state.write().collapse(group);
	}

	pub fn resize(&mut self, idx: usize, new_w: u32, new_h: u32) {
		self.state.write().resize(idx, new_w, new_h);
	}

	pub fn mint_group_id(&mut self) -> GroupId {
		self.state.write().mint_group_id()
	}

	/// The grid's column count: the view's, or with [`Config::scroll_x`] a bound far past it, so a
	/// tile there is placed at its least width rather than a share of this.
	pub fn cols(&self) -> u32 {
		self.state.read().cols()
	}

	/// Whole rows the container shows.
	pub fn rows(&self) -> u32 {
		self.state.read().rows()
	}

	/// The band the layout is stored under.
	pub fn band(&self) -> Band {
		self.state.read().band()
	}

	/// Whether `on_band`'s invocation follows a restore from the seed cache. Only meaningful inside
	/// `on_band` — a fresh band arrives with an empty grid, and anything the host places fills it.
	pub fn restored(&self) -> bool {
		let s = self.state.read();
		!(s.grid().cells.is_empty() && s.grid().floats.is_empty())
	}

	/// Wipe the layout back to empty (a host re-seeding a fresh band).
	pub fn reset(&mut self) {
		self.state.write().reset();
	}

	/// Every panel id the live layout hosts, tiles' then floats', in tab order — for a host rebuilding
	/// its panel list from a restored layout.
	pub fn tab_ids(&self) -> Vec<PanelId> {
		let s = self.state.read();
		let g = s.grid();
		g.cells
			.iter()
			.map(|c| &c.group)
			.chain(g.floats.iter().map(|f| &f.group))
			.flat_map(|g| g.tabs.iter().cloned())
			.collect()
	}

	/// Serialize the current layout.
	pub fn save(&self) -> String {
		self.state.read().save()
	}

	/// Replace the layout with a previously [`save`](Self::save)d one; errors on a corrupt or
	/// future-version payload. Every panel it hosts must already be in `panels`.
	pub fn load(&mut self, json: &str) -> Result<(), tilers_core::model::serial::LoadError> {
		let declared = declared(&self.panels.peek());
		let mut s = self.state.write();
		s.set_panels(declared);
		s.load(json)
	}

	/// Say `text` in the dock's toast: a [`Config::on_publish`] host reporting how its publish went.
	pub fn notify(&mut self, text: String) {
		self.state.write().notify(text);
	}
}

/// Root of the packed layout. Owns the `Signal<PackedState>` (root-scope, so the [`PackedApi`] it
/// hands the host outlives this component), installs the window keydown/blur listeners, measures
/// its own box, and stacks the tile skeleton over the content overlay.
///
/// - `panels` is a `Signal` so windows spawned at runtime appear in the overlay.
/// - `on_band`: invoked with the [`PackedApi`] once per [`Band`] entry — after the first measure and
///   again on every band crossing — *after* the framework has resolved that band's seed cache.
///   [`PackedApi::restored`] says whether it found one; if not, the grid is empty and the host is
///   expected to place its tiles (or load its own default) against the now-real step size.
/// - A tile's header is the framework's: its tabs (each with its `×`), then `?`, `+`, float/dock and
///   close. `actions` adds the host's own buttons before those, per tile; it never replaces or hides
///   them — a panel's [`DockPanel::locked`] does that. A press anywhere in them is never the tile's.
///   It runs inside this component's render, so hooks belong in a component it returns.
/// - `on_add`: the header's `+` asks the host to open a tab in that tile; without it there is no `+`.
#[component]
pub fn PackedArea(
	panels: Signal<Vec<DockPanel>>,
	on_band: Option<Callback<PackedApi>>,
	config: Option<Config>,
	actions: Option<Callback<Tile, Element>>,
	on_add: Option<Callback<GroupId>>,
) -> Element {
	let cfg = config.unwrap_or_default();
	let scroll_x = cfg.scroll_x;
	// Root-scope, not this scope: `PackedApi` is driven from outside `PackedArea`'s subtree, so the
	// cell must outlive this component (this root component never unmounts).
	let mut state = use_hook(move || Signal::new_in_scope(PackedState::new(cfg), ScopeId::ROOT));
	let api = PackedApi { state, panels };

	#[cfg(target_arch = "wasm32")]
	let mut root = use_signal(|| None::<web_sys::Element>);

	// Keybinds are app-level: a `window` keydown listener (not an element `onkeydown`, which fires
	// only while the dock subtree holds DOM focus — it usually doesn't), plus `blur` to drop a chord
	// the window stopped hearing. Both are removed on unmount.
	#[cfg(target_arch = "wasm32")]
	let listeners = use_hook(move || {
		use wasm_bindgen::{JsCast, closure::Closure};
		let window = web_sys::window().expect("a browser window");
		let key = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
			// Every keystroke typed into any page input lands here, so the write's notification is
			// dropped unless something a render reads moved.
			let (mut s, notify) = state.write_unchecked().into_parts();
			let out = s.on_key((&e).into());
			drop(s);
			if out.changed {
				drop(notify);
			} else {
				std::mem::forget(notify);
			}
			if out.prevent_default {
				e.prevent_default();
			}
		});
		// Capture phase: fire before any descendant (Google Maps, Plotly, …) can `stopPropagation` a
		// keydown on its way up to `window`, which would otherwise silence every bind.
		window
			.add_event_listener_with_callback_and_bool("keydown", key.as_ref().unchecked_ref(), true)
			.expect("add keydown listener");
		let mut state = state;
		let blur = Closure::<dyn FnMut()>::new(move || state.write().cancel_chord());
		window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref()).expect("add blur listener");
		// A carry's drag ended: dropped here, `drop` came first and released it; anywhere else, it lands nowhere.
		let dragend = Closure::<dyn FnMut()>::new(move || {
			if state.peek().carrying() {
				state.write().drag_cancel();
			}
		});
		window
			.add_event_listener_with_callback("dragend", dragend.as_ref().unchecked_ref())
			.expect("add dragend listener");
		// Capture phase, so of nested docks the innermost claims last, and a tile's own `stopPropagation` can't skip it.
		let press = Closure::<dyn FnMut()>::new(move || state.peek().claim_keys());
		std::rc::Rc::new((key, blur, dragend, press))
	});
	#[cfg(target_arch = "wasm32")]
	let press = listeners.clone();
	#[cfg(target_arch = "wasm32")]
	use_drop(move || {
		use wasm_bindgen::JsCast;
		let window = web_sys::window().expect("a browser window");
		window
			.remove_event_listener_with_callback_and_bool("keydown", listeners.0.as_ref().unchecked_ref(), true)
			.expect("remove keydown listener");
		window
			.remove_event_listener_with_callback("blur", listeners.1.as_ref().unchecked_ref())
			.expect("remove blur listener");
		window
			.remove_event_listener_with_callback("dragend", listeners.2.as_ref().unchecked_ref())
			.expect("remove dragend listener");
	});

	// A frame loop for as long as a drag is live, so a pointer held at an edge keeps scrolling.
	#[cfg(target_arch = "wasm32")]
	let frames = use_hook(move || {
		use std::{cell::RefCell, rc::Rc};

		use wasm_bindgen::{JsCast, closure::Closure};
		let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::default();
		let pending: Rc<RefCell<Option<i32>>> = Rc::default();
		let request = {
			let tick = tick.clone();
			move || {
				let t = tick.borrow();
				let f = t.as_ref().expect("dropped only at unmount, which cancels the pending frame first");
				web_sys::window()
					.expect("a browser window")
					.request_animation_frame(f.as_ref().unchecked_ref())
					.expect("requestAnimationFrame never throws")
			}
		};
		*tick.borrow_mut() = Some(Closure::new({
			let (pending, request) = (pending.clone(), request.clone());
			move || {
				// An idle frame changes nothing, so its write's notification is dropped.
				let (mut s, notify) = state.write_unchecked().into_parts();
				let changed = s.autoscroll();
				let live = s.dragging();
				drop(s);
				if changed {
					drop(notify);
				} else {
					std::mem::forget(notify);
				}
				*pending.borrow_mut() = live.then(&request);
			}
		}));
		Rc::new((tick, pending, request))
	});
	#[cfg(target_arch = "wasm32")]
	use_effect({
		let frames = frames.clone();
		move || {
			if state.read().dragging() && frames.1.borrow().is_none() {
				*frames.1.borrow_mut() = Some((frames.2)());
			}
		}
	});
	#[cfg(target_arch = "wasm32")]
	use_drop(move || {
		if let Some(id) = frames.1.take() {
			web_sys::window()
				.expect("a browser window")
				.cancel_animation_frame(id)
				.expect("cancelAnimationFrame never throws");
		}
		frames.0.take(); // the closure holds `request`, which holds the closure
	});

	// A panel's declaration is host intent the state re-derives floors and locks from.
	use_effect(move || {
		let declared = declared(&panels.read());
		state.write().set_panels(declared);
	});

	// Undo history: snapshot a settled layout after each structural edit. `wants_undo_snapshot` is
	// read (to subscribe) before any write, so the resulting push doesn't re-fire this effect.
	use_effect(move || {
		if state.read().wants_undo_snapshot() {
			state.write().capture_undo();
		}
	});

	let titles: HashMap<PanelId, String> = panels.read().iter().map(|p| (p.id.clone(), p.title.clone())).collect();
	let labels: HashMap<PanelId, Element> = panels.read().iter().filter_map(|p| Some((p.id.clone(), p.label.clone()?))).collect();
	let frames = state.read().frames(&titles);
	let content = state.read().content_slots();
	let ghost = state.read().ghost(&titles);
	let inspect = state.read().inspect();
	let help = state.read().help(&titles);
	let hint = state.read().hint();
	let notice = state.read().notice();
	let dragging = state.read().dragging();
	let carrying = state.read().carrying();
	let resizing = state.read().resizing();
	let root_style = state.read().root_style();
	let panel_list = panels.read().clone();

	let render = |frame: &Frame| -> Element {
		let gid = frame.gid;
		let class = [
			Some("tl-tile"),
			frame.shadow.then_some("tl-shadow"),
			frame.collapsed.then_some("tl-collapsed"),
			frame.floating.then_some("tl-float"),
			frame.locked.then_some("tl-locked"),
		]
		.into_iter()
		.flatten()
		.collect::<Vec<_>>()
		.join(" ");
		rsx! {
			div { key: "{gid.0}", class, style: "{frame.style}",
				if !frame.shadow {
					div { class: "tl-group",
						div {
							class: if frame.tab_drop { "tl-header tl-tab-drop" } else { "tl-header" },
							"data-dvg": "{gid.0}",
							onpointerdown: move |e: PointerEvent| {
								if e.trigger_button() != Some(MouseButton::Primary) {
									return;
								}
								e.stop_propagation();
								let c = e.client_coordinates();
								let g = e.element_coordinates();
								state.write().press_tile(gid, (c.x, c.y), (g.x, g.y));
							},
							for tab in frame.tabs.iter() {
								div {
									key: "{tab.id.0}",
									"data-panel": "{tab.id.0}",
									class: if tab.active { "tl-tab tl-active" } else { "tl-tab" },
									onpointerdown: {
										let id = tab.id.clone();
										move |e: PointerEvent| {
											if e.trigger_button() != Some(MouseButton::Primary) {
												return;
											}
											e.stop_propagation();
											if on_control(&e) {
												return;
											}
											let c = e.client_coordinates();
											let g = e.element_coordinates();
											state.write().press_tab(id.clone(), gid, (c.x, c.y), (g.x, g.y));
										}
									},
									ondragenter: {
										let id = tab.id.clone();
										move |_| state.write().drag_enter_tab(gid, &id)
									},
									match labels.get(&tab.id) {
										Some(label) => label.clone(),
										None => rsx! { "{tab.title}" },
									}
									if !tab.locked {
										button {
											class: "tl-tab-close",
											title: "Close tab",
											onclick: {
												let id = tab.id.clone();
												move |_| state.write().close(&id)
											},
											"×"
										}
									}
								}
							}
							div { class: "tl-actions", onpointerdown: |e: PointerEvent| e.stop_propagation(),
								if let Some(actions) = actions {
									{
										actions
											.call(Tile {
												group: gid,
												active: frame.tabs.iter().find(|t| t.active).expect("a tile shows one active tab").id.clone(),
												maximized: frame.maximized,
												collapsed: frame.collapsed,
												floating: frame.floating,
												api,
											})
									}
								}
								button { class: "tl-action", title: "Keybinds", onclick: move |_| state.write().open_help(gid), "?" }
								if let Some(on_add) = on_add {
									button { class: "tl-action", title: "Add a tab", onclick: move |_| on_add.call(gid), "+" }
								}
								if !frame.locked {
									button {
										class: "tl-action",
										title: if frame.floating { "Dock" } else { "Float" },
										onclick: move |_| state.write().toggle_float(gid),
										if frame.floating { "⊟" } else { "⧉" }
									}
									button { class: "tl-action", title: "Close", onclick: move |_| state.write().close_tile(gid), "✕" }
								}
							}
						}
						div { class: "tl-content-slot" }
					}
					for (grip , style) in frame.grips.iter().cloned() {
						div {
							class: grip.class(),
							style,
							onpointerdown: move |e: PointerEvent| {
								if e.trigger_button() != Some(MouseButton::Primary) {
									return;
								}
								e.stop_propagation();
								// Capture the pointer: moves keep streaming past the window bottom while the button is
								// held, so the tile stretches below the fold (the root's `overflow-y: auto` then scrolls).
								capture_pointer(&e);
								let c = e.client_coordinates();
								state.write().resize_start(gid, grip, c.x, c.y);
							},
							onpointermove: move |e: PointerEvent| {
								let c = e.client_coordinates();
								state.write().resize_move((c.x, c.y));
							},
							onpointerup: move |_| state.write().resize_end(),
							onpointercancel: move |_| state.write().resize_end(),
						}
					}
				}
			}
		}
	};

	rsx! {
		style { dangerous_inner_html: CSS }
		div {
			class: if scroll_x { "tl-packed tl-scroll-x" } else { "tl-packed" },
			style: root_style,
			onmounted: move |e| {
				#[cfg(target_arch = "wasm32")]
				{
					use dioxus::web::WebEventExt;
					use wasm_bindgen::JsCast;
					let el = e.data().as_web_event();
					el.add_event_listener_with_callback_and_bool("pointerdown", press.3.as_ref().unchecked_ref(), true)
						.expect("add pointerdown listener");
					state.write().set_root(el.clone());
					measure(&el, state, api, on_band);
					root.set(Some(el));
				}
				#[cfg(not(target_arch = "wasm32"))]
				let _ = (e, api, on_band);
			},
			onscroll: move |_| {
				#[cfg(target_arch = "wasm32")]
				{
					let root = root.peek();
					let root = root.as_ref().expect("a scroll comes from the mounted root");
					state.write().set_scroll((root.scroll_left() as f64, root.scroll_top() as f64));
				}
			},
			ondragover: move |e: DragEvent| {
				if state.peek().carrying() {
					e.prevent_default(); // takes the drop
					let c = e.client_coordinates();
					state.write().drag_move((c.x, c.y));
				}
			},
			ondrop: move |e: DragEvent| {
				if state.peek().carrying() {
					e.prevent_default();
					state.write().drag_release();
				}
			},
			ondragleave: move |e: DragEvent| {
				#[cfg(target_arch = "wasm32")]
				if state.peek().carrying() {
					use dioxus::web::WebEventExt;
					use wasm_bindgen::JsCast;
					let into = e.as_web_event().related_target().and_then(|t| t.dyn_into::<web_sys::Node>().ok());
					let root = root.peek();
					let root = root.as_ref().expect("a drag reaches the mounted root");
					if !into.is_some_and(|n| root.contains(Some(&n))) {
						state.write().carry_out();
					}
				}
				#[cfg(not(target_arch = "wasm32"))]
				let _ = e;
			},
			onresize: move |_| {
				#[cfg(target_arch = "wasm32")]
				if let Some(el) = root.peek().as_ref() {
					measure(el, state, api, on_band); // before mount, `onmounted` takes the first measure
				}
			},

			for frame in frames.iter().filter(|f| !f.floating) {
				{render(frame)}
			}

			div { class: "tl-overlay",
				for panel in panel_list.iter() {
					div {
						key: "{panel.id.0}",
						class: "tl-render-overlay",
						style: content.get(&panel.id).map(|s| s.style.clone()).unwrap_or_else(|| "display:none;".to_string()),
						// Clicking a pane's body focuses it too (not just its header/tab), so pane keybinds act
						// on whichever pane you last interacted with.
						onpointerdown: {
							let group = content.get(&panel.id).map(|s| s.group);
							move |_| {
								if let Some(g) = group {
									state.write().focus(g);
								}
							}
						},
						{panel.content.clone()}
					}
				}
			}

			div { class: "tl-floats",
				for frame in frames.iter().filter(|f| f.floating) {
					{render(frame)}
				}
			}

			for (style, label) in inspect.iter() {
				div { class: "tl-inspect", style: style.clone(), span { {label.clone()} } }
			}

			// Drag capture: a fixed surface (the Dioxus-web stand-in for `setPointerCapture`) that owns
			// pointermove/up for the whole gesture, so moves over child tiles don't leak.
			// A carry's pointer is the browser's: the surface would only take its drop away from the tiles.
			if dragging && !carrying {
				div {
					style: "position:fixed; inset:0; z-index:1000; cursor:grabbing;",
					onpointermove: move |e: PointerEvent| {
						let c = e.client_coordinates();
						state.write().drag_move((c.x, c.y));
					},
					onpointerup: move |_| state.write().drag_release(),
					onpointercancel: move |_| state.write().drag_cancel(),
				}
			}
			// Cursor-only scrim while resizing: the captured pointer routes moves to the handle; this just
			// keeps the resize cursor steady over whatever the pointer crosses.
			if let Some(grip) = resizing {
				div { style: "position:fixed; inset:0; z-index:1000; cursor:{grip.cursor()};" }
			}
			if let Some(g) = ghost {
				div {
					class: if g.collapsed { "tl-ghost tl-collapsed" } else { "tl-ghost" },
					style: "left:{g.left}px; top:{g.top}px; width:{g.w}px; height:{g.h}px;",
					div { class: "tl-header",
						for t in g.tabs.iter() {
							div { key: "{t.id.0}", class: if t.active { "tl-tab tl-active" } else { "tl-tab" }, "{t.title}" }
						}
					}
				}
			}
			if let Some(n) = notice {
				div { key: "{n.seq}", class: "tl-notice", "{n.text}" }
			}
			// Which-key popup for a pending chord. `.tl-hint` delays its own reveal in CSS.
			if let Some(h) = hint {
				div { class: "tl-hint",
					div { class: "tl-hint-prefix", "{h.prefix}" }
					for r in h.rows.iter() {
						div { class: "tl-hint-row",
							span { class: "tl-help-key", "{r.chord}" }
							span { "{r.label}" }
							if r.scope {
								span { " …" }
							}
						}
					}
				}
			}
			if let Some(help) = help {
				div { class: "tl-help-scrim", onclick: move |_| state.write().close_help(),
					div { class: "tl-help",
						div { class: "tl-help-title", "Keybinds" }
						if let Some(p) = help.pane {
							div { class: "tl-help-pane", {p} }
						}
						for tilers_core::keys::HelpRow { sequence: chord, label, .. } in help.rows {
							div { class: "tl-help-row",
								span { class: "tl-help-label", {label} }
								span { class: "tl-help-key", {chord} }
							}
						}
						div { class: "tl-help-foot", "click anywhere to close" }
					}
				}
			}
		}
	}
}
/// What [`PackedArea`]'s `actions` renders for: one tile or float, and the handle to act on it.
#[derive(Clone, PartialEq)]
pub struct Tile {
	pub group: GroupId,
	/// The panel its strip shows.
	pub active: PanelId,
	pub maximized: bool,
	/// Showing as a spine.
	pub collapsed: bool,
	/// Above the grid.
	pub floating: bool,
	pub api: PackedApi,
}

/// A press on a host's control in a tab's label is the control's: it never starts a tab drag, whose
/// capture surface would take the click.
fn on_control(e: &PointerEvent) -> bool {
	#[cfg(target_arch = "wasm32")]
	{
		use dioxus::web::WebEventExt;
		use wasm_bindgen::JsCast;
		e.data()
			.as_web_event()
			.target()
			.and_then(|t| t.dyn_into::<web_sys::Element>().ok())
			.expect("a pointerdown targets an element")
			.closest(".tl-tab :is(button, a[href], input, select, textarea)")
			.expect("a constant, valid selector")
			.is_some()
	}
	#[cfg(not(target_arch = "wasm32"))]
	{
		let _ = e;
		false
	}
}

fn declared(panels: &[DockPanel]) -> HashMap<PanelId, Declared> {
	panels.iter().map(|p| (p.id.clone(), Declared { min: p.min, locked: p.locked })).collect()
}

/// Read the root's `getBoundingClientRect` origin + content box (`clientWidth/Height`, which
/// exclude the scrollbar gutter the tiles must not spill into) and hand them to the core measure.
#[cfg(target_arch = "wasm32")]
fn measure(el: &web_sys::Element, mut state: Signal<PackedState>, api: PackedApi, on_band: Option<Callback<PackedApi>>) {
	let rect = el.get_bounding_client_rect();
	state
		.write()
		.set_measure((rect.x(), rect.y()), (el.client_width() as f64, el.client_height() as f64), tilers_core::framed(el));
	// The latch resolves the band's cache itself; `on_band` only fills in what it left behind — and
	// only then lists every panel a restored layout hosts, so its floors land after.
	if state.write().take_band().is_some() {
		if let Some(cb) = on_band {
			cb.call(api);
		}
		let declared = declared(&api.panels.peek());
		state.write().set_panels(declared);
	}
}

/// `setPointerCapture` on the resize grip so its pointermove keeps firing past the viewport edge.
#[cfg(target_arch = "wasm32")]
fn capture_pointer(e: &PointerEvent) {
	use dioxus::web::WebEventExt;
	use wasm_bindgen::JsCast;
	let ev = e.data().as_web_event();
	ev.target()
		.and_then(|t| t.dyn_into::<web_sys::Element>().ok())
		.expect("pointerdown target is the handle element")
		.set_pointer_capture(ev.pointer_id())
		.expect("capture on a live element with an active pointer");
}
#[cfg(not(target_arch = "wasm32"))]
fn capture_pointer(_: &PointerEvent) {}
