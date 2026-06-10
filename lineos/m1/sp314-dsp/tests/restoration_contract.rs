// tests/restoration_contract.rs
// Dev-time rules apply — std::f32 permitted for test signal generation.

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

use sp314_dsp::pipeline::presets::MasteringTarget;
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
    assert!(
        out_l.abs() < 1e-8,
        "Gate did not close on silence (out: {})",
        out_l
    );
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
fn music_preset_bypasses_gate_and_hum() {
    let target = MasteringTarget::AggressiveEDM;
    let config = target.engine_config(48000);

    assert_eq!(config.restoration_config.hum_enabled, false);
    assert_eq!(config.restoration_config.gate_enabled, false);
}
