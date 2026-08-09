use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use sp314_dsp::metering::measure_integrated_lufs;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use xaak::repo::DspState;

fn load_f32_file(path: &Path) -> Vec<f32> {
    let bytes = fs::read(path).unwrap_or_default();
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_ne_bytes(c.try_into().unwrap()))
        .collect()
}

fn calc_rms(left: &[f32], right: &[f32]) -> f32 {
    let mut sum_sq = 0.0f64;
    let n = left.len().min(right.len());
    if n == 0 {
        return 0.0;
    }
    for i in 0..n {
        sum_sq += (left[i] as f64) * (left[i] as f64) + (right[i] as f64) * (right[i] as f64);
    }
    ((sum_sq / (n * 2) as f64).sqrt()) as f32
}

fn calc_masked_rms(left: &[f32], right: &[f32], voice_mask: &[bool], target_voice: bool) -> f32 {
    let frame_size = 480;
    let n_frames = voice_mask.len();
    let mut sum_sq = 0.0f64;
    let mut count = 0usize;

    for f in 0..n_frames {
        if voice_mask[f] == target_voice {
            let start = f * frame_size;
            let end = ((f + 1) * frame_size).min(left.len());
            for i in start..end {
                sum_sq += (left[i] as f64) * (left[i] as f64) + (right[i] as f64) * (right[i] as f64);
                count += 2;
            }
        }
    }

    if count == 0 {
        0.0
    } else {
        (sum_sq / count as f64).sqrt() as f32
    }
}

#[test]
#[ignore]
fn w9_normalizer_chain() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let mut reader = hound::WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32 / 32768.0).collect(),
    };
    let total_frames = samples.len() / spec.channels as usize;
    let mut in_l = Vec::with_capacity(total_frames);
    let mut in_r = Vec::with_capacity(total_frames);
    if spec.channels == 2 {
        for chunk in samples.chunks_exact(2) {
            in_l.push(chunk[0]);
            in_r.push(chunk[1]);
        }
    } else {
        for s in samples {
            in_l.push(s);
            in_r.push(s);
        }
    }

    let in_rms = calc_rms(&in_l, &in_r);
    let in_lufs = measure_integrated_lufs(&in_l, &in_r);

    // Build voice mask from input (frame_size = 480, threshold = 0.02)
    let frame_size = 480;
    let n_frames = total_frames / frame_size;
    let mut voice_mask = Vec::with_capacity(n_frames);
    for f in 0..n_frames {
        let start = f * frame_size;
        let end = (start + frame_size).min(total_frames);
        let f_rms = calc_rms(&in_l[start..end], &in_r[start..end]);
        voice_mask.push(f_rms > 0.02);
    }

    let in_voice_rms = calc_masked_rms(&in_l, &in_r, &voice_mask, true);
    let in_music_rms = calc_masked_rms(&in_l, &in_r, &voice_mask, false);

    // RUN A: Noduck
    println!("\n=== RUN A: NODUCK ===");
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let req_a = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w9_podcast".to_string()),
        track_id: Some("w9A".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false),
        use_nmfd: None,
    };

    let (_blob, _, _pcm_a, _, _, artifacts_a) = run_dsp(
        &req_a,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w9-podcast-A".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_dir_a.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for noduck");

    let mut noduck_voice_rms = 0.0f32;
    let mut noduck_music_rms = 0.0f32;

    if let Some(ref path) = artifacts_a.persisted_master {
        println!("Persisted master path: {}", path.display());
    }

    if let Some((l_path, r_path)) = artifacts_a.pre_master_guards {
        let pre_l = load_f32_file(l_path.path());
        let pre_r = load_f32_file(r_path.path());
        let pre_rms = calc_rms(&pre_l, &pre_r);
        let pre_lufs = measure_integrated_lufs(&pre_l, &pre_r);

        noduck_voice_rms = calc_masked_rms(&pre_l, &pre_r, &voice_mask, true);
        noduck_music_rms = calc_masked_rms(&pre_l, &pre_r, &voice_mask, false);

        let first10_lufs = measure_integrated_lufs(&pre_l[..480000.min(pre_l.len())], &pre_r[..480000.min(pre_r.len())]);
        let last10_start = (110 * 48000).min(pre_l.len());
        let last10_lufs = measure_integrated_lufs(&pre_l[last10_start..], &pre_r[last10_start..]);

        let window_samples = 10 * 48000;
        let mut rms_vals = Vec::new();
        for i in 0..12 {
            let start = i * window_samples;
            let end = ((i + 1) * window_samples).min(pre_l.len());
            if start < pre_l.len() {
                rms_vals.push(format!("{:.6}", calc_rms(&pre_l[start..end], &pre_r[start..end])));
            }
        }

        println!("Input RMS: {:.6} | Input LUFS: {:.2}", in_rms, in_lufs);
        println!("Pre-master RMS: {:.6} | Pre-master LUFS: {:.2}", pre_rms, pre_lufs);
        println!("Pre-master LUFS First 10s: {:.2} | Last 10s: {:.2}", first10_lufs, last10_lufs);
        println!("10s Window RMS: {}", rms_vals.join(", "));
    }

    // RUN B: Ducked
    println!("\n=== RUN B: DUCKED ===");
    let state_tmp_b = tempfile::TempDir::new().unwrap();
    let out_dir_b = tempfile::TempDir::new().unwrap();
    let req_b = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w9_podcast".to_string()),
        track_id: Some("w9B".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: None,
    };

    let (_blob, _, _pcm_b, _, _, artifacts_b) = run_dsp(
        &req_b,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w9-podcast-B".to_string(),
        state_tmp_b.path().to_str().unwrap(),
        out_dir_b.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for ducked");

    let mut ducked_voice_rms = 0.0f32;
    let mut ducked_music_rms = 0.0f32;

    if let Some(ref path) = artifacts_b.persisted_master {
        println!("Persisted master path: {}", path.display());
    }

    if let Some((l_path, r_path)) = artifacts_b.pre_master_guards {
        let pre_l = load_f32_file(l_path.path());
        let pre_r = load_f32_file(r_path.path());
        let pre_rms = calc_rms(&pre_l, &pre_r);
        let pre_lufs = measure_integrated_lufs(&pre_l, &pre_r);

        ducked_voice_rms = calc_masked_rms(&pre_l, &pre_r, &voice_mask, true);
        ducked_music_rms = calc_masked_rms(&pre_l, &pre_r, &voice_mask, false);

        let first10_lufs = measure_integrated_lufs(&pre_l[..480000.min(pre_l.len())], &pre_r[..480000.min(pre_r.len())]);
        let last10_start = (110 * 48000).min(pre_l.len());
        let last10_lufs = measure_integrated_lufs(&pre_l[last10_start..], &pre_r[last10_start..]);

        let window_samples = 10 * 48000;
        let mut rms_vals = Vec::new();
        for i in 0..12 {
            let start = i * window_samples;
            let end = ((i + 1) * window_samples).min(pre_l.len());
            if start < pre_l.len() {
                rms_vals.push(format!("{:.6}", calc_rms(&pre_l[start..end], &pre_r[start..end])));
            }
        }

        println!("Input RMS: {:.6} | Input LUFS: {:.2}", in_rms, in_lufs);
        println!("Pre-master RMS: {:.6} | Pre-master LUFS: {:.2}", pre_rms, pre_lufs);
        println!("Pre-master LUFS First 10s: {:.2} | Last 10s: {:.2}", first10_lufs, last10_lufs);
        println!("10s Window RMS: {}", rms_vals.join(", "));
    }

    println!("\n=== MASKED RMS COMPARISON ===");
    println!("voice_rms input: {:.6} | noduck: {:.6} | ducked: {:.6}", in_voice_rms, noduck_voice_rms, ducked_voice_rms);
    println!("music_rms input: {:.6} | noduck: {:.6} | ducked: {:.6}", in_music_rms, noduck_music_rms, ducked_music_rms);

    let voice_ratio_db = 20.0 * (ducked_voice_rms / noduck_voice_rms.max(1e-10)).log10();
    let music_ratio_db = 20.0 * (ducked_music_rms / noduck_music_rms.max(1e-10)).log10();

    println!("ratio voice_ducked/voice_noduck: {:.4} dB", voice_ratio_db);
    println!("ratio music_ducked/music_noduck: {:.4} dB", music_ratio_db);
}
