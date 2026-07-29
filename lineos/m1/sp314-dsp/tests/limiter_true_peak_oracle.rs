//! F-048 — BrickwallLimiter enforces SAMPLE peak, not TRUE peak.
//!
//! MECHANISM (core.rs, process()):
//!     let current_peak = self.true_peak.process(l, r);   // correct, but this
//!                                                        // sample exits the
//!                                                        // delay line 240
//!                                                        // samples from now
//!     let delayed_peak = fmaxf(delay_l.max_abs(),        // <-- SAMPLE peak
//!                              delay_r.max_abs());       //     of what exits NOW
//!     let sidechain_peak = fmaxf(current_peak, delayed_peak);
//!
//! The true-peak estimate is computed for the incoming sample but never
//! travels with it through the delay line. By the time a sample is scaled,
//! only its raw magnitude survives. Material whose intersample peaks exceed
//! its sample peaks — anything with energy near Nyquist/4 — passes unlimited.
//!
//! #[ignore]d as executable documentation of a known defect. When the delay
//! line carries true-peak estimates instead of raw magnitudes, remove the
//! ignore — it should pass. Expect every pinned output hash to change.

use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};
use sp314_dsp::limiter::true_peak::measure_true_peak_dbtp;

const SR: u32 = 48_000;
const CEILING_DB: f32 = -1.0;
const TOL: f32 = 0.01;

fn limited(mut l: Vec<f32>, mut r: Vec<f32>) -> f32 {
    let cfg = LimiterConfig {
        ceiling_db: CEILING_DB,
        ..Default::default()
    };
    let mut lim = BrickwallLimiter::new(cfg, SR);
    let la = lim.lookahead_samples();
    lim.process_block(&mut l, &mut r);
    // Skip the lookahead priming region — ring-buffer zeros, not signal.
    measure_true_peak_dbtp(&l[la..], &r[la..])
}

fn sine(freq: f32, phase: f32, amp: f32) -> Vec<f32> {
    (0..SR as usize)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32 + phase).sin())
        .collect()
}

#[test]
#[ignore = "F-048: limiter is sample-peak; remove when the delay line carries true-peak"]
fn limiter_holds_true_peak_ceiling() {
    // Each case: (name, signal). Every one must land at or below the ceiling.
    let cases: Vec<(&str, Vec<f32>)> = vec![
        // sample peak == true peak — these already pass today
        ("sine 997Hz 0dBFS", sine(997.0, 0.0, 1.0)),
        ("sine 19kHz phased", sine(19_000.0, 0.785, 1.0)),
        // sample peak << true peak — these are the failures
        // 12 kHz at 48 k is 4 samples/cycle; a 45 deg phase puts every sample
        // at 0.707 (-3.01 dB) while the crest between them reaches 0 dB.
        ("sine 12kHz phased", sine(12_000.0, 0.785, 1.0)),
        ("sine 12kHz +6dB", sine(12_000.0, 0.785, 2.0)),
    ];

    let mut failures = Vec::new();
    for (name, s) in cases {
        let tp = limited(s.clone(), s);
        if tp > CEILING_DB + TOL {
            failures.push(format!(
                "{name}: {tp:+.4} dBTP (over by {:+.4})",
                tp - CEILING_DB
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "true peak exceeded ceiling:\n  {}",
        failures.join("\n  ")
    );
}

/// Pins the defect itself, so it cannot be reintroduced silently after a fix.
/// Runs by default — it asserts what the limiter DOES today, not what it should.
#[test]
fn limiter_gain_tracks_sample_peak_not_true_peak_f048() {
    let s = sine(12_000.0, 0.785, 1.0);
    let tp_in = measure_true_peak_dbtp(&s, &s);
    let tp_out = limited(s.clone(), s);
    assert!(
        tp_in > CEILING_DB + 1.0,
        "fixture must exceed the ceiling to be meaningful: {tp_in:+.4}"
    );
    assert!(
        (tp_out - tp_in).abs() < TOL,
        "F-048 changed: a signal whose SAMPLE peak sits below the ceiling used to \
         pass through untouched (in {tp_in:+.4} -> out {tp_out:+.4}). If the limiter \
         now reduces it, the defect is fixed — delete this test and un-ignore \
         limiter_holds_true_peak_ceiling."
    );
}
