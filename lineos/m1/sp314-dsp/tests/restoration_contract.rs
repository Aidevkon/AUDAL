// allow: contract tests index synthetic buffers the same way the
// DSP code under test does; iterator rewrites add nothing here.
#![allow(clippy::needless_range_loop)]

// tests/restoration_contract.rs
// Dev-time rules apply — std::f32 permitted for test signal generation.

use approx::assert_abs_diff_eq;
use sp314_dsp::restoration::RestorationChain;

#[test]
fn dehum_removes_50hz_sine() {
    let mut chain = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let len = 96000;
    let mut left = vec![0.0_f32; len];
    let mut right = vec![0.0_f32; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s_50 = (2.0 * std::f32::consts::PI * 50.0 * t).sin();
        let s_1k = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();

        left[i] = s_50 + s_1k;
        right[i] = left[i];
    }

    // Process the mixed signal
    chain.process(&mut left, &mut right);

    // To cleanly separate 50Hz and 1kHz in the output without a complex FFT,
    // we can just run pure 50Hz and pure 1kHz through the chain separately
    // to measure the energy reduction, since the chain is mostly linear.
    // Wait, the De-Esser is nonlinear, but it won't react to -12 dBFS low frequencies.
    let mut chain_50 = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let mut left_50 = vec![0.0_f32; len];
    let mut right_50 = vec![0.0_f32; len];
    for i in 0..len {
        let t = i as f32 / 48000.0;
        left_50[i] = (2.0 * std::f32::consts::PI * 50.0 * t).sin();
        right_50[i] = left_50[i];
    }
    chain_50.process(&mut left_50, &mut right_50);

    let mut output_energy_50 = 0.0;
    for i in 48000..len {
        // Skip transient
        output_energy_50 += left_50[i] * left_50[i];
    }

    let mut ref_energy_50 = 0.0;
    for i in 48000..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 50.0 * t).sin();
        ref_energy_50 += s * s;
    }

    assert!(
        output_energy_50 < ref_energy_50 * 0.01,
        "50Hz energy not reduced below 1%"
    );

    let mut chain_1k = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let mut left_1k = vec![0.0_f32; len];
    let mut right_1k = vec![0.0_f32; len];
    for i in 0..len {
        let t = i as f32 / 48000.0;
        left_1k[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
        right_1k[i] = left_1k[i];
    }
    chain_1k.process(&mut left_1k, &mut right_1k);

    let mut output_energy_1k = 0.0;
    for i in 48000..len {
        output_energy_1k += left_1k[i] * left_1k[i];
    }

    let mut ref_energy_1k = 0.0;
    for i in 48000..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
        ref_energy_1k += s * s;
    }

    assert!(
        output_energy_1k > ref_energy_1k * 0.99,
        "1kHz energy reduced too much"
    );
}

#[test]
fn deess_reduces_high_frequency_bursts() {
    let mut chain = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);

    let len = 4096;
    let mut left = vec![0.0_f32; len * 2];
    let mut right = vec![0.0_f32; len * 2];

    // -12 dBFS = 0.25 linear (approx)
    let amp_500 = 0.25;
    // -6 dBFS = 0.5 linear (approx)
    let amp_8000 = 0.5;

    for i in 0..len {
        let t = i as f32 / 48000.0;
        left[i] = (2.0 * std::f32::consts::PI * 500.0 * t).sin() * amp_500;
        right[i] = left[i];
    }

    for i in len..(len * 2) {
        let t = i as f32 / 48000.0;
        left[i] = (2.0 * std::f32::consts::PI * 8000.0 * t).sin() * amp_8000;
        right[i] = left[i];
    }

    chain.process(&mut left, &mut right);

    let mut peak_500 = 0.0_f32;
    // measure peak of 500Hz after transient
    for i in 1000..len {
        peak_500 = peak_500.max(left[i].abs());
    }

    let mut peak_8000 = 0.0_f32;
    // measure peak of 8000Hz after attack
    for i in (len + 1000)..(len * 2) {
        peak_8000 = peak_8000.max(left[i].abs());
    }

    assert!(
        peak_8000 < amp_8000 * 0.80,
        "De-esser did not reduce 8kHz enough (peak: {})",
        peak_8000
    );
    assert!(
        peak_500 > amp_500 * 0.95,
        "De-esser reduced 500Hz incorrectly (peak: {})",
        peak_500
    );
}

use sp314_dsp::restoration::gate::NoiseGate;
use sp314_dsp::restoration::RestorationConfig;

#[test]
fn noise_gate_closes_on_silence() {
    let mut gate = NoiseGate::new(48000.0, -6.0_f32);
    let mut _last_l = 1.0;

    // warm up with silence for 48000 samples (1 second) to fully close
    for _ in 0..48000 {
        let (l, _) = gate.process_stereo(0.0, 0.0);
        _last_l = l;
    }

    // Now pass a small signal and see if it's muted
    let (out_l, _) = gate.process_stereo(1e-6, 1e-6);
    assert_abs_diff_eq!(out_l, 0.0_f32, epsilon = 1e-8);
}

#[test]
fn noise_gate_opens_on_signal() {
    let mut gate = NoiseGate::new(48000.0, -6.0_f32);

    // warm up with silence
    for _ in 0..5000 {
        gate.process_stereo(0.0, 0.0);
    }

    let mut peak = 0.0_f32;
    for _ in 0..5000 {
        let (l, _) = gate.process_stereo(0.5, 0.5);
        peak = peak.max(l.abs());
    }

    assert!(peak > 0.4, "Gate did not open on signal (peak: {})", peak);
}

#[test]
fn noise_gate_hold_prevents_chatter() {
    let mut gate = NoiseGate::new(48000.0, -6.0_f32);

    // Open gate
    for _ in 0..1000 {
        gate.process_stereo(0.5, 0.5);
    }

    // 10ms of silence (480 samples)
    let mut min_gain = 1.0_f32;
    for _ in 0..480 {
        // process silence, but we measure the gain by passing a tiny test signal
        // Wait, NoiseGate doesn't expose gain. We can infer it by passing a signal and checking amplitude.
        // But if we pass a signal it keeps it open!
        // We must pass 0.0, and infer gain internally? No, we can just pass a signal just BELOW threshold!
        let (l, _) = gate.process_stereo(1e-5, 1e-5);
        let gain = l / 1e-5;
        min_gain = min_gain.min(gain);
    }

    assert!(
        min_gain > 0.95,
        "Gate closed too quickly during hold period (min_gain: {})",
        min_gain
    );
}

#[test]
fn lowcut_removes_sub_80hz() {
    let mut chain = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let len = 48000;
    let mut left = vec![0.0_f32; len];
    let mut right = vec![0.0_f32; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        left[i] = (2.0 * std::f32::consts::PI * 40.0 * t).sin();
        right[i] = left[i];
    }

    let mut in_energy = 0.0;
    for i in 24000..len {
        in_energy += left[i] * left[i];
    }

    chain.process(&mut left, &mut right);

    let mut out_energy = 0.0;
    for i in 24000..len {
        out_energy += left[i] * left[i];
    }

    assert!(
        out_energy < in_energy * 0.10,
        "40Hz not attenuated enough by low-cut"
    );
}

#[test]
fn deess_preserves_music_bed() {
    // A music bed (200Hz + 1kHz) plays alongside an 8kHz sibilant burst.
    // Split-band de-essing must reduce the 8kHz burst without pumping the bed.
    let mut chain = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let len = 48000; // 1 second

    let mut left = vec![0.0_f32; len];
    let mut right = vec![0.0_f32; len];

    // Continuous music bed (200Hz + 1kHz) at moderate level
    let bed_amp = 0.25; // -12 dBFS
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let bed = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * bed_amp
            + (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * bed_amp;
        // Add 8kHz sibilant burst in the second half only
        let sib = if i >= len / 2 {
            (2.0 * std::f32::consts::PI * 8000.0 * t).sin() * 0.5
        } else {
            0.0
        };
        left[i] = bed + sib;
        right[i] = left[i];
    }

    // Measure total energy in the first half (bed only, no sibilant) before processing
    let mut pre_bed_energy = 0.0_f64;
    for i in 1000..(len / 2) {
        pre_bed_energy += (left[i] as f64) * (left[i] as f64);
    }

    chain.process(&mut left, &mut right);

    // Measure bed-only energy after processing (first half has no sibilant)
    let mut post_bed_energy = 0.0_f64;
    for i in 1000..(len / 2) {
        post_bed_energy += (left[i] as f64) * (left[i] as f64);
    }

    // Bed energy should be virtually unchanged (within 5% — the LR4
    // crossover sums flat, so low/mid content passes through untouched)
    let ratio = post_bed_energy / pre_bed_energy;
    assert!(
        ratio > 0.95 && ratio < 1.05,
        "Music bed energy changed by split-band de-esser (ratio: {:.4})",
        ratio,
    );

    // The 8kHz burst in the second half should still be reduced
    let mut post_peak_8k = 0.0_f32;
    for i in (len / 2 + 4800)..len {
        post_peak_8k = post_peak_8k.max(left[i].abs());
    }
    // Input peak was bed_amp * 2 + 0.5 = 1.0; the 8kHz component (0.5) should be ducked.
    // The theoretical peak is ~0.68 + phase shift, which is ~0.72.
    assert!(
        post_peak_8k < 0.75,
        "8kHz burst not reduced enough (peak: {:.4})",
        post_peak_8k,
    );
}

#[test]
fn deess_preserves_stereo_image() {
    // Asymmetric 8kHz burst: louder on L than R.
    // Linked envelope must apply IDENTICAL reduction to both channels.
    let mut chain = RestorationChain::new(48000.0, RestorationConfig::voice(), -6.0_f32);
    let len = 48000;

    let mut left = vec![0.0_f32; len];
    let mut right = vec![0.0_f32; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        // L: strong 8kHz at -3 dBFS (0.707)
        left[i] = (2.0 * std::f32::consts::PI * 8000.0 * t).sin() * 0.707;
        // R: weaker 8kHz at -12 dBFS (0.25)
        right[i] = (2.0 * std::f32::consts::PI * 8000.0 * t).sin() * 0.25;
    }

    chain.process(&mut left, &mut right);

    // After the envelope has settled (skip first 100ms = 4800 samples),
    // the L/R ratio must remain 0.707/0.25 = 2.828 because the linked
    // envelope applies the SAME reduction factor to both channels.
    let expected_ratio = 0.707_f32 / 0.25;
    let mut max_ratio_err = 0.0_f32;
    for i in 4800..len {
        if right[i].abs() > 0.05 {
            let actual_ratio = left[i] / right[i];
            let err = (actual_ratio - expected_ratio).abs() / expected_ratio;
            max_ratio_err = max_ratio_err.max(err);
        }
    }

    assert!(
        max_ratio_err < 0.01,
        "Stereo image wandered: max L/R ratio error {:.4} (expected < 1%)",
        max_ratio_err,
    );
}
