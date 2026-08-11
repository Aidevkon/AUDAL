use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
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

fn print_dynamics(left: &[f32], right: &[f32], label: &str) {
    let window_samples = 19200; // 400ms @ 48kHz
    let n = left.len().min(right.len());
    let n_windows = n / window_samples;
    if n_windows == 0 {
        return;
    }

    let mut db_vals = Vec::with_capacity(n_windows);
    for i in 0..n_windows {
        let start = i * window_samples;
        let end = start + window_samples;
        let rms = calc_rms(&left[start..end], &right[start..end]);
        if rms > 1e-7 {
            let db = 20.0 * rms.log10();
            db_vals.push(db);
        }
    }

    if db_vals.is_empty() {
        return;
    }

    db_vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = db_vals.len();
    let idx_10 = ((count as f32 * 0.10) as usize).min(count - 1);
    let idx_50 = ((count as f32 * 0.50) as usize).min(count - 1);
    let idx_90 = ((count as f32 * 0.90) as usize).min(count - 1);

    let p10 = db_vals[idx_10];
    let p50 = db_vals[idx_50];
    let p90 = db_vals[idx_90];

    println!(
        "DYNAMICS  {}: p10={:.2} p50={:.2} p90={:.2} range={:.2} dB",
        label,
        p10,
        p50,
        p90,
        p90 - p10
    );
}

#[test]
#[ignore]
fn w10_duck_dynamics() {
    let input_path = Path::new("/tmp/w9/podcast_loud_bed.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_loud_bed.wav missing");
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
        project_id: Some("w10_podcast".to_string()),
        track_id: Some("w10A".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false),
        use_nmfd: None,
    };

    let (blob_a, _, _pcm_a, _, _, artifacts_a) = run_dsp(
        &req_a,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w10-podcast-A".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_dir_a.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for noduck");

    println!("[FINAL] integrated_lufs={:.2} true_peak={:.2} \
              too_quiet={} spotify_ok={} apple_pod_ok={}",
        blob_a.loudness().expect("test expects Certified").integrated_lufs,
        blob_a.loudness().expect("test expects Certified").true_peak_dbtp,
        blob_a.loudness().expect("test expects Certified").too_quiet_for_mobile,
        blob_a.loudness().expect("test expects Certified").spotify_compliant,
        blob_a.loudness().expect("test expects Certified").apple_podcasts_compliant);

    let mut noduck_pre_l = Vec::new();
    let mut noduck_pre_r = Vec::new();

    if let Some((l_path, r_path)) = artifacts_a.pre_master_guards {
        noduck_pre_l = load_f32_file(l_path.path());
        noduck_pre_r = load_f32_file(r_path.path());
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
        project_id: Some("w10_podcast".to_string()),
        track_id: Some("w10B".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: None,
    };

    let (blob_b, _, _pcm_b, _, _, artifacts_b) = run_dsp(
        &req_b,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w10-podcast-B".to_string(),
        state_tmp_b.path().to_str().unwrap(),
        out_dir_b.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for ducked");

    println!("[FINAL] integrated_lufs={:.2} true_peak={:.2} \
              too_quiet={} spotify_ok={} apple_pod_ok={}",
        blob_b.loudness().expect("test expects Certified").integrated_lufs,
        blob_b.loudness().expect("test expects Certified").true_peak_dbtp,
        blob_b.loudness().expect("test expects Certified").too_quiet_for_mobile,
        blob_b.loudness().expect("test expects Certified").spotify_compliant,
        blob_b.loudness().expect("test expects Certified").apple_podcasts_compliant);

    let mut ducked_pre_l = Vec::new();
    let mut ducked_pre_r = Vec::new();

    if let Some((l_path, r_path)) = artifacts_b.pre_master_guards {
        ducked_pre_l = load_f32_file(l_path.path());
        ducked_pre_r = load_f32_file(r_path.path());
    }

    println!("\n=== DYNAMICS METRICS ===");
    print_dynamics(&in_l, &in_r, "input");
    print_dynamics(&noduck_pre_l, &noduck_pre_r, "noduck");
    print_dynamics(&ducked_pre_l, &ducked_pre_r, "ducked");
}
