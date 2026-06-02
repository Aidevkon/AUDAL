// src/bin/test_engine.rs
// One-shot engine test — not part of the library, dev-time only

use sp314_dsp::{
    masking_eq::{MaskingAwareEQ, MaskingEQConfig},
    compressor::stereo::{CompressorV3, CompressorV3Config},
    compressor::core::CompressorBandConfig,
    pipeline::engine::{Sp314MasteringEngine, EngineConfig},
};

fn assert_signal_integrity(signal: &[f32], node: &'static str) {
    let has_nan = signal.iter().any(|s| s.is_nan() || s.is_infinite());
    let rms: f32 = signal.iter().map(|s| s*s).sum::<f32>() / signal.len() as f32;
    let is_silent = rms < 1e-15 && signal.len() > 1000;
    assert!(!has_nan, "Signal integrity violation at '{}': NaN/Inf detected", node);
    assert!(!is_silent, "Signal integrity violation at '{}': audio silenced RMS={:.2e}", node, rms);
}

fn main() {
    let input_path  = "/home/aidevcon/Music/test.wav";
    let output_path = "/home/aidevcon/Music/test_mastered.wav";

    // --- Read WAV ---
    let mut reader = hound::WavReader::open(input_path)
        .expect("Failed to open input WAV");

    let spec = reader.spec();
    println!("=== Input: {} ===", input_path);
    println!("  Sample rate:  {} Hz", spec.sample_rate);
    println!("  Channels:     {}", spec.channels);
    println!("  Bit depth:    {}", spec.bits_per_sample);

    // Read samples as f32
    let samples_raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap()).collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap() as f32 / max_val).collect()
        }
    };

    // De-interleave stereo
    let (mut left, mut right): (Vec<f32>, Vec<f32>) = if spec.channels == 2 {
        let l: Vec<f32> = samples_raw.iter().step_by(2).cloned().collect();
        let r: Vec<f32> = samples_raw.iter().skip(1).step_by(2).cloned().collect();
        (l, r)
    } else {
        // Mono → duplicate to stereo
        (samples_raw.clone(), samples_raw)
    };

    let duration_secs = left.len() as f32 / spec.sample_rate as f32;
    let mins = (duration_secs / 60.0) as u32;
    let secs = (duration_secs % 60.0) as u32;
    println!("  Duration:     {}:{:02} ({} samples per channel)",
             mins, secs, left.len());

    // --- Configure Engine ---
    // Default mastering config — balanced settings
    let band_config = CompressorBandConfig {
        threshold_db:      -18.0,
        ratio:               3.0,
        knee_db:             2.0,
        attack_ms:          10.0,
        release_ms:        150.0,
        makeup_db:           2.0,
        crossover_hz:      150.0,
    };

    let comp_config = CompressorV3Config {
        mid_config:  band_config.clone(),
        side_config: CompressorBandConfig {
            threshold_db: -24.0,
            ratio:          2.0,
            knee_db:        2.0,
            attack_ms:     20.0,
            release_ms:   200.0,
            makeup_db:      0.0,
            crossover_hz: 150.0,
        },
    };

    let eq_config = MaskingEQConfig {
        target_db:      [1.5, 1.0, 0.5, 0.0, 0.5, 1.0, 1.5, 1.0],
        mask_margin_db: 3.0,
        max_boost_db:   6.0,
        target_phon:   80.0,
    };

    let engine_config = EngineConfig {
        eq_config,
        comp_config,
        parallel_mix:     0.7,   // 70% wet compression
        target_makeup_db: 0.0,   // let adaptive budget handle it
        limiter_config:   sp314_dsp::limiter::LimiterConfig::default(),
        restoration_config: sp314_dsp::restoration::RestorationConfig::bypass(),
        harmonic_config:  None,
        clipper_enabled:  false,
    };

    let mut engine = Sp314MasteringEngine::new(engine_config, spec.sample_rate)
        .expect("Engine init failed");

    // --- Autotune ---
    println!("\n=== Autotune ===");
    use sp314_dsp::analysis::PreAnalyzer;
    let pre_analysis = PreAnalyzer::run(&left, &right, spec.sample_rate);
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        -14.0,
    );
    let gain_linear = 10.0_f32.powf(autotune_result.pre_gain_db / 20.0_f32);
    for s in left.iter_mut()  { *s *= gain_linear; }
    for s in right.iter_mut() { *s *= gain_linear; }
    assert_signal_integrity(&left, "autotune_l");
    assert_signal_integrity(&right, "autotune_r");

    // --- Process ---
    println!("\n=== Processing ===");
    let telemetry = engine.process_offline(&mut left, &mut right);
    assert_signal_integrity(&left, "process_offline_l");
    assert_signal_integrity(&right, "process_offline_r");

    println!("  Pre-pass peak:  {:.1} dBFS", telemetry.peak_db);
    println!("  Pre-pass RMS:   {:.1} dBFS", telemetry.rms_db);

    let pad = if telemetry.peak_db > -3.0 { -12.0 }
              else if telemetry.peak_db > -9.0 { -6.0 }
              else { 0.0 };
    println!("  Adaptive pad:   {:.1} dB ({})",
             pad,
             if pad == -12.0 { "hot track" }
             else if pad == -6.0 { "normal track" }
             else { "quiet track" });

    // --- Measure output ---
    let out_peak = left.iter().chain(right.iter())
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    let out_rms_sq = (left.iter().map(|s| s*s).sum::<f32>() +
                      right.iter().map(|s| s*s).sum::<f32>())
                     / (2.0 * left.len() as f32);
    let out_peak_db = if out_peak < 1e-9 { -144.0 } else { 20.0 * out_peak.log10() };
    let out_rms_db  = if out_rms_sq < 1e-15 { -144.0 } else { 10.0 * out_rms_sq.log10() };

    println!("\n=== Output Metrics ===");
    println!("  Peak:  {:.1} dBFS", out_peak_db);
    println!("  RMS:   {:.1} dBFS", out_rms_db);

    // --- Write output WAV ---
    let out_spec = hound::WavSpec {
        channels:        2,
        sample_rate:     spec.sample_rate,
        bits_per_sample: 24,
        sample_format:   hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, out_spec)
        .expect("Failed to create output WAV");

    // 24-bit safe conversion:
    // max positive = 8388607 (2^23 - 1), max negative = -8388608 (2^23)
    // Hard clip at 1.0 * 8388608 = 8388608 would panic in hound.
    // Use asymmetric scaling + clamp to prevent out-of-range panic.
    let max_pos = 8388607.0_f32;
    let max_neg = 8388608.0_f32;

    assert_signal_integrity(&left, "export_l");
    assert_signal_integrity(&right, "export_r");

    for i in 0..left.len() {
        let l_smp = if left[i]  >= 0.0 { left[i]  * max_pos } else { left[i]  * max_neg };
        let r_smp = if right[i] >= 0.0 { right[i] * max_pos } else { right[i] * max_neg };
        writer.write_sample(l_smp.clamp(-8388608.0, 8388607.0) as i32).unwrap();
        writer.write_sample(r_smp.clamp(-8388608.0, 8388607.0) as i32).unwrap();
    }
    writer.finalize().expect("Failed to write WAV");

    println!("\n=== Output: {} ===", output_path);
    println!("  Format: 24-bit WAV, {} Hz", spec.sample_rate);
    println!("\nDone.");
}
