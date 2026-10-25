//! The Leptos view over [`PackedState`]. One `RwSignal<PackedState, LocalStorage>` (thread-local:
//! the state holds `Rc`s, so it is `!Send`) drives everything. The tile skeleton is a coarse
//! reactive block — it holds no user content, so rebuilding it on every state change is harmless —
//! while the content overlay is a keyed `<For>` whose per-panel position is a fine-grained reactive
//! style, so a panel's node survives layout restructuring while still tracking its tile. Every
//! `on:*` reads one DOM primitive and forwards it to the identical core reducer method.

use std::{collections::HashMap, sync::Arc};

use leptos::prelude::*;
use tilers_core::{Band, CSS, Config, Group, GroupId, MinSize, PackedState, PanelId, model::serial::LoadError};

use crate::panel::DockPanel;

/// Imperative handle over the packed layout, handed to the host via `on_band`: a thin wrapper over
/// the thread-local `RwSignal<PackedState>` so the host can script tiles, save/load, and read the
/// live band from outside the component. Every method is a `with`/`update` of the one cell.
#[derive(Clone, Copy)]
pub struct PackedApi {
	state: RwSignal<PackedState, LocalStorage>,
	panels: RwSignal<Vec<DockPanel>>,
}

impl PackedApi {
	/// `group`'s panels must already be in `panels`: their declared mins floor the tile.
	pub fn place(&self, group: Group, w: u32, h: u32) {
		let mins = self.panels.with_untracked(|ps| mins_of(ps));
		self.state.update(|s| {
			s.set_mins(mins);
			s.place(group, w, h)
		});
	}

	pub fn add_tab(&self, group: GroupId, panel: PanelId) {
		let mins = self.panels.with_untracked(|ps| mins_of(ps));
		self.state.update(|s| {
			s.set_mins(mins);
			s.add_tab(group, panel)
		});
	}

	pub fn close_active(&self, group: GroupId) {
		self.state.update(|s| s.close_active(group));
	}

	pub fn resize(&self, idx: usize, new_w: u32, new_h: u32) {
		self.state.update(|s| s.resize(idx, new_w, new_h));
	}

	pub fn mint_group_id(&self) -> GroupId {
		self.state.try_update(|s| s.mint_group_id()).expect("state signal alive")
	}

	pub fn cols(&self) -> u32 {
		self.state.with(|s| s.cols())
	}

	/// Whole rows the container shows.
	pub fn rows(&self) -> u32 {
		self.state.with(|s| s.rows())
	}

	/// The band the layout is stored under.
	pub fn band(&self) -> Band {
		self.state.with(|s| s.band())
	}

	/// Whether `on_band`'s invocation follows a restore from the seed cache. Only meaningful inside
	/// `on_band` — a fresh band arrives with an empty grid, and anything the host places fills it.
	pub fn restored(&self) -> bool {
		self.state.with(|s| !s.grid().cells.is_empty())
	}

	pub fn reset(&self) {
		self.state.update(|s| s.reset());
	}

	/// Every panel id the live layout hosts, in cell/tab order — for a host rebuilding its panel list.
	pub fn tab_ids(&self) -> Vec<PanelId> {
		self.state.with(|s| s.grid().cells.iter().flat_map(|c| c.group.tabs.iter().cloned()).collect())
	}

	pub fn save(&self) -> String {
		self.state.with(|s| s.save())
	}

	/// Every panel the payload hosts must already be in `panels`.
	pub fn load(&self, json: &str) -> Result<(), LoadError> {
		let mins = self.panels.with_untracked(|ps| mins_of(ps));
		self.state
			.try_update(|s| {
				s.set_mins(mins);
				s.load(json)
			})
			.expect("state signal alive")
	}

	/// Say `text` in the dock's toast: a [`Config::on_publish`] host reporting how its publish went.
	pub fn notify(&self, text: String) {
		self.state.update(|s| s.notify(text));
	}
}

/// Root of the packed layout. Owns the thread-local `RwSignal<PackedState>`, installs the window
/// keydown/blur listeners, measures its own box, and stacks the tile skeleton over the content
/// overlay.
///
/// - `panels` is a signal so windows spawned at runtime appear in the overlay.
/// - `on_band`: invoked with the [`PackedApi`] once per [`Band`] entry — after the first measure and
///   again on every band crossing — *after* the framework has resolved that band's seed cache.
///   [`PackedApi::restored`] says whether it found one; if not, the grid is empty and the host is
///   expected to place its tiles (or load its own default) against the now-real step size.
/// - `request_tab`: the per-tile `+` button calls it to ask the host to open a new tab.
#[component]
pub fn PackedArea(
	panels: RwSignal<Vec<DockPanel>>,
	#[prop(optional)] config: Option<Config>,
	#[prop(optional)] on_band: Option<Arc<dyn Fn(PackedApi) + Send + Sync>>,
	#[prop(optional)] request_tab: Option<Arc<dyn Fn(GroupId) + Send + Sync>>,
) -> impl IntoView {
	let cfg = config.unwrap_or_default();
	let title_h_rem = cfg.title_h_rem;
	let state = RwSignal::new_local(PackedState::new(cfg));
	let api = PackedApi { state, panels };
	let root = NodeRef::<leptos::html::Div>::new();

	// Observing the root rather than the window: a host can resize the container on its own (a
	// sidebar docking, a split pane), and any resize may cross a band, so it runs the latch too.
	#[cfg(target_arch = "wasm32")]
	{
		use wasm_bindgen::{JsCast, closure::Closure};
		let on_resize = Closure::<dyn Fn()>::new({
			let on_band = on_band.clone();
			move || {
				if let Some(el) = root.get_untracked() {
					measure(el.into(), state, api, on_band.as_ref());
				}
			}
		});
		let observer = web_sys::ResizeObserver::new(on_resize.as_ref().unchecked_ref()).expect("ResizeObserver is baseline in every browser");
		root.on_load({
			let observer = observer.clone();
			let on_band = on_band.clone();
			move |el| {
				state.update(|s| s.set_root(el.clone().into()));
				observer.observe(&el);
				measure(el.into(), state, api, on_band.as_ref())
			}
		});
		let held = send_wrapper::SendWrapper::new((observer, on_resize));
		on_cleanup(move || held.take().0.disconnect());
	}
	#[cfg(not(target_arch = "wasm32"))]
	let _ = (api, &on_band); // nothing measures without a DOM

	// A panel's declared min is host intent the state re-derives floors from.
	Effect::new(move |_| {
		let mins = panels.with(|ps| mins_of(ps));
		state.update(|s| s.set_mins(mins));
	});

	// Undo history: snapshot a settled layout after each structural edit. The `wants_undo_snapshot`
	// read subscribes this effect; the guarded update doesn't re-fire it into a loop.
	Effect::new(move |_| {
		if state.with(|s| s.wants_undo_snapshot()) {
			state.update(|s| s.capture_undo());
		}
	});

	// App-level keybinds on `window`, in the capture phase: a descendant (Google Maps, Plotly, …) that
	// `stopPropagation`s a keydown would otherwise silence them. `window_event_listener` is bubble-only.
	#[cfg(target_arch = "wasm32")]
	{
		use wasm_bindgen::{JsCast, closure::Closure};
		let keydown = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |e: web_sys::KeyboardEvent| {
			// Every keystroke typed into any page input lands here; only a change may re-render.
			let out = state
				.try_maybe_update(|s| {
					let o = s.on_key((&e).into());
					(o.changed, o)
				})
				.expect("listener removed with the state's owner");
			if out.prevent_default {
				e.prevent_default();
			}
		});
		let blur = Closure::<dyn Fn()>::new(move || state.update(|s| s.cancel_chord()));
		let window = web_sys::window().expect("a browser window");
		window
			.add_event_listener_with_callback_and_bool("keydown", keydown.as_ref().unchecked_ref(), true)
			.expect("add keydown listener");
		window.add_event_listener_with_callback("blur", blur.as_ref().unchecked_ref()).expect("add blur listener");
		let held = send_wrapper::SendWrapper::new((keydown, blur));
		on_cleanup(move || {
			let (keydown, blur) = held.take();
			let window = web_sys::window().expect("a browser window");
			window
				.remove_event_listener_with_callback_and_bool("keydown", keydown.as_ref().unchecked_ref(), true)
				.expect("remove keydown listener");
			window.remove_event_listener_with_callback("blur", blur.as_ref().unchecked_ref()).expect("remove blur listener");
		});
	}

	// A frame loop for as long as a drag is live, so a pointer held at an edge keeps scrolling.
	#[cfg(target_arch = "wasm32")]
	{
		use std::{cell::RefCell, rc::Rc};

		use wasm_bindgen::{JsCast, closure::Closure};
		let tick: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::default();
		let pending: Rc<RefCell<Option<i32>>> = Rc::default();
		let request = {
			let tick = tick.clone();
			move || {
				let t = tick.borrow();
				let f = t.as_ref().expect("dropped only at cleanup, which cancels the pending frame first");
				web_sys::window()
					.expect("a browser window")
					.request_animation_frame(f.as_ref().unchecked_ref())
					.expect("requestAnimationFrame never throws")
			}
		};
		*tick.borrow_mut() = Some(Closure::new({
			let (pending, request) = (pending.clone(), request.clone());
			move || {
				// An idle frame changes nothing, so it must not re-render.
				let live = state.try_maybe_update(|s| (s.autoscroll(), s.dragging())).expect("loop cancelled with the state's owner");
				*pending.borrow_mut() = live.then(&request);
			}
		}));
		Effect::new({
			let pending = pending.clone();
			move |_| {
				if state.with(|s| s.dragging()) && pending.borrow().is_none() {
					*pending.borrow_mut() = Some(request());
				}
			}
		});
		let held = send_wrapper::SendWrapper::new((tick, pending));
		on_cleanup(move || {
			let (tick, pending) = held.take();
			if let Some(id) = pending.take() {
				web_sys::window()
					.expect("a browser window")
					.cancel_animation_frame(id)
					.expect("cancelAnimationFrame never throws");
			}
			tick.take(); // the closure holds `request`, which holds the closure
		});
	}

	view! {
		<style>{CSS}</style>
		// `--tl-title-h` is the stylesheet's one knob for the chrome band; the state resolves the same
		// rem to px for the content overlay's offset.
		<div
			class="tl-packed"
			node_ref=root
			style=format!("--tl-title-h: {title_h_rem}rem;")
			on:scroll=move |_| {
				let y = root.get_untracked().expect("a scroll comes from the mounted root").scroll_top()
					as f64;
				state.update(|s| s.set_scroll(y));
			}
		>
			// Tile skeleton — coarse re-render on any state change (no user content lives here).
			{move || {
				let titles = panel_titles(panels);
				state
					.with(|s| s.frames(&titles))
					.into_iter()
					.map(|f| render_frame(f, state, request_tab.clone()))
					.collect_view()
			}}

			// Content overlay — keyed so a panel's node survives; position is a fine-grained reactive style.
			<div class="tl-overlay">
				<For
					each=move || panels.get()
					key=|p| p.id.0.clone()
					children=move |panel| {
						let pid = panel.id.clone();
						let pid_click = panel.id.clone();
						let content = panel.content.clone();
						view! {
							<div
								class="tl-render-overlay"
								style=move || {
									state
										.with(|s| {
											s.content_slots()
												.get(&pid)
												.map(|sl| sl.style.clone())
												.unwrap_or_else(|| "display:none;".to_string())
										})
								}
								on:pointerdown=move |_| {
									if let Some(g) = state
										.with_untracked(|s| s.content_slots().get(&pid_click).map(|sl| sl.group))
									{
										state.update(|s| s.focus(g));
									}
								}
							>
								{content()}
							</div>
						}
					}
				/>
			</div>

			// Inspect popups.
			{move || {
				state
					.with(|s| s.inspect())
					.into_iter()
					.map(|(style, label)| {
						view! {
							<div class="tl-inspect" style=style>
								<span>{label}</span>
							</div>
						}
					})
					.collect_view()
			}}

			// Drag capture surface: owns pointermove/up for the whole gesture so moves over tiles don't leak.
			{move || {
				state
					.with(|s| s.dragging())
					.then(|| {
						view! {
							<div
								style="position:fixed; inset:0; z-index:1000; cursor:grabbing;"
								on:pointermove=move |e: web_sys::PointerEvent| {
									state.update(|s| { s.drag_move((e.client_x() as f64, e.client_y() as f64)) });
								}
								on:pointerup=move |_| state.update(|s| s.drag_release())
								on:pointercancel=move |_| state.update(|s| s.drag_cancel())
							></div>
						}
					})
			}}

			// Cursor-only scrim while resizing.
			{move || {
				state
					.with(|s| s.resizing())
					.map(|grip| {
						let style = format!("position:fixed; inset:0; z-index:1000; cursor:{};", grip.cursor());
						view! { <div style=style></div> }
					})
			}}

			// Floating drag ghost.
			{move || {
				let titles = panel_titles(panels);
				state
					.with(|s| s.ghost(&titles))
					.map(|g| {
						view! {
							<div
								class="tl-ghost"
								style=format!(
									"left:{}px; top:{}px; width:{}px; height:{}px;",
									g.left,
									g.top,
									g.w,
									g.h,
								)
							>
								<div class="tl-header">
									{g
										.tabs
										.into_iter()
										.map(|t| {
											view! {
												<div class=if t.active {
													"tl-tab tl-active"
												} else {
													"tl-tab"
												}>{t.title}</div>
											}
										})
										.collect_view()}
								</div>
							</div>
						}
					})
			}}

			// A memo, so only a new notice re-renders (and so remounts) it.
			{
				let notice = Memo::new(move |_| state.with(|s| s.notice()));
				move || notice.get().map(|n| view! { <div class="tl-notice">{n.text}</div> })
			}

			// Which-key popup for a pending chord. `.tl-hint` delays its own reveal in CSS.
			{move || {
				state
					.with(|s| s.hint())
					.map(|h| {
						view! {
							<div class="tl-hint">
								<div class="tl-hint-prefix">{h.prefix}</div>
								{h
									.rows
									.into_iter()
									.map(|r| {
										view! {
											<div class="tl-hint-row">
												<span class="tl-help-key">{r.chord}</span>
												<span>{r.label}{r.scope.then_some(" …")}</span>
											</div>
										}
									})
									.collect_view()}
							</div>
						}
					})
			}}

			// The full keymap.
			{move || {
				let titles = panel_titles(panels);
				state
					.with(|s| s.help(&titles))
					.map(|help| {
						view! {
							<div class="tl-help-scrim" on:click=move |_| state.update(|s| s.close_help())>
								<div class="tl-help">
									<div class="tl-help-title">"Keybinds"</div>
									{help.pane.map(|p| view! { <div class="tl-help-pane">{p}</div> })}
									{help
										.rows
										.into_iter()
										.map(|(chord, label)| {
											view! {
												<div class="tl-help-row">
													<span class="tl-help-label">{label}</span>
													<span class="tl-help-key">{chord}</span>
												</div>
											}
										})
										.collect_view()}
									<div class="tl-help-foot">"click anywhere to close"</div>
								</div>
							</div>
						}
					})
			}}
		</div>
	}
}

fn mins_of(panels: &[DockPanel]) -> HashMap<PanelId, MinSize> {
	panels.iter().map(|p| (p.id.clone(), p.min)).collect()
}

/// Panel id → tab label, read from the panels signal (tracks it).
fn panel_titles(panels: RwSignal<Vec<DockPanel>>) -> HashMap<PanelId, String> {
	panels.with(|ps| ps.iter().map(|p| (p.id.clone(), p.title.clone())).collect())
}

/// One tile of the skeleton. A shadow frame is a bare grey placeholder; a full frame renders its
/// header (move-handle + tabs + actions), a body spacer, and the resize grips — each wired to a core
/// reducer method.
fn render_frame(frame: tilers_core::Frame, state: RwSignal<PackedState, LocalStorage>, request_tab: Option<Arc<dyn Fn(GroupId) + Send + Sync>>) -> AnyView {
	if frame.shadow {
		return view! { <div class="tl-tile tl-shadow" style=frame.style></div> }.into_any();
	}
	let gid = frame.gid;
	let idx = frame.idx;
	let header_class = if frame.tab_drop { "tl-header tl-tab-drop" } else { "tl-header" };
	let tabs = frame
		.tabs
		.into_iter()
		.map(move |tab| {
			let id = tab.id.clone();
			view! {
				<div
					data-panel=tab.id.0.clone()
					class=if tab.active { "tl-tab tl-active" } else { "tl-tab" }
					on:pointerdown=move |e: web_sys::PointerEvent| {
						if e.button() != 0 {
							return;
						}
						e.stop_propagation();
						state
							.update(|s| {
								s.press_tab(
									id.clone(),
									gid,
									(e.client_x() as f64, e.client_y() as f64),
									(e.offset_x() as f64, e.offset_y() as f64),
								)
							});
					}
				>
					{tab.title}
				</div>
			}
		})
		.collect_view();
	view! {
		<div class="tl-tile" style=frame.style>
			<div class="tl-group">
				<div
					class=header_class
					data-dvg=gid.0
					on:pointerdown=move |e: web_sys::PointerEvent| {
						if e.button() != 0 {
							return;
						}
						e.stop_propagation();
						state
							.update(|s| {
								s.press_tile(
									gid,
									(e.client_x() as f64, e.client_y() as f64),
									(e.offset_x() as f64, e.offset_y() as f64),
								)
							});
					}
				>
					{tabs}
					<div class="tl-actions">
						<button
							class="tl-action"
							title="Keybinds"
							on:pointerdown=move |e: web_sys::PointerEvent| e.stop_propagation()
							on:click=move |_| state.update(|s| s.open_help(gid))
						>
							"?"
						</button>
						<button
							class="tl-action"
							title="Add window as a tab"
							on:pointerdown=move |e: web_sys::PointerEvent| e.stop_propagation()
							on:click=move |_| {
								if let Some(rt) = request_tab.clone() {
									rt(gid);
								}
							}
						>
							"+"
						</button>
						<button
							class="tl-action"
							title="Close"
							on:pointerdown=move |e: web_sys::PointerEvent| e.stop_propagation()
							on:click=move |_| state.update(|s| s.close_active(gid))
						>
							"✕"
						</button>
					</div>
				</div>
				<div class="tl-content-slot"></div>
			</div>
			{frame
				.grips
				.into_iter()
				.map(|(grip, style)| {
					view! {
						<div
							class=grip.class()
							style=style
							on:pointerdown=move |e: web_sys::PointerEvent| {
								if e.button() != 0 {
									return;
								}
								e.stop_propagation();
								capture_pointer(&e);
								state
									.update(|s| s.resize_start(idx, grip, e.client_x() as f64, e.client_y() as f64));
							}
							on:pointermove=move |e: web_sys::PointerEvent| {
								state.update(|s| s.resize_move((e.client_x() as f64, e.client_y() as f64)));
							}
							on:pointerup=move |_| state.update(|s| s.resize_end())
							on:pointercancel=move |_| state.update(|s| s.resize_end())
						></div>
					}
				})
				.collect::<Vec<_>>()}
		</div>
	}
	.into_any()
}

/// Hand the root's origin + content box to the core measure, then resolve the band: the latch has
/// already restored-or-reset the grid by the time `on_band` runs, so the host only fills in what it
/// left behind — and only then lists every panel a restored layout hosts, so its floors land after.
#[cfg(target_arch = "wasm32")]
fn measure(el: web_sys::Element, state: RwSignal<PackedState, LocalStorage>, api: PackedApi, on_band: Option<&Arc<dyn Fn(PackedApi) + Send + Sync>>) {
	let rect = el.get_bounding_client_rect();
	let framed = tilers_core::framed(&el);
	state.update(|s| s.set_measure((rect.x(), rect.y()), (el.client_width() as f64, el.client_height() as f64), framed));
	if state.try_update(|s| s.take_band()).expect("state signal alive").is_some() {
		if let Some(cb) = on_band {
			cb(api);
		}
		let mins = api.panels.with_untracked(|ps| mins_of(ps));
		state.update(|s| s.set_mins(mins));
	}
}

/// `setPointerCapture` on the resize grip so its pointermove keeps firing past the viewport edge.
fn capture_pointer(e: &web_sys::PointerEvent) {
	use wasm_bindgen::JsCast;
	if let Some(el) = e.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
		// A live element with an active pointer — a failed capture just means a non-captured resize.
		let _ = el.set_pointer_capture(e.pointer_id());
	}
}
