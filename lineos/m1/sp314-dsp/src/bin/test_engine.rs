// src/bin/test_engine.rs
// One-shot engine test — not part of the library, dev-time only

use sp314_dsp::{
    pipeline::engine::Sp314MasteringEngine,
    pipeline::dither::TpdfDither,
};

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
            reader.samples::<f32>().map(|s: Result<f32, _>| s.unwrap()).collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s: Result<i32, _>| s.unwrap() as f32 / max_val).collect()
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
    let target = sp314_dsp::pipeline::presets::MasteringTarget::SpotifyV3;
    let base_config = target.engine_config(spec.sample_rate);

    // --- Autotune ---
    println!("\n=== Processing ===");
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(&left, &right, base_config.clone(), target, spec.sample_rate);
    println!("Autotuner: makeup_db={:.1}, rms={:.1}, iterations={}, converged={}",
        autotune_result.makeup_db, autotune_result.achieved_rms,
        autotune_result.iterations, autotune_result.converged);

    let mut tuned_config = base_config;
    tuned_config.target_makeup_db = autotune_result.makeup_db;

    let mut engine = Sp314MasteringEngine::new(tuned_config, spec.sample_rate)
        .expect("Engine init failed");

    let telemetry = engine.process_offline(&mut left, &mut right);

    println!("  Pre-pass peak:  {:.1} dBFS", telemetry.peak_db);
    println!("  Pre-pass RMS:   {:.1} dBFS", telemetry.rms_db);
    println!("  Pre-pass LUFS:  {:.1} LUFS", telemetry.lufs);

    // --- Measure output ---
    let out_peak = left.iter().chain(right.iter())
        .map(|s| libm::fabsf(*s))
        .fold(0.0_f32, f32::max);
    let out_rms_sq = (left.iter().map(|s| s*s).sum::<f32>() +
                      right.iter().map(|s| s*s).sum::<f32>())
                     / (2.0 * left.len() as f32);
    let out_peak_db = if out_peak < 1e-9 { -144.0 } else { 20.0 * libm::log10f(out_peak) };
    let out_rms_db  = if out_rms_sq < 1e-15 { -144.0 } else { 10.0 * libm::log10f(out_rms_sq) };

    println!("\n=== Output Metrics ===");
    println!("  Limiter ceiling: -0.5 dBFS");
    println!("  Output peak post-limiter: {:.1} dBFS", out_peak_db);
    let out_telemetry = sp314_dsp::pipeline::telemetry::analyze_offline_pre_pass(&left, &right);
    println!("  Output LUFS:   {:.1} LUFS", out_telemetry.lufs);
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

    let mut dither = TpdfDither::new(0x5EED_1234_ABCD_EF01);

    for i in 0..left.len() {
        let l = dither.process_sample(left[i],  left[i]  >= 0.0);
        let r = dither.process_sample(right[i], right[i] >= 0.0);
        writer.write_sample(l).unwrap();
        writer.write_sample(r).unwrap();
    }
    writer.finalize().expect("Failed to write WAV");

    println!("  Dither: TPDF 24-bit (seed: 0x5EED_1234_ABCD_EF01)");

    println!("\n=== Output: {} ===", output_path);
    println!("  Format: 24-bit WAV, {} Hz", spec.sample_rate);
    
    // SHA-256 of output file — the Golden Hash
    let file_bytes = std::fs::read(output_path).unwrap();
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&file_bytes);
    let result = hasher.finalize();
    let hex_hash = hex::encode(result);
    println!("Golden Hash (SHA-256): {}", hex_hash);
    
    println!("\nDone.");
}
