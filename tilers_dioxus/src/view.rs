//! The Dioxus view over [`PackedState`]: one root-scope `Signal<PackedState>` replacing the old
//! ~14 separate signals, three `rsx!` blocks (root / frame / content) rendered straight from the
//! core view-model getters, and event handlers that each read one DOM primitive then forward it to
//! a reducer method. No layout logic lives here — only the reactive cell, the templates, and the
//! DOM reads the core can't do itself.

use std::collections::HashMap;

use dioxus::{html::input_data::MouseButton, prelude::*};
use tilers_core::{Band, CSS, Config, Group, GroupId, MinSize, PackedState, PanelId};

use crate::panel::DockPanel;

/// Imperative handle over the packed layout, handed to the host via `on_band`: a thin wrapper
/// over the root-scope `Signal<PackedState>` so the host can script tiles, save/load, and read the
/// live band from *outside* [`PackedArea`]'s subtree. Every method is a `read`/`write` of the one
/// cell; the reducer does the work.
#[derive(Clone, Copy)]
pub struct PackedApi {
	state: Signal<PackedState>,
	panels: Signal<Vec<DockPanel>>,
}

impl PackedApi {
	/// `group`'s panels must already be in `panels`: their declared mins floor the tile.
	pub fn place(&mut self, group: Group, w: u32, h: u32) {
		let mins = mins_of(&self.panels.peek());
		let mut s = self.state.write();
		s.set_mins(mins);
		s.place(group, w, h);
	}

	pub fn add_tab(&mut self, group: GroupId, panel: PanelId) {
		let mins = mins_of(&self.panels.peek());
		let mut s = self.state.write();
		s.set_mins(mins);
		s.add_tab(group, panel);
	}

	pub fn close_active(&mut self, group: GroupId) {
		self.state.write().close_active(group);
	}

	pub fn resize(&mut self, idx: usize, new_w: u32, new_h: u32) {
		self.state.write().resize(idx, new_w, new_h);
	}

	pub fn mint_group_id(&mut self) -> GroupId {
		self.state.write().mint_group_id()
	}

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
		!self.state.read().grid().cells.is_empty()
	}

	/// Wipe the layout back to empty (a host re-seeding a fresh band).
	pub fn reset(&mut self) {
		self.state.write().reset();
	}

	/// Every panel id the live layout hosts, in cell/tab order — for a host rebuilding its panel
	/// list from a restored layout.
	pub fn tab_ids(&self) -> Vec<PanelId> {
		self.state.read().grid().cells.iter().flat_map(|c| c.group.tabs.iter().cloned()).collect()
	}

	/// Serialize the current layout.
	pub fn save(&self) -> String {
		self.state.read().save()
	}

	/// Replace the layout with a previously [`save`](Self::save)d one; errors on a corrupt or
	/// future-version payload. Every panel it hosts must already be in `panels`.
	pub fn load(&mut self, json: &str) -> Result<(), tilers_core::model::serial::LoadError> {
		let mins = mins_of(&self.panels.peek());
		let mut s = self.state.write();
		s.set_mins(mins);
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
/// - the host must provide a `Callback<GroupId>` context — the per-tile `+` button calls it to ask
///   the host to open a new tab.
#[component]
pub fn PackedArea(panels: Signal<Vec<DockPanel>>, on_band: Option<Callback<PackedApi>>, config: Option<Config>) -> Element {
	let cfg = config.unwrap_or_default();
	let title_h_rem = cfg.title_h_rem;
	// Root-scope, not this scope: `PackedApi` is driven from outside `PackedArea`'s subtree, so the
	// cell must outlive this component (this root component never unmounts).
	let mut state = use_hook(move || Signal::new_in_scope(PackedState::new(cfg), ScopeId::ROOT));
	let api = PackedApi { state, panels };
	let request_tab = use_context::<Callback<GroupId>>();

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
		std::rc::Rc::new((key, blur))
	});
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
	});

	// A panel's declared min is host intent the state re-derives floors from.
	use_effect(move || {
		let mins = mins_of(&panels.read());
		state.write().set_mins(mins);
	});

	// Undo history: snapshot a settled layout after each structural edit. `wants_undo_snapshot` is
	// read (to subscribe) before any write, so the resulting push doesn't re-fire this effect.
	use_effect(move || {
		if state.read().wants_undo_snapshot() {
			state.write().capture_undo();
		}
	});

	let titles: HashMap<PanelId, String> = panels.read().iter().map(|p| (p.id.clone(), p.title.clone())).collect();
	let frames = state.read().frames(&titles);
	let content = state.read().content_slots();
	let ghost = state.read().ghost(&titles);
	let inspect = state.read().inspect();
	let help = state.read().help(&titles);
	let hint = state.read().hint();
	let notice = state.read().notice();
	let dragging = state.read().dragging();
	let resizing = state.read().resizing();
	let panel_list = panels.read().clone();

	rsx! {
		style { dangerous_inner_html: CSS }
		div {
			class: "tl-packed",
			// The stylesheet's one knob for the chrome band; the state resolves the same rem to px.
			style: "--tl-title-h: {title_h_rem}rem;",
			onmounted: move |e| {
				#[cfg(target_arch = "wasm32")]
				{
					use dioxus::web::WebEventExt;
					let el = e.data().as_web_event();
					state.write().set_root(el.clone());
					measure(&el, state, api, on_band);
					root.set(Some(el));
				}
				#[cfg(not(target_arch = "wasm32"))]
				let _ = (e, api, on_band);
			},
			onscroll: move |_| {
				#[cfg(target_arch = "wasm32")]
				state.write().set_scroll(root.peek().as_ref().expect("a scroll comes from the mounted root").scroll_top() as f64);
			},
			onresize: move |_| {
				#[cfg(target_arch = "wasm32")]
				if let Some(el) = root.peek().as_ref() {
					measure(el, state, api, on_band); // before mount, `onmounted` takes the first measure
				}
			},

			for frame in frames.iter() {
				div {
					key: "{frame.gid.0}",
					class: if frame.shadow { "tl-tile tl-shadow" } else { "tl-tile" },
					style: "{frame.style}",
					if !frame.shadow {
						div { class: "tl-group",
							div {
								class: if frame.tab_drop { "tl-header tl-tab-drop" } else { "tl-header" },
								"data-dvg": "{frame.gid.0}",
								onpointerdown: {
									let gid = frame.gid;
									move |e: PointerEvent| {
										if e.trigger_button() != Some(MouseButton::Primary) {
											return;
										}
										e.stop_propagation();
										let c = e.client_coordinates();
										let g = e.element_coordinates();
										state.write().press_tile(gid, (c.x, c.y), (g.x, g.y));
									}
								},
								for tab in frame.tabs.iter() {
									div {
										key: "{tab.id.0}",
										"data-panel": "{tab.id.0}",
										class: if tab.active { "tl-tab tl-active" } else { "tl-tab" },
										onpointerdown: {
											let (id, from) = (tab.id.clone(), frame.gid);
											move |e: PointerEvent| {
												if e.trigger_button() != Some(MouseButton::Primary) {
													return;
												}
												e.stop_propagation();
												let c = e.client_coordinates();
												let g = e.element_coordinates();
												state.write().press_tab(id.clone(), from, (c.x, c.y), (g.x, g.y));
											}
										},
										"{tab.title}"
									}
								}
								div { class: "tl-actions",
									button {
										class: "tl-action",
										title: "Keybinds",
										onpointerdown: |e: PointerEvent| e.stop_propagation(),
										onclick: {
											let gid = frame.gid;
											move |_| state.write().open_help(gid)
										},
										"?"
									}
									button {
										class: "tl-action",
										title: "Add window as a tab",
										onpointerdown: |e: PointerEvent| e.stop_propagation(),
										onclick: {
											let gid = frame.gid;
											move |_| request_tab.call(gid)
										},
										"+"
									}
									button {
										class: "tl-action",
										title: "Close",
										onpointerdown: |e: PointerEvent| e.stop_propagation(),
										onclick: {
											let gid = frame.gid;
											move |_| {
												state.write().close_active(gid);
											}
										},
										"✕"
									}
								}
							}
							div { class: "tl-content-slot" }
						}
						for (grip, style) in frame.grips.iter().cloned() {
							div {
								class: grip.class(),
								style,
								onpointerdown: {
									let idx = frame.idx;
									move |e: PointerEvent| {
										if e.trigger_button() != Some(MouseButton::Primary) {
											return;
										}
										e.stop_propagation();
										// Capture the pointer: moves keep streaming past the window bottom while the button is
										// held, so the tile stretches below the fold (the root's `overflow-y: auto` then scrolls).
										capture_pointer(&e);
										let c = e.client_coordinates();
										state.write().resize_start(idx, grip, c.x, c.y);
									}
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

			for (style, label) in inspect.iter() {
				div { class: "tl-inspect", style: style.clone(), span { {label.clone()} } }
			}

			// Drag capture: a fixed surface (the Dioxus-web stand-in for `setPointerCapture`) that owns
			// pointermove/up for the whole gesture, so moves over child tiles don't leak.
			if dragging {
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
				div { class: "tl-ghost", style: "left:{g.left}px; top:{g.top}px; width:{g.w}px; height:{g.h}px;",
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
						for (chord, label) in help.rows {
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
fn mins_of(panels: &[DockPanel]) -> HashMap<PanelId, MinSize> {
	panels.iter().map(|p| (p.id.clone(), p.min)).collect()
}

/// Read the root's `getBoundingClientRect` origin + content box (`clientWidth/Height`, which
/// exclude the scrollbar gutter the tiles must not spill into) and hand them to the core measure.
#[cfg(target_arch = "wasm32")]
fn measure(el: &web_sys::Element, mut state: Signal<PackedState>, api: PackedApi, on_band: Option<Callback<PackedApi>>) {
	let rect = el.get_bounding_client_rect();
	state.write().set_measure((rect.x(), rect.y()), (el.client_width() as f64, el.client_height() as f64), tilers_core::framed(el));
	// The latch resolves the band's cache itself; `on_band` only fills in what it left behind — and
	// only then lists every panel a restored layout hosts, so its floors land after.
	if state.write().take_band().is_some() {
		if let Some(cb) = on_band {
			cb.call(api);
		}
		let mins = mins_of(&api.panels.peek());
		state.write().set_mins(mins);
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
