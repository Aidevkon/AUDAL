//! PINNED ORACLE for F-049 — butter_hp2/butter_lp2 carry Q = 1.414, not the
//! Q = 0.707 Butterworth their comments claim (SQRT_2 where FRAC_1_SQRT_2 was
//! intended, since ecd7895 / day one).
//!
//! These asserts PIN THE CURRENT (WRONG) BEHAVIOUR ON PURPOSE. If you came
//! here because this test failed after you "fixed" butter_hp2 or butter_lp2:
//! stop, read F-049 in FINDINGS.md. Every spectral profile in the repo was
//! measured through these resonant filters; correcting them is an
//! investigation with its own oracles (profile re-measurement, F-047
//! reopening), not a constant swap. When that investigation lands, flip
//! these pins deliberately in the same commit.
//!
//! Run with --nocapture to see the full measured response curves — the
//! prints are kept for exactly that debugging use.

use sp314_dsp::analysis::pre_analysis::{butter_hp2, butter_hp2_q, butter_lp2, Biquad};

const SR: f32 = 48000.0;
const CUTOFF: f32 = 1000.0;

/// Feed a sine at `freq` Hz, discard 100 ms settle, return out/in RMS in dB.
fn measure_magnitude(bq: &mut Biquad, freq: f32, sr: f32) -> f32 {
    let n = 48000_usize;
    let settle = 4800_usize;
    let w = 2.0 * core::f32::consts::PI * freq / sr;
    let mut sum_out_sq = 0.0_f64;
    let mut sum_in_sq = 0.0_f64;
    let mut count = 0_usize;
    for i in 0..n {
        let s = (w * i as f32).sin();
        let out = bq.process(s);
        if i >= settle {
            sum_in_sq += (s as f64) * (s as f64);
            sum_out_sq += (out as f64) * (out as f64);
            count += 1;
        }
    }
    let rms_in = (sum_in_sq / count as f64).sqrt();
    let rms_out = (sum_out_sq / count as f64).sqrt();
    if rms_in < 1e-20 {
        return -144.0;
    }
    20.0 * (rms_out / rms_in).log10() as f32
}

#[test]
fn pinned_current_hp_is_resonant_and_corrected_hp_is_flat() {
    let freqs = [
        100.0, 300.0, 500.0, 700.0, 800.0, 900.0, 950.0, 1000.0, 1050.0, 1100.0, 1200.0, 1500.0,
        2000.0, 4000.0, 8000.0,
    ];
    let correct_q = core::f32::consts::FRAC_1_SQRT_2;

    println!();
    println!("=== HIGHPASS @ {:.0} Hz, SR={:.0} ===", CUTOFF, SR);
    println!(
        "{:<10} {:>14} {:>14} {:>10}",
        "Freq(Hz)", "current(dB)", "corrected(dB)", "delta(dB)"
    );
    for &f in &freqs {
        let mut hp_current = butter_hp2(CUTOFF, SR);
        let mut hp_correct = butter_hp2_q(CUTOFF, SR, correct_q);
        let mag_current = measure_magnitude(&mut hp_current, f, SR);
        let mag_correct = measure_magnitude(&mut hp_correct, f, SR);
        println!(
            "{:<10.0} {:>14.2} {:>14.2} {:>+10.2}",
            f,
            mag_current,
            mag_correct,
            mag_current - mag_correct
        );
    }

    // PIN 1: the current HP peaks near +3.5 dB above cutoff (measured +3.56
    // at 1200 Hz on 2026-07-30). Butterworth would never exceed 0 dB.
    let mut hp = butter_hp2(CUTOFF, SR);
    let peak_1200 = measure_magnitude(&mut hp, 1200.0, SR);
    assert!(
        (3.0..4.2).contains(&peak_1200),
        "current butter_hp2 no longer peaks (+{peak_1200:.2} dB at 1200 Hz). \
         If you corrected the Q constant: read F-049 before proceeding."
    );

    // PIN 2: at the corner, current sits ~+3 dB where Butterworth sits -3 dB
    // (measured delta +6.02 dB).
    let mut hp_cur = butter_hp2(CUTOFF, SR);
    let mut hp_cor = butter_hp2_q(CUTOFF, SR, correct_q);
    let delta_corner =
        measure_magnitude(&mut hp_cur, CUTOFF, SR) - measure_magnitude(&mut hp_cor, CUTOFF, SR);
    assert!(
        (5.5..6.5).contains(&delta_corner),
        "corner delta {delta_corner:.2} dB, expected ~6. See F-049."
    );

    // PIN 3: the corrected designer IS flat — passband never above +0.1 dB.
    for f in [1500.0_f32, 2000.0, 4000.0, 8000.0] {
        let mut hp_cor = butter_hp2_q(CUTOFF, SR, correct_q);
        let mag = measure_magnitude(&mut hp_cor, f, SR);
        assert!(
            mag <= 0.1,
            "butter_hp2_q passband ripple at {f} Hz: {mag:.2} dB — must stay Butterworth-flat"
        );
    }
}

#[test]
fn pinned_current_lp_is_resonant_too() {
    println!();
    println!("=== LOWPASS @ {:.0} Hz, SR={:.0} (current) ===", CUTOFF, SR);
    let freqs = [100.0, 500.0, 800.0, 900.0, 1000.0, 1200.0, 2000.0, 4000.0];
    for &f in &freqs {
        let mut lp = butter_lp2(CUTOFF, SR);
        println!("{:<10.0} {:>14.2}", f, measure_magnitude(&mut lp, f, SR));
    }
    // PIN: same resonance on the LP side (measured +3.56 dB at 900 Hz).
    let mut lp = butter_lp2(CUTOFF, SR);
    let peak_900 = measure_magnitude(&mut lp, 900.0, SR);
    assert!(
        (3.0..4.2).contains(&peak_900),
        "current butter_lp2 no longer peaks (+{peak_900:.2} dB at 900 Hz). See F-049."
    );
}
