use dioxus::prelude::*;

use crate::market::{DOWN, Market, SYM, UP};

const VW: f64 = 1000.0;
const VH: f64 = 440.0;
const PADR: f64 = 56.0; // price axis gutter
const PADT: f64 = 8.0;
const VOLH: f64 = 64.0; // volume strip height
const GAP: f64 = 10.0; // gap between candles and volume
const NCANDLE: usize = 22;

#[component]
pub fn Chart(market: Signal<Market>) -> Element {
	let m = market.read();
	let h = &m.hist;
	let n = h.len();
	let last = m.px;
	let prev = if n >= 2 { h[n - 2] } else { last };
	let first = *h.first().unwrap_or(&last);
	let day_open = if n > 0 { h[0] } else { last };
	let chg = last - first;
	let chg_pct = if first == 0.0 { 0.0 } else { chg / first * 100.0 };
	let up = last >= prev;
	let day_up = chg >= 0.0;
	let last_col = if up { UP } else { DOWN };
	let day_col = if day_up { UP } else { DOWN };

	// bucket hist into candles
	let bucket = n.max(1).div_ceil(NCANDLE);
	let mut candles: Vec<Candle> = Vec::new();
	for chunk in h.chunks(bucket.max(1)) {
		if chunk.is_empty() {
			continue;
		}
		let o = chunk[0];
		let c = *chunk.last().unwrap();
		let hi = chunk.iter().cloned().fold(f64::MIN, f64::max);
		let lo = chunk.iter().cloned().fold(f64::MAX, f64::min);
		candles.push(Candle { o, h: hi, l: lo, c, v: 0.0 });
	}
	let nc = candles.len().max(1);

	// fold tape volume into candles by chronological order (tape newest-first)
	let t_now = m.t;
	let t_span = (n as u64).max(1);
	let t_start = t_now.saturating_sub(t_span);
	let mut tot_vol = 0.0;
	for tr in m.tape.iter() {
		let rel = tr.t.saturating_sub(t_start);
		let idx = ((rel as f64 / t_span as f64) * nc as f64) as usize;
		let idx = idx.min(nc - 1);
		candles[idx].v += tr.qty;
		tot_vol += tr.qty;
	}
	let maxv = candles.iter().map(|c| c.v).fold(0.0_f64, f64::max).max(1.0);

	let lo = candles.iter().map(|c| c.l).fold(f64::MAX, f64::min).min(last);
	let hi = candles.iter().map(|c| c.h).fold(f64::MIN, f64::max).max(last);
	let pad = (hi - lo).max(0.01) * 0.08;
	let lo = lo - pad;
	let hi = hi + pad;
	let span = (hi - lo).max(0.01);

	let plot_w = VW - PADR;
	let plot_top = PADT;
	let plot_bot = VH - VOLH - GAP;
	let plot_h = plot_bot - plot_top;
	let y = |p: f64| plot_top + (hi - p) / span * plot_h;
	let slot = plot_w / nc as f64;
	let cw = (slot * 0.62).min(16.0);

	// grid + axis labels
	let grids: Vec<(f64, String)> = (0..=4)
		.map(|i| {
			let p = hi - span * i as f64 / 4.0;
			(y(p), format!("{p:.2}"))
		})
		.collect();

	let last_y = y(last);
	let last_tag_y = last_y.clamp(plot_top + 7.0, plot_bot - 1.0);

	// area line (close path) for subtle gradient under price
	let mut line = String::new();
	for (i, c) in candles.iter().enumerate() {
		let cx = i as f64 * slot + slot / 2.0;
		let py = y(c.c);
		if i == 0 {
			line.push_str(&format!("M{cx:.1},{py:.1}"));
		} else {
			line.push_str(&format!(" L{cx:.1},{py:.1}"));
		}
	}
	let area = format!("{line} L{plot_w:.1},{plot_bot:.1} L0,{plot_bot:.1} Z", plot_w = (nc as f64 - 0.5) * slot, plot_bot = plot_bot);

	// x time ticks
	let xticks: Vec<(f64, String)> = (0..=3)
		.map(|i| {
			let frac = i as f64 / 3.0;
			let cx = frac * (nc as f64 - 1.0) * slot + slot / 2.0;
			let tt = t_start + (frac * t_span as f64) as u64;
			(cx, format!("{tt}"))
		})
		.collect();

	rsx! {
		div { style: "padding:8px 10px; height:100%; box-sizing:border-box; display:flex; flex-direction:column;
			background:radial-gradient(120% 80% at 50% 0%,#10211d,#0b0f0e); overflow:hidden;",

			div { style: "display:flex; align-items:baseline; gap:14px; flex-wrap:wrap; font:ui-monospace, monospace;",
				span { style: "font:700 14px ui-monospace, monospace; color:#dff7f1; letter-spacing:.5px;", "{SYM}" }
				span { style: "font:700 22px ui-monospace, monospace; color:{last_col};", "{last:.2}" }
				{
					let arrow = if day_up { "▲" } else { "▼" };
					let sign = if day_up { "+" } else { "" };
					rsx! {
						span { style: "font:600 12px ui-monospace, monospace; color:{day_col};",
							"{arrow} {sign}{chg:.2} ({sign}{chg_pct:.2}%)"
						}
					}
				}
				div { style: "margin-left:auto; display:flex; gap:14px; font:11px ui-monospace, monospace; color:#5a6b66;",
					span { "O ", span { style: "color:#8aa39c;", "{day_open:.2}" } }
					span { "H ", span { style: "color:{UP};", "{hi:.2}" } }
					span { "L ", span { style: "color:{DOWN};", "{lo:.2}" } }
					span { "C ", span { style: "color:#8aa39c;", "{last:.2}" } }
					span { "Vol ", span { style: "color:#8aa39c;", "{tot_vol:.0}" } }
				}
			}

			div { style: "margin-top:6px; flex:1; min-height:0; border:1px solid #1d2c28; border-radius:4px;
				background:linear-gradient(180deg,#0f1b18,#0a0e0d); overflow:hidden;",
				svg {
					width: "100%",
					height: "100%",
					view_box: "0 0 {VW} {VH}",
					preserve_aspect_ratio: "none",

					defs {
						linearGradient { id: "areaFill", x1: "0", y1: "0", x2: "0", y2: "1",
							stop { offset: "0%", stop_color: last_col, stop_opacity: "0.18" }
							stop { offset: "100%", stop_color: last_col, stop_opacity: "0" }
						}
					}

					for (gy , lbl) in grids.iter() {
						line { x1: "0", y1: "{gy}", x2: "{plot_w}", y2: "{gy}", stroke: "#16241f", stroke_width: "1" }
						text { x: "{plot_w + 6.0}", y: "{gy + 3.0}", fill: "#5a6b66", font_size: "11", font_family: "ui-monospace, monospace", "{lbl}" }
					}

					line { x1: "{plot_w}", y1: "{plot_top}", x2: "{plot_w}", y2: "{VH}", stroke: "#1d2c28", stroke_width: "1" }

					path { d: "{area}", fill: "url(#areaFill)", stroke: "none" }
					path { d: "{line}", fill: "none", stroke: last_col, stroke_width: "1", stroke_opacity: "0.45" }

					for c in candles.iter().enumerate() {
						{
							let (i, cd) = c;
							let cx = i as f64 * slot + slot / 2.0;
							let bull = cd.c >= cd.o;
							let col = if bull { UP } else { DOWN };
							let yo = y(cd.o);
							let yc = y(cd.c);
							let top = yo.min(yc);
							let bh = (yo - yc).abs().max(1.0);
							let bx = cx - cw / 2.0;
							let yh = y(cd.h);
							let yl = y(cd.l);
							let vh = cd.v / maxv * (VOLH - 6.0);
							let vy = VH - vh;
							rsx! {
								line { x1: "{cx}", y1: "{yh}", x2: "{cx}", y2: "{yl}", stroke: col, stroke_width: "1.2" }
								rect { x: "{bx}", y: "{top}", width: "{cw}", height: "{bh}", fill: col, rx: "0.5" }
								rect { x: "{bx}", y: "{vy}", width: "{cw}", height: "{vh}", fill: col, fill_opacity: "0.32" }
							}
						}
					}

					for (cx , tt) in xticks.iter() {
						text { x: "{cx}", y: "{VH - 2.0}", fill: "#3f4f4a", font_size: "10", font_family: "ui-monospace, monospace", text_anchor: "middle", "{tt}" }
					}

					line { x1: "0", y1: "{last_y}", x2: "{plot_w}", y2: "{last_y}", stroke: last_col, stroke_width: "1", stroke_dasharray: "5 4", stroke_opacity: "0.8" }
					rect { x: "{plot_w}", y: "{last_tag_y - 8.0}", width: "{PADR}", height: "16", fill: last_col, rx: "2" }
					{
						let txt = format!("{last:.2}");
						rsx! {
							text { x: "{plot_w + PADR / 2.0}", y: "{last_tag_y + 4.0}", fill: "#08110e", font_size: "11", font_weight: "700", font_family: "ui-monospace, monospace", text_anchor: "middle", "{txt}" }
						}
					}
				}
			}
		}
	}
}
struct Candle {
	o: f64,
	h: f64,
	l: f64,
	c: f64,
	v: f64,
}
