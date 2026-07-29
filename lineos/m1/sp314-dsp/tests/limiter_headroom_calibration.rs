//! F-048 calibration: how much does a per-sample gain envelope overshoot the
//! true-peak ceiling? Static gain is exact (measured: err 0.0000), so any
//! excess comes from the gain moving inside the detector's 18-tap window.

use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};
use sp314_dsp::limiter::true_peak::measure_true_peak_dbtp;

const SR: u32 = 48_000;
const CEILING: f32 = -1.0;
const CEILINGS: [f32; 4] = [-0.1, -1.0, -3.0, -6.0];

fn over_at(sig: &[f32], ceiling: f32) -> f32 {
    let cfg = LimiterConfig {
        ceiling_db: ceiling,
        ..Default::default()
    };
    let mut lim = BrickwallLimiter::new(cfg, SR);
    let la = lim.lookahead_samples();
    let (mut l, mut r) = (sig.to_vec(), sig.to_vec());
    lim.process_block(&mut l, &mut r);
    measure_true_peak_dbtp(&l[la..], &r[la..]) - ceiling
}

fn over(sig: &[f32], release_ms: f32) -> f32 {
    let cfg = LimiterConfig {
        ceiling_db: CEILING,
        release_ms,
        ..Default::default()
    };
    let mut lim = BrickwallLimiter::new(cfg, SR);
    let la = lim.lookahead_samples();
    let (mut l, mut r) = (sig.to_vec(), sig.to_vec());
    lim.process_block(&mut l, &mut r);
    measure_true_peak_dbtp(&l[la..], &r[la..]) - CEILING
}

fn sine(f: f32, ph: f32, a: f32) -> Vec<f32> {
    (0..SR as usize)
        .map(|i| a * (2.0 * std::f32::consts::PI * f * i as f32 / SR as f32 + ph).sin())
        .collect()
}

/// Deterministic broadband material: summed inharmonic partials plus a
/// repeating transient. Stands in for real programme content.
fn dense(amp: f32) -> Vec<f32> {
    let n = SR as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / SR as f32;
            let tone: f32 = [83.0, 197.0, 441.0, 1103.0, 2699.0, 7351.0, 11_003.0]
                .iter()
                .enumerate()
                .map(|(k, f)| {
                    (2.0 * std::f32::consts::PI * f * t + k as f32 * 0.7).sin() / (k + 1) as f32
                })
                .sum();
            let hit = if i % 6000 < 40 { 0.8 } else { 0.0 };
            amp * (tone * 0.35 + hit)
        })
        .collect()
}

#[test]
fn headroom_survey() {
    let cases: Vec<(&str, Vec<f32>)> = vec![
        ("sine 997Hz", sine(997.0, 0.0, 1.0)),
        ("sine 12k phased", sine(12_000.0, 0.785, 1.0)),
        ("sine 12k +6dB", sine(12_000.0, 0.785, 2.0)),
        ("sine 19k phased", sine(19_000.0, 0.785, 1.0)),
        ("dense 0dB", dense(1.0)),
        ("dense +6dB", dense(2.0)),
        ("dense +12dB", dense(4.0)),
    ];
    println!(
        "{:<18} {:>9} {:>9} {:>9} {:>9}",
        "signal", "r=20ms", "r=100ms", "r=400ms", "spread"
    );
    let mut worst = f32::MIN;
    for (name, s) in &cases {
        let a = over(s, 20.0);
        let b = over(s, 100.0);
        let c = over(s, 400.0);
        worst = worst.max(a).max(b).max(c);
        println!(
            "{name:<18} {a:>+9.4} {b:>+9.4} {c:>+9.4} {:>+9.4}",
            a.max(b).max(c) - a.min(b).min(c)
        );
    }
    println!("\nWORST OVERSHOOT: {worst:+.4} dB");
    // The headroom constant exists to absorb exactly this. If the estimator
    // changes — more oversampling, different taps — this bound moves and
    // TRUE_PEAK_HEADROOM_DB must be re-derived from the new measurement.
    assert!(
        worst < sp314_dsp::limiter::core::TRUE_PEAK_HEADROOM_DB,
        "estimator underread {worst:+.4} dB now exceeds TRUE_PEAK_HEADROOM_DB; \
         re-measure and raise the constant"
    );
    // The headroom constant exists to absorb exactly this. If the estimator
    // changes — more oversampling, different taps — this bound moves and
    // TRUE_PEAK_HEADROOM_DB must be re-derived from the new measurement.
    assert!(
        worst < sp314_dsp::limiter::core::TRUE_PEAK_HEADROOM_DB,
        "estimator underread {worst:+.4} dB now exceeds TRUE_PEAK_HEADROOM_DB; \
         re-measure and raise the constant"
    );
    println!(
        "\n{:<18} {:>9} {:>9} {:>9} {:>9}",
        "signal", "c=-0.1", "c=-1.0", "c=-3.0", "c=-6.0"
    );
    for (name, s) in &cases {
        let v: Vec<f32> = CEILINGS.iter().map(|&c| over_at(s, c)).collect();
        println!(
            "{name:<18} {:>+9.4} {:>+9.4} {:>+9.4} {:>+9.4}",
            v[0], v[1], v[2], v[3]
        );
    }
}
