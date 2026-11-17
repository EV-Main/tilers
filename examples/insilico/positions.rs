use dioxus::prelude::*;

use crate::market::{DOWN, Market, SYM, UP};

const LEV: f64 = 10.0;
const MMR: f64 = 0.005; // maintenance margin rate

#[component]
pub fn Positions(market: Signal<Market>) -> Element {
	let m = market.read();
	if m.pos_qty.abs() < 1e-9 {
		return rsx! {
			div { style: "display:flex; flex-direction:column; gap:6px; align-items:center; justify-content:center; height:100%; box-sizing:border-box; color:#5a6b66; font:12px ui-monospace, monospace;",
				div { style: "font-size:22px; line-height:1; opacity:0.5;", "—" }
				div { "No open positions" }
				div { style: "font-size:10px; opacity:0.7;", "{SYM}" }
			}
		};
	}

	let long = m.pos_qty > 0.0;
	let qty_abs = m.pos_qty.abs();
	let entry = m.pos_avg;
	let mark = m.px;
	let notional = qty_abs * mark;
	let margin = notional / LEV;
	let pnl = m.unrealized();
	let roe = if margin == 0.0 { 0.0 } else { pnl / margin * 100.0 };
	// liq: price at which loss eats the maintenance buffer. long below entry, short above.
	let liq = if long { entry * (1.0 - 1.0 / LEV + MMR) } else { entry * (1.0 + 1.0 / LEV - MMR) };

	let side = if long { "LONG" } else { "SHORT" };
	let side_col = if long { UP } else { DOWN };
	let pnl_col = if pnl >= 0.0 { UP } else { DOWN };

	// distance bar: 0 at liq, 100 at entry; clamp mark's position between.
	let span = (entry - liq).abs().max(1e-9);
	let dist = ((mark - liq).abs() / span * 100.0).clamp(0.0, 100.0);
	let bar_col = if dist < 25.0 {
		DOWN
	} else if dist < 50.0 {
		"#e8c06a"
	} else {
		UP
	};

	let cell = "padding:4px 6px; text-align:right; white-space:nowrap;";
	let head = "padding:3px 6px; text-align:right; color:#5a6b66; font-weight:500; border-bottom:1px solid #1d2925;";
	let mut mk = market;
	rsx! {
		div { style: "padding:6px; font:11px ui-monospace, monospace; height:100%; box-sizing:border-box; display:flex; flex-direction:column; gap:8px;",
			table { style: "width:100%; border-collapse:collapse;",
				thead {
					tr {
						th { style: "{head} text-align:left;", "Symbol" }
						th { style: "{head}", "Side" }
						th { style: "{head}", "Size" }
						th { style: "{head}", "Entry" }
						th { style: "{head}", "Mark" }
						th { style: "{head} color:#e8c06a;", "Liq" }
						th { style: "{head}", "uPnL (ROE)" }
					}
				}
				tbody {
					tr { style: "border-bottom:1px solid #141d1a;",
						td { style: "{cell} text-align:left; color:#9fb; font-weight:600;", "{SYM}" }
						td { style: "{cell} color:{side_col}; font-weight:700;", "{side}" }
						td { style: "{cell} color:#cdd;", "{qty_abs:.2}" }
						td { style: "{cell} color:#aaa;", "{entry:.2}" }
						td { style: "{cell} color:#cdd;", "{mark:.2}" }
						td { style: "{cell} color:#e8c06a;", "{liq:.2}" }
						td { style: "{cell} color:{pnl_col}; font-weight:600;",
							"{pnl:+.2}"
							span { style: "display:block; font-size:10px; opacity:0.85;", "{roe:+.2}%" }
						}
					}
				}
			}

			div { style: "display:flex; justify-content:space-between; align-items:baseline; padding:0 2px;",
				div { style: "color:#5a6b66;",
					"margin "
					span { style: "color:#cdd;", "{margin:.2}" }
					span { style: "color:#63e9cd;", "  {LEV:.0}x" }
				}
				div { style: "color:#5a6b66;",
					"notional "
					span { style: "color:#cdd;", "{notional:.2}" }
				}
			}

			div { style: "padding:0 2px;",
				div { style: "display:flex; justify-content:space-between; color:#5a6b66; font-size:10px; margin-bottom:2px;",
					span { style: "color:#e8c06a;", "liq {liq:.2}" }
					span { "dist {dist:.0}%" }
					span { "entry {entry:.2}" }
				}
				div { style: "position:relative; height:5px; background:#141d1a; border-radius:3px; overflow:hidden;",
					div { style: "position:absolute; left:0; top:0; bottom:0; width:{dist}%; background:{bar_col}; border-radius:3px;" }
				}
			}

			div { style: "display:flex; gap:6px; margin-top:auto;",
				button {
					style: "flex:1; cursor:pointer; border:1px solid #2c3a36; background:transparent; color:#cdd;
						border-radius:4px; padding:6px 0; font:11px ui-monospace; font-weight:600;",
					onclick: move |_| { mk.write().submit(!long, None, qty_abs * 0.5); },
					"Reduce 50%"
				}
				button {
					style: "flex:1; cursor:pointer; border:0; border-radius:4px; padding:6px 0; font-weight:700; color:#400; background:{DOWN};",
					onclick: move |_| { mk.write().submit(!long, None, qty_abs); },
					"Market Close"
				}
			}
		}
	}
}
