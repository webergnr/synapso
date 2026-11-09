//! Stopwatch for the hot paths. Silent unless `SYNAPSO_BENCH` is set, and then
//! it prints one aggregated line per path to stderr — a per-frame print costs
//! more than the frame it is measuring.
//!
//! ```text
//! SYNAPSO_BENCH=1 SYNAPSO_VAULT=/tmp/synapso-stress cargo run --release
//! ```
//!
//! a global Mutex and a Vec of samples. It is off in normal runs and
//! the paths it wraps take milliseconds; a lock-free histogram would be a
//! benchmark of its own.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// How many samples make a report — `SYNAPSO_BENCH=10` to report sooner.
fn window() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| {
        std::env::var("SYNAPSO_BENCH")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n| *n > 0)
            .unwrap_or(60)
    })
}

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SYNAPSO_BENCH").is_some())
}

/// One label and the measurements taken since its last report.
type Samples = Vec<(&'static str, Vec<f32>)>;

fn samples() -> &'static Mutex<Samples> {
    static SAMPLES: OnceLock<Mutex<Samples>> = OnceLock::new();
    SAMPLES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Records one measurement, and prints the window's shape once it is full.
pub fn record(label: &'static str, ms: f32) {
    if !on() {
        return;
    }
    let mut all = samples().lock().expect("bench");
    let slot = match all.iter_mut().find(|(name, _)| *name == label) {
        Some(slot) => slot,
        None => {
            all.push((label, Vec::new()));
            all.last_mut().expect("just pushed")
        }
    };
    slot.1.push(ms);
    if slot.1.len() < window() {
        return;
    }
    let mut sorted = std::mem::take(&mut slot.1);
    sorted.sort_by(f32::total_cmp);
    let at = |q: f32| sorted[((sorted.len() - 1) as f32 * q) as usize];
    eprintln!(
        "bench {label:<16} n={:<4} p50={:>7.2}ms p95={:>7.2}ms max={:>7.2}ms",
        sorted.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

/// Times a block: `let _t = bench::span("view");`
pub struct Span(&'static str, Instant);

impl Span {
    pub fn new(label: &'static str) -> Self {
        Self(label, Instant::now())
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        record(self.0, self.1.elapsed().as_secs_f32() * 1000.0);
    }
}

/// One-off measurements (startup): printed as they happen, not aggregated.
pub fn once(label: &str, at: Instant) {
    if on() {
        eprintln!(
            "bench {label:<16} {:.1}ms",
            at.elapsed().as_secs_f32() * 1000.0
        );
    }
}
