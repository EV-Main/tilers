#![feature(default_field_values)]
//! Deterministic model fuzzer for the tilers DnD tree reshapers (matklad FRNG +
//! TigerBeetle VOPR). One integration binary, per matklad's "delete cargo integration tests":
//! `fuzz` drives random seeds, auto-minimizes the first failure, and records its minimal
//! `(seed, size)` to `CORPUS.txt`; `regressions` replays every recorded case. Env-var replay:
//! `FUZZ_SEED=… FUZZ_SIZE=… cargo test --test integration -- --nocapture` verbose-replays one case.
//!
//! The FRNG, the minimizer, the corpus format and the scan/record/replay loop are
//! [`v_utils::fuzz`], shared with `trading_data`'s fuzz binary. What is local is [`sim`] and the
//! one-line target table below.

mod actions;
mod fill;
mod keys;
mod mins;
mod oracle;
mod sim;

use v_utils::fuzz::{Suite, Target};

const SUITE: Suite = Suite {
	targets: &[Target {
		name: "sim",
		// `keys.rs` too: the key alphabet is walked out of `Keymap::default()`.
		version: v_utils::fuzz::fnv(&[v_utils::fuzz::FRNG_SRC, include_str!("actions.rs"), include_str!("sim.rs"), include_str!("../../src/keys.rs")]),
		// `Failure::step` is flattened into the message rather than dropped: it is the one thing a
		// recorded line cannot recover from `(seed, size)` without re-running the case.
		run: |s, z, v| sim::run(s, z, v, &mut |_, _| {}).map_err(|f| format!("step {}: {}", f.step, f.what)),
	}],
	corpus: concat!(env!("CARGO_MANIFEST_DIR"), "/tests/integration/CORPUS.txt"),
};

#[test]
fn fuzz() {
	SUITE.fuzz();
}

#[test]
fn regressions() {
	SUITE.regressions();
}
