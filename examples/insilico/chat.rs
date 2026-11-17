use std::collections::BTreeSet;

use dioxus::prelude::*;

use crate::market::Market;

const ME: &str = "username_hr19j5";

#[component]
pub fn Chat(market: Signal<Market>) -> Element {
	let mut draft = use_signal(String::new);
	let m = market.read();

	let mut send = move || {
		let text = draft.read().trim().to_string();
		if text.is_empty() {
			return;
		}
		market.write().chat.push((ME, text));
		draft.set(String::new());
	};

	let online = 1 + m.chat.iter().map(|(w, _)| *w).filter(|w| *w != ME).collect::<BTreeSet<_>>().len();

	rsx! {
		div { style: "display:flex; height:100%; box-sizing:border-box; font:12px/1.45 system-ui; background:#0c1413; color:#cfe; overflow:hidden;",

			// channel rail
			div { style: "width:128px; flex:none; background:#0a100f; border-right:1px solid #16221f; padding:8px 6px; display:flex; flex-direction:column; gap:2px; overflow:hidden;",
				div { style: "color:#5a6b66; font-size:10px; letter-spacing:.08em; text-transform:uppercase; padding:2px 6px 6px;", "Channels" }
				for (name , active) in [("general", true), ("news", false), ("random", false)] {
					{
						let (bg, col) = if active { ("#16221f", "#cfe") } else { ("transparent", "#5a6b66") };
						rsx! {
							div { style: "padding:4px 8px; border-radius:5px; background:{bg}; color:{col}; cursor:default; white-space:nowrap; overflow:hidden; text-overflow:ellipsis;",
								span { style: "color:#3c4b47;", "# " }
								"{name}"
							}
						}
					}
				}
				div { style: "margin-top:auto; padding:6px; display:flex; align-items:center; gap:6px; border-top:1px solid #16221f;",
					div { style: "width:7px; height:7px; border-radius:50%; background:#3ddc84; box-shadow:0 0 5px #3ddc84;" }
					span { style: "color:#5a6b66; font-size:10px;", "{online} online" }
				}
			}

			// main column
			div { style: "flex:1; display:flex; flex-direction:column; min-width:0; overflow:hidden;",

				// header
				div { style: "flex:none; padding:8px 12px; border-bottom:1px solid #16221f; background:#0a100f;",
					div { style: "display:flex; align-items:baseline; gap:8px;",
						span { style: "color:#3c4b47; font-size:15px;", "#" }
						span { style: "color:#e6f3f0; font-weight:700;", "general" }
						span { style: "color:#5a6b66; font-size:10px;", "· {online} online" }
					}
					div { style: "color:#5a6b66; font-size:10px; margin-top:2px;", "desk chatter — scalps, fills & cope" }
				}

				// messages (newest at bottom; column-reverse keeps it pinned without JS)
				div { style: "flex:1; min-height:0; overflow-y:auto; display:flex; flex-direction:column-reverse; padding:8px 12px;",
					div {
						for (i , (who , msg)) in m.chat.iter().enumerate() {
							{
								let prev = i.checked_sub(1).map(|p| m.chat[p].0);
								let grouped = prev == Some(*who);
								let col = user_color(who);
								let ts = stamp(m.t.saturating_sub((m.chat.len() - i) as u64 * 3));
								let pad = if grouped { "1px 0" } else { "8px 0 1px" };
								rsx! {
									div { style: "display:flex; gap:9px; padding:{pad}; align-items:flex-start;",
										if grouped {
											div { style: "width:30px; flex:none;" }
										} else {
											div { style: "width:30px; height:30px; flex:none; border-radius:50%; background:{col}; color:#08110f; font-weight:700; display:flex; align-items:center; justify-content:center; font-size:13px;",
												{who.chars().next().unwrap_or('?').to_uppercase().to_string()}
											}
										}
										div { style: "min-width:0; flex:1;",
											if !grouped {
												div { style: "display:flex; align-items:baseline; gap:7px; margin-bottom:1px;",
													span { style: "color:{col}; font-weight:600;", "{who}" }
													span { style: "color:#3c4b47; font-size:10px;", "{ts}" }
												}
											}
											div { style: "color:#bcd2cc; word-break:break-word;", "{msg}" }
										}
									}
								}
							}
						}
					}
				}

				// composer
				div { style: "flex:none; padding:8px 12px; border-top:1px solid #16221f; background:#0a100f;",
					div { style: "display:flex; align-items:center; gap:9px; background:#11201d; border:1px solid #1d2f2b; border-radius:8px; padding:6px 8px;",
						div { style: "width:24px; height:24px; flex:none; border-radius:50%; background:{user_color(ME)}; color:#08110f; font-weight:700; display:flex; align-items:center; justify-content:center; font-size:11px;",
							"U"
						}
						input {
							style: "flex:1; min-width:0; background:transparent; border:none; outline:none; color:#e6f3f0; font:12px system-ui;",
							placeholder: "Message #general",
							value: "{draft}",
							oninput: move |e| draft.set(e.value()),
							onkeydown: move |e| if e.key() == Key::Enter {
								send();
							},
						}
						button {
							style: "flex:none; background:#63e9cd; color:#06120f; border:none; border-radius:6px; padding:4px 10px; font-weight:700; cursor:pointer;",
							onclick: move |_| send(),
							"Send"
						}
					}
				}
			}
		}
	}
}
/// stable per-user color so each name reads the same across the tape
fn user_color(name: &str) -> &'static str {
	const PAL: [&str; 8] = ["#63e9cd", "#e8c06a", "#7aa2ff", "#ff8fb3", "#9be36a", "#c792ea", "#5ad1e8", "#ffb86c"];
	let h = name.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
	PAL[(h % PAL.len() as u32) as usize]
}

/// `t` is a tick counter, not wall-clock — fold it into a believable HH:MM.
fn stamp(t: u64) -> String {
	let mins = t.div_euclid(4);
	format!("{:02}:{:02}", 9 + mins.div_euclid(60) % 13, mins % 60)
}
