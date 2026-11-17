use dioxus::prelude::*;

use crate::market::{DOWN, Market, UP};

const TEAL: &str = "#63e9cd";
const MUTED: &str = "#8aa39c";
const AMBER: &str = "#e8c06a";

#[component]
pub fn Console(market: Signal<Market>) -> Element {
	let mut input = use_signal(String::new);
	let mut filter = use_signal(|| "ALL");

	let mut submit = move || {
		let cmd = input.read().trim().to_string();
		if cmd.is_empty() {
			return;
		}
		input.set(String::new());
		if cmd == "clear" {
			market.write().log.clear();
			return;
		}
		market.write().note(format!("$ {cmd}"));
	};

	let m = market.read();
	let f = *filter.read();

	let chip = |label: &'static str, cur: &str| -> String {
		let on = label == cur;
		format!(
			"padding:1px 8px; border-radius:9px; cursor:pointer; user-select:none; border:1px solid {}; color:{}; background:{};",
			if on { TEAL } else { "#2a3a36" },
			if on { "#0c1513" } else { MUTED },
			if on { TEAL } else { "transparent" },
		)
	};

	rsx! {
		div { style: "display:flex; flex-direction:column; height:100%; box-sizing:border-box; background:#0b1311; color:#cfe; font:11px ui-monospace, SFMono-Regular, Menlo, monospace; border:1px solid #1c2a27;",
			div { style: "display:flex; align-items:center; gap:8px; padding:5px 8px; border-bottom:1px solid #1c2a27; background:#0e1816;",
				span { style: "width:7px; height:7px; border-radius:50%; background:{UP}; box-shadow:0 0 6px {UP};" }
				span { style: "color:{TEAL}; font-weight:600;", "Bybit" }
				span { style: "color:{MUTED};", "Connected" }
				div { style: "flex:1;" }
				for c in ["ALL", "FILLS", "ORDERS"] {
					div {
						style: chip(c, f),
						onclick: move |_| filter.set(c),
						"{c}"
					}
				}
			}
			div { style: "flex:1; min-height:0; overflow:auto; padding:3px 0; display:flex; flex-direction:column;",
				for (i , (t , line)) in m.log.iter().enumerate() {
					{
						let (tag, color) = cat(line);
						let show = match f {
							"FILLS" => tag == "FILL",
							"ORDERS" => tag == "ORDER" || tag == "CANCEL" || tag == "REJECT",
							_ => true,
						};
						let bg = if i % 2 == 0 { "transparent" } else { "#0e1816" };
						if show {
							rsx! {
								div { style: "display:flex; gap:6px; align-items:baseline; padding:1px 8px; background:{bg};",
									span { style: "color:#54655f; flex:0 0 auto;", "{stamp(*t)}" }
									span { style: "flex:0 0 52px; text-align:center; border-radius:3px; padding:0 2px; font-size:9px; font-weight:700; color:#0c1513; background:{color};",
										"{tag}"
									}
									span { style: "color:{color}; white-space:pre-wrap; word-break:break-all;", "{line}" }
								}
							}
						} else {
							rsx! {}
						}
					}
				}
			}
			div { style: "display:flex; align-items:center; gap:6px; padding:5px 8px; border-top:1px solid #1c2a27; background:#0e1816;",
				span { style: "color:{TEAL}; font-weight:700;", ">" }
				input {
					style: "flex:1; background:transparent; border:none; outline:none; color:#cfe; font:inherit;",
					placeholder: "Type CLI command here",
					value: "{input}",
					oninput: move |e| input.set(e.value()),
					onkeydown: move |e| {
						if e.key() == Key::Enter {
							submit();
						}
					},
				}
			}
		}
	}
}
/// (tag, color) derived from the message prefix
fn cat(line: &str) -> (&'static str, &'static str) {
	if line.starts_with("REJECTED") {
		("REJECT", DOWN)
	} else if line.starts_with("FILL") {
		("FILL", UP)
	} else if line.starts_with("PLACED") {
		("ORDER", AMBER)
	} else if line.starts_with("CANCEL") {
		("CANCEL", AMBER)
	} else if line.starts_with("$ ") {
		("CMD", TEAL)
	} else {
		("INFO", MUTED)
	}
}

/// tick clock -> [hh:mm:ss]
fn stamp(t: u64) -> String {
	let s = t % 60;
	let m = t.div_euclid(60) % 60;
	let h = t.div_euclid(3600) % 24;
	format!("[{h:02}:{m:02}:{s:02}]")
}
