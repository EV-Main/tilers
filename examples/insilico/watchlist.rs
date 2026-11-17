use dioxus::prelude::*;

use crate::market::{DOWN, Market, SYM, UP};

// (symbol, base/quote price multiple, decimals, phase offset into hist)
const ROWS: [(&str, f64, usize, usize); 6] = [
	("BTCUSDT", 430.0, 1, 0),
	("ETHUSDT", 16.8, 2, 7),
	(SYM, 1.0, 2, 0),
	("BNBUSDT", 4.1, 2, 13),
	("XRPUSDT", 0.0135, 4, 21),
	("DOGEUSDT", 0.00115, 5, 31),
];

const SPK_W: f64 = 64.0;
const SPK_H: f64 = 18.0;

#[component]
pub fn Watchlist(market: Signal<Market>) -> Element {
	let m = market.read();
	let hist = &m.hist;
	let n = hist.len();
	let base = *hist.first().unwrap_or(&m.px);

	rsx! {
		style { ".wl-row:hover{{background:#16241f !important;}}" }
		div { style: "font:11px ui-monospace, monospace; height:100%; box-sizing:border-box; display:flex; flex-direction:column;
			background:#0b0f0e; color:#cfe;",

			div { style: "display:flex; align-items:center; gap:6px; padding:6px 8px; border-bottom:1px solid #1d2c28;",
				span { style: "font:700 12px ui-monospace, monospace; color:#dff7f1; letter-spacing:.5px;", "Markets" }
				span { style: "color:#5a6b66;", "{ROWS.len()} pairs" }
				span { style: "margin-left:auto; color:#63e9cd; font-size:10px;", "● live" }
			}

			div { style: "display:flex; padding:3px 8px; color:#5a6b66; font-size:10px; letter-spacing:.4px;
				border-bottom:1px solid #14201c;",
				span { style: "flex:1;", "PAIR" }
				span { style: "width:{SPK_W}px; text-align:center;", "24H" }
				span { style: "width:78px; text-align:right;", "LAST" }
				span { style: "width:58px; text-align:right;", "CHG%" }
			}

			div { style: "flex:1; min-height:0; overflow:auto;",
				for (sym , mult , dec , phase) in ROWS.into_iter() {
					{
						// each symbol borrows the primary path with its own phase shift and gain so it
						// moves live yet reads as a distinct instrument.
						let gain = 1.0 + (phase as f64 % 5.0) * 0.18;
						let path: Vec<f64> = (0..n)
							.map(|i| {
								let h = hist[(i + phase) % n];
								(base + (h - base) * gain) * mult
							})
							.collect();
						let px = *path.last().unwrap_or(&(m.px * mult));
						let first = *path.first().unwrap_or(&px);
						let chg = if first == 0.0 { 0.0 } else { (px - first) / first * 100.0 };
						let up = chg >= 0.0;
						let col = if up { UP } else { DOWN };
						let arrow = if up { "▲" } else { "▼" };
						let sign = if up { "+" } else { "" };
						let primary = sym == SYM;

						let lo = path.iter().cloned().fold(f64::MAX, f64::min);
						let hi = path.iter().cloned().fold(f64::MIN, f64::max);
						let span = (hi - lo).max(1e-9);
						let step = SPK_W / (n.max(2) - 1) as f64;
						let mut d = String::new();
						for (i, p) in path.iter().enumerate() {
							let x = i as f64 * step;
							let y = SPK_H - 1.0 - (p - lo) / span * (SPK_H - 2.0);
							d.push_str(if i == 0 { "M" } else { "L" });
							d.push_str(&format!("{x:.1} {y:.1} "));
						}

						let pair = sym.strip_suffix("USDT").unwrap_or(sym);
						let row_bg = if primary { "background:#11201c;" } else { "" };
						let mark = if primary { "border-left:2px solid #63e9cd;" } else { "border-left:2px solid transparent;" };

						rsx! {
							div {
								class: "wl-row",
								style: "display:flex; align-items:center; padding:4px 8px 4px 6px; {row_bg} {mark}
									border-bottom:1px solid #0f1714; cursor:pointer; transition:background .1s;",
								div { style: "flex:1; display:flex; flex-direction:column; line-height:1.2;",
									span { style: "font-weight:700; color:#e6fbf4;", "{pair}" }
									span { style: "color:#5a6b66; font-size:9px;",
										"{pair}/USDT · perp"
									}
								}
								svg { width: "{SPK_W}", height: "{SPK_H}", view_box: "0 0 {SPK_W} {SPK_H}",
									path { d: "{d}", fill: "none", stroke: col, stroke_width: "1.2", stroke_opacity: "0.85" }
								}
								div { style: "width:78px; display:flex; flex-direction:column; align-items:flex-end; line-height:1.2;",
									span { style: "color:#e6fbf4;", "{px:.dec$}" }
									span { style: "color:#5a6b66; font-size:9px;", "{lo:.dec$}" }
								}
								span { style: "width:58px; text-align:right; color:{col}; font-weight:600;",
									"{arrow} {sign}{chg:.2}%"
								}
							}
						}
					}
				}
			}
		}
	}
}
