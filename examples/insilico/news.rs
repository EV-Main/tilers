use dioxus::prelude::*;

use crate::market::Market;

const CASHTAGS: [&str; 6] = ["SOL", "BTC", "ETH", "USDT", "ETF", "CO-5"];
#[component]
pub fn News(market: Signal<Market>) -> Element {
	let m = market.read();
	let now = m.t;
	let total = m.news.len();
	rsx! {
		div { style: "display:flex; flex-direction:column; height:100%; box-sizing:border-box; font:11px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace; background:#0b1411; color:#bcd;",
			// header strip
			div { style: "flex:0 0 auto; display:flex; align-items:center; gap:8px; padding:6px 9px; border-bottom:1px solid #16221f; background:#0e1916;",
				span { style: "width:7px; height:7px; border-radius:50%; background:#3ddc84; box-shadow:0 0 6px #3ddc84; animation:nwspulse 1.6s ease-in-out infinite;" }
				span { style: "font-weight:700; letter-spacing:.08em; color:#dff; text-transform:uppercase;", "Feed" }
				span { style: "color:#5a6b66;", "·" }
				span { style: "color:#63e9cd; font-weight:600; letter-spacing:.06em;", "LIVE" }
				span { style: "margin-left:auto; color:#5a6b66;", "{total} items" }
			}
			// scrolling card list, newest first
			div { style: "flex:1 1 auto; overflow:auto; padding:5px 6px;",
				for (i , (src , body)) in m.news.iter().enumerate().rev() {
					{
						let (dot, _stag, stagc) = sentiment(body);
						let tag = topic(src, body);
						let hue = src_hue(src);
						let initial = src.trim_start_matches('@').chars().next().unwrap_or('?').to_ascii_uppercase();
						// derive a plausible "Xm ago": older items (lower index) are further back.
						let age = (total - 1 - i) as u64;
						let mins = age * 2 + (now % 3);
						let ago = if mins == 0 { "now".to_string() } else if mins < 60 { format!("{mins}m ago") } else { format!("{}h ago", mins.div_euclid(60)) };
						let clock = format!("{:02}:{:02}", (9 + now.div_euclid(60) % 14), now % 60);
						// split body so cashtags can render as pills
						let words: Vec<String> = body.split(' ').map(|w| w.to_string()).collect();
						rsx! {
							div { class: "nwscard", style: "display:flex; gap:8px; padding:8px 7px; border-bottom:1px solid #14201d; border-radius:5px; transition:background .12s;",
								// avatar
								div { style: "flex:0 0 auto; width:26px; height:26px; border-radius:50%; display:flex; align-items:center; justify-content:center; font-weight:700; color:#0b1411; background:{hue};",
									"{initial}"
								}
								div { style: "flex:1 1 auto; min-width:0;",
									// top line: source + tags + time
									div { style: "display:flex; align-items:center; gap:6px; flex-wrap:wrap;",
										span { style: "font-weight:700; color:{hue};", "{src}" }
										span { style: "font-size:9px; font-weight:700; letter-spacing:.05em; padding:1px 5px; border-radius:3px; color:#0b1411; background:{stagc};", "{tag}" }
										span { style: "width:6px; height:6px; border-radius:50%; background:{dot};" }
										span { style: "margin-left:auto; color:#5a6b66; font-size:10px;", "{ago} · {clock}" }
									}
									// headline / body with inline cashtag pills
									div { style: "margin-top:3px; color:#cdddd7; word-break:break-word;",
										for w in words.iter() {
											{
												let stripped: String = w.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
												let is_tag = CASHTAGS.contains(&stripped.to_uppercase().as_str()) && !stripped.is_empty();
												if is_tag {
													rsx! {
														span { style: "display:inline-block; padding:0 4px; margin:0 1px; border-radius:3px; color:#63e9cd; background:#10322c; font-weight:700;", "{w}" }
														" "
													}
												} else {
													rsx! { "{w} " }
												}
											}
										}
									}
								}
							}
						}
					}
				}
			}
			style { "@keyframes nwspulse{{0%,100%{{opacity:1}}50%{{opacity:.25}}}} .nwscard:hover{{background:#10201c}}" }
		}
	}
}
/// Stable per-source chip colour so the same handle always reads the same.
fn src_hue(src: &str) -> &'static str {
	match src.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32)) % 6 {
		0 => "#63e9cd",
		1 => "#e8c06a",
		2 => "#7aa2ff",
		3 => "#ff9d7a",
		4 => "#c08ae8",
		_ => "#3ddc84",
	}
}

/// Cheap keyword sentiment → (dot colour, tag label, tag colour).
fn sentiment(body: &str) -> (&'static str, &'static str, &'static str) {
	let l = body.to_lowercase();
	let bull = ["record", "inflow", "surge", "rally", "fantastic", "building", "beats", "up", "high"];
	let bear = ["reject", "dump", "sell-off", "selloff", "down", "low", "fear", "halt", "ban"];
	if bull.iter().any(|k| l.contains(k)) {
		("#3ddc84", "BULLISH", "#3ddc84")
	} else if bear.iter().any(|k| l.contains(k)) {
		("#ff5d6c", "BEARISH", "#ff5d6c")
	} else {
		("#5a6b66", "NEUTRAL", "#5a6b66")
	}
}

/// Topic tag from source/content.
fn topic(src: &str, body: &str) -> &'static str {
	let l = body.to_lowercase();
	if src == "Tape" {
		"TAPE"
	} else if src.starts_with('@') {
		"SOCIAL"
	} else if l.contains("etf") || l.contains("sol") || l.contains("btc") || l.contains("eth") {
		"CRYPTO"
	} else {
		"MACRO"
	}
}
