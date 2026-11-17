use dioxus::prelude::*;

use crate::market::{DOWN, Market, UP};

const TAPE_CSS: &str = r#"
@keyframes tp-flash { from { background-color: rgba(99,233,205,0.22); } to { background-color: transparent; } }
.tp-row.tp-new { animation: tp-flash 0.6s ease-out; box-shadow: inset 0 1px 0 rgba(99,233,205,0.35); }
.tp-row.tp-whale { box-shadow: inset 2px 0 0 currentColor; }
.tp-row:hover { background-color: rgba(255,255,255,0.03) !important; }
"#;
#[component]
pub fn Trades(market: Signal<Market>) -> Element {
	let m = market.read();
	let max_qty = m.tape.iter().map(|t| t.qty).fold(0.0_f64, f64::max).max(1e-9);
	let whale = max_qty * 0.7;
	let buy_vol: f64 = m.tape.iter().filter(|t| t.buy).map(|t| t.qty).sum();
	let tot_vol: f64 = m.tape.iter().map(|t| t.qty).sum::<f64>().max(1e-9);
	let buy_pct = (buy_vol / tot_vol * 100.0).round();
	let sell_pct = 100.0 - buy_pct;

	rsx! {
		style { {TAPE_CSS} }
		div { style: "display:flex; flex-direction:column; height:100%; box-sizing:border-box; font:11px ui-monospace, monospace; background:#0c1311; overflow:hidden;",
			div { style: "display:flex; color:#5a6b66; font-size:10px; letter-spacing:.04em; text-transform:uppercase; padding:5px 8px 4px; border-bottom:1px solid #1a2724; flex:0 0 auto;",
				span { style: "flex:1.3; text-align:left;", "Price" }
				span { style: "flex:1; text-align:right;", "Size" }
				span { style: "flex:1; text-align:right;", "Time" }
			}
			div { style: "flex:1 1 auto; overflow:hidden; display:flex; flex-direction:column;",
				for (i , tr) in m.tape.iter().take(28).enumerate() {
					{
						let col = if tr.buy { UP } else { DOWN };
						let frac = (tr.qty / max_qty).clamp(0.0, 1.0);
						let bar_w = frac * 100.0;
						// bar grows from the side the aggressor hit
						let bar = if tr.buy {
							format!("linear-gradient(90deg, rgba(61,220,132,0.16) {bar_w:.0}%, transparent {bar_w:.0}%)")
						} else {
							format!("linear-gradient(270deg, rgba(255,93,108,0.16) {bar_w:.0}%, transparent {bar_w:.0}%)")
						};
						let is_whale = tr.qty >= whale;
						let new_cls = if i == 0 { "tp-new" } else { "" };
						let whale_cls = if is_whale { "tp-whale" } else { "" };
						let weight = if is_whale { 700 } else { 500 };
						let arrow = if tr.buy { "\u{25B2}" } else { "\u{25BC}" };
						let secs = tr.t % 60;
						let mins = tr.t.div_euclid(60) % 60;
						let qty_col = if is_whale { col } else { "#9bb3ad" };
						rsx! {
							div {
								class: "tp-row {new_cls} {whale_cls}",
								style: "display:flex; align-items:center; padding:2px 8px; background:{bar}; border-bottom:1px solid #121d1a;",
								span { style: "flex:1.3; text-align:left; color:{col}; font-weight:{weight}; display:flex; align-items:center; gap:5px;",
									span { style: "font-size:8px; line-height:1;", "{arrow}" }
									span { "{tr.px:.2}" }
								}
								span { style: "flex:1; text-align:right; color:{qty_col}; font-weight:{weight}; font-variant-numeric:tabular-nums;", "{tr.qty:.2}" }
								span { style: "flex:1; text-align:right; color:#5a6b66; font-variant-numeric:tabular-nums;", "{mins:02}:{secs:02}" }
							}
						}
					}
				}
			}
			div { style: "flex:0 0 auto; padding:5px 8px 6px; border-top:1px solid #1a2724;",
				div { style: "display:flex; justify-content:space-between; font-size:9px; color:#5a6b66; margin-bottom:3px;",
					span { style: "color:{UP};", "BUY {buy_pct:.0}%" }
					span { "pressure" }
					span { style: "color:{DOWN};", "{sell_pct:.0}% SELL" }
				}
				div { style: "display:flex; height:4px; border-radius:2px; overflow:hidden; background:#1a2724;",
					div { style: "width:{buy_pct}%; background:{UP};" }
					div { style: "width:{sell_pct}%; background:{DOWN};" }
				}
			}
		}
	}
}
