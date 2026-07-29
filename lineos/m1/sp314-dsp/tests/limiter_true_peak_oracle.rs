//! F-048 guard — the limiter must hold its configured TRUE-peak ceiling.
//!
//! It did not, until the delay line started carrying peak estimates alongside
//! samples. Before: the true-peak estimate was computed for each incoming
//! sample and then discarded, so the sidechain read raw magnitudes and material
//! whose intersample peaks exceeded its sample peaks passed unlimited —
//! 12 kHz phased came in at +0.1088 dBTP and left at +0.1088 dBTP.
//!
//! Two parts to the fix, both in limiter/core.rs:
//!   - PeakRing carries the estimate belonging to the sample being scaled
//!   - TRUE_PEAK_HEADROOM_DB absorbs the 4x estimator's fixed underread
//!
//! If this test fails again, one of those two is broken.

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
