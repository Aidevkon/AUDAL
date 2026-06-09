use lineos_types::{StereoBuffer, MasteringIntent, LoudnessTarget};
use m0d::dsp::DspAdapter;

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

    let samples_raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| s.unwrap()).collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().map(|s| s.unwrap() as f32 / max_val).collect()
        }
    };

    let (left, right): (Vec<f32>, Vec<f32>) = if spec.channels == 2 {
        let l: Vec<f32> = samples_raw.iter().step_by(2).cloned().collect();
        let r: Vec<f32> = samples_raw.iter().skip(1).step_by(2).cloned().collect();
        (l, r)
    } else {
        (samples_raw.clone(), samples_raw)
    };

    let mut audio = StereoBuffer {
        left: left.clone(),
        right: right.clone(),
        sample_rate: spec.sample_rate,
        num_frames: left.len(),
    };

    let intent = MasteringIntent {
        target: LoudnessTarget {
            target_lufs: -14.0,
            max_true_peak_db: -1.0,
            max_lra_lu: None,
            platform: "default".into(),
        },
        preset_name: "spotify".to_string(),
        stem_mode: false,
        target_makeup_db: 0.0,
    };

    // --- Autotune ---
    println!("\n=== Autotune ===");
    use sp314_dsp::analysis::PreAnalyzer;
    let pre_analysis = PreAnalyzer::run(&audio.left, &audio.right, spec.sample_rate);
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        -14.0,
    );
    let gain_linear = 10.0_f32.powf(autotune_result.pre_gain_db / 20.0_f32);
    for s in audio.left.iter_mut()  { *s *= gain_linear; }
    for s in audio.right.iter_mut() { *s *= gain_linear; }
    assert_signal_integrity(&audio.left, "autotune_l");
    assert_signal_integrity(&audio.right, "autotune_r");

    // --- Process ---
    println!("\n=== Processing ===");
    let _result = DspAdapter::master(&intent, &mut audio.left, &mut audio.right, audio.sample_rate, None).expect("Mastering failed");
    assert_signal_integrity(&audio.left, "process_offline_l");
    assert_signal_integrity(&audio.right, "process_offline_r");

    // Output Metrics via PreAnalyzer
    let output_analysis = PreAnalyzer::run(&audio.left, &audio.right, spec.sample_rate);
    let out_peak = audio.left.iter().chain(audio.right.iter())
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);
    let out_peak_db = if out_peak < 1e-9 { -144.0 } else { 20.0 * out_peak.log10() };

    println!("\n=== Output Metrics ===");
    println!("  Integrated LUFS: {:.1} LUFS", output_analysis.integrated_lufs);
    println!("  True Peak:       {:.1} dBTP", output_analysis.true_peak_dbtp);
    println!("  Peak:            {:.1} dBFS", out_peak_db);

    assert!(output_analysis.integrated_lufs > -20.0 && output_analysis.integrated_lufs < -8.0,
        "LUFS gate failed: {:.1} LUFS outside [-20, -8] range", output_analysis.integrated_lufs);
    assert!(output_analysis.true_peak_dbtp < -0.5,
        "True peak gate failed: {:.1} dBTP exceeds -0.5 ceiling", output_analysis.true_peak_dbtp);

    // --- Write output WAV ---
    let out_spec = hound::WavSpec {
        channels:        2,
        sample_rate:     spec.sample_rate,
        bits_per_sample: 24,
        sample_format:   hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(output_path, out_spec)
        .expect("Failed to create output WAV");

    let max_pos = 8388607.0_f32;
    let max_neg = 8388608.0_f32;

    assert_signal_integrity(&audio.left, "export_l");
    assert_signal_integrity(&audio.right, "export_r");

    for i in 0..audio.left.len() {
        let l_smp = if audio.left[i]  >= 0.0 { audio.left[i]  * max_pos } else { audio.left[i]  * max_neg };
        let r_smp = if audio.right[i] >= 0.0 { audio.right[i] * max_pos } else { audio.right[i] * max_neg };
        writer.write_sample(l_smp.clamp(-8388608.0, 8388607.0) as i32).unwrap();
        writer.write_sample(r_smp.clamp(-8388608.0, 8388607.0) as i32).unwrap();
    }
    writer.finalize().expect("Failed to write WAV");

    println!("\n=== Output: {} ===", output_path);
    println!("  Format: 24-bit WAV, {} Hz", spec.sample_rate);
    println!("\nDone.");
}
