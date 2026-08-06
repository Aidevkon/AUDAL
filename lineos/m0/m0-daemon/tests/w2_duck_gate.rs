//! w2_duck_gate.rs
//! W2.2 ControlTrack consumer - duck στο M (bass+harmonics). Sensor-agnostic: πιστότητα στο posterior stream, όποιο κι αν είναι. Gate: w2_duck_gate + splice fixtures.

use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::fs;
use std::sync::Arc;
use xaak::repo::DspState;

fn fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/duck_splice")
        .join(name)
}

#[test]
#[ignore]
fn w2_duck_gate_synth() {
    let path = fixture_path("duck_splice_synth_snr-15.flac");
    assert!(path.exists(), "Missing fixture: {}", path.display());

    // Paths
    let noduck_dest = "/tmp/w2_synth_noduck.wav";
    let ducked_dest = "/tmp/w2_synth_ducked.wav";
    let csv_dest = "/tmp/w2_synth_ducked.csv";
    let generated_csv = std::env::temp_dir().join("vad-trace-w2synthB.csv");

    let _ = fs::remove_file(noduck_dest);
    let _ = fs::remove_file(ducked_dest);
    let _ = fs::remove_file(csv_dest);
    let _ = fs::remove_file(&generated_csv);

    // RUN A: Baseline (noduck)
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let req_a = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w2_synth".to_string()),
        track_id: Some("w2synthA".to_string()),
        mix_levels: None, normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false), // No observer -> empty track -> gain 1.0
    };

    let (_blob, _, _pcm_a, _, _, artifacts_a) = run_dsp(
        &req_a,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w2-synth-A".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_dir_a.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for noduck");

    fs::copy(artifacts_a.persisted_master.unwrap(), noduck_dest).unwrap();
    if let Some((l, r)) = artifacts_a.pre_master_guards {
        fs::copy(l.path(), "/tmp/w2_synth_noduck_pre_l.f32").unwrap();
        fs::copy(r.path(), "/tmp/w2_synth_noduck_pre_r.f32").unwrap();
        println!("Written noduck pre-master L: /tmp/w2_synth_noduck_pre_l.f32");
    }
    println!("Written noduck WAV: {}", noduck_dest);

    // RUN B: Ducked
    let state_tmp_b = tempfile::TempDir::new().unwrap();
    let out_dir_b = tempfile::TempDir::new().unwrap();
    let req_b = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w2_synth".to_string()),
        track_id: Some("w2synthB".to_string()),
        mix_levels: None, normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true), // Ducking active
    };

    let (_blob, _, _pcm_b, _, _, artifacts_b) = run_dsp(
        &req_b,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w2-synth-B".to_string(),
        state_tmp_b.path().to_str().unwrap(),
        out_dir_b.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for ducked");

    fs::copy(artifacts_b.persisted_master.unwrap(), ducked_dest).unwrap();
    println!("Written ducked WAV: {}", ducked_dest);

    if let Some((l, r)) = artifacts_b.pre_master_guards {
        fs::copy(l.path(), "/tmp/w2_synth_ducked_pre_l.f32").unwrap();
        fs::copy(r.path(), "/tmp/w2_synth_ducked_pre_r.f32").unwrap();
        println!("Written ducked pre-master L: /tmp/w2_synth_ducked_pre_l.f32");
    }

    assert!(generated_csv.exists(), "CSV not found!");
    fs::copy(&generated_csv, csv_dest).unwrap();
    let csv_content = fs::read_to_string(csv_dest).unwrap();
    println!("CSV Lines: {}", csv_content.lines().count());
    println!("Written CSV: {}", csv_dest);
}

#[test]
#[ignore]
fn w2_duck_gate_real() {
    let path = fixture_path("duck_splice_real_snr-15.flac");
    assert!(path.exists(), "Missing fixture: {}", path.display());

    // Paths
    let noduck_dest = "/tmp/w2_real_noduck.wav";
    let ducked_dest = "/tmp/w2_real_ducked.wav";
    let csv_dest = "/tmp/w2_real_ducked.csv";
    let generated_csv = std::env::temp_dir().join("vad-trace-w2realB.csv");

    let _ = fs::remove_file(noduck_dest);
    let _ = fs::remove_file(ducked_dest);
    let _ = fs::remove_file(csv_dest);
    let _ = fs::remove_file(&generated_csv);

    // RUN A: Baseline (noduck)
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let req_a = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w2_real".to_string()),
        track_id: Some("w2realA".to_string()),
        mix_levels: None, normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false), 
    };

    let (_blob, _, _pcm_a, _, _, artifacts_a) = run_dsp(
        &req_a,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w2-real-A".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_dir_a.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for noduck");

    fs::copy(artifacts_a.persisted_master.unwrap(), noduck_dest).unwrap();
    println!("Written noduck WAV: {}", noduck_dest);

    if let Some((l, r)) = artifacts_a.pre_master_guards {
        fs::copy(l.path(), "/tmp/w2_real_noduck_pre_l.f32").unwrap();
        fs::copy(r.path(), "/tmp/w2_real_noduck_pre_r.f32").unwrap();
        println!("Written noduck pre-master L: /tmp/w2_real_noduck_pre_l.f32");
    }

    // RUN B: Ducked
    let state_tmp_b = tempfile::TempDir::new().unwrap();
    let out_dir_b = tempfile::TempDir::new().unwrap();
    let req_b = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w2_real".to_string()),
        track_id: Some("w2realB".to_string()),
        mix_levels: None, normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true), 
    };

    let (_blob, _, _pcm_b, _, _, artifacts_b) = run_dsp(
        &req_b,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w2-real-B".to_string(),
        state_tmp_b.path().to_str().unwrap(),
        out_dir_b.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for ducked");

    fs::copy(artifacts_b.persisted_master.unwrap(), ducked_dest).unwrap();
    println!("Written ducked WAV: {}", ducked_dest);

    if let Some((l, r)) = artifacts_b.pre_master_guards {
        fs::copy(l.path(), "/tmp/w2_real_ducked_pre_l.f32").unwrap();
        fs::copy(r.path(), "/tmp/w2_real_ducked_pre_r.f32").unwrap();
        println!("Written ducked pre-master L: /tmp/w2_real_ducked_pre_l.f32");
    }

    assert!(generated_csv.exists(), "CSV not found!");
    fs::copy(&generated_csv, csv_dest).unwrap();
    let csv_content = fs::read_to_string(csv_dest).unwrap();
    println!("CSV Lines: {}", csv_content.lines().count());
    println!("Written CSV: {}", csv_dest);
}

#[test]
#[ignore]
fn w2_duck_gate_synth_variance() {
    let path = fixture_path("duck_splice_synth_snr-15.flac");
    let mut req_a = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
        project_id: Some("w2_synth_var".to_string()),
        track_id: Some("w2synthVAR".to_string()),
        mix_levels: None, normalizer_ceiling_db: None, preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_a) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w2-synth-VAR".to_string(), state_tmp_a.path().to_str().unwrap(), out_dir_a.path().to_str().unwrap()).unwrap();
    fs::copy(artifacts_a.pre_master_guards.unwrap().0.path(), "/tmp/w2_synth_noduck_VAR_pre_l.f32").unwrap();

    req_a.track_id = Some("w2synthVAR2".to_string());
    let state_tmp_c = tempfile::TempDir::new().unwrap();
    let out_dir_c = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_c) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w2-synth-VAR2".to_string(), state_tmp_c.path().to_str().unwrap(), out_dir_c.path().to_str().unwrap()).unwrap();
    fs::copy(artifacts_c.pre_master_guards.unwrap().0.path(), "/tmp/w2_synth_noduck_VAR2_pre_l.f32").unwrap();
}

#[test]
#[ignore]
fn w3b_mix_levels_gate() {
    use m0d::handlers::master::MixLevels;

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav");
    assert!(path.exists(), "Missing fixture: {}", path.display());

    // RUN A: mix_levels: None
    let mut req_a = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
        project_id: Some("w3b".to_string()),
        track_id: Some("w3b_none".to_string()),
        mix_levels: None, normalizer_ceiling_db: None, preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };

    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let (_, _, pcm_a, _, _, _) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w3b-A".to_string(), state_tmp_a.path().to_str().unwrap(), out_dir_a.path().to_str().unwrap()).unwrap();
    
    let bytes_a = fs::read(pcm_a.path()).unwrap();
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&bytes_a);
    let hash_a = format!("{:x}", hasher.finalize());
    println!("SHA (None): {}", hash_a);
    assert_eq!(hash_a, "7df8c9ec66bdf767926054cf5d4805dc7d55ec4a030ff671cc01823f620f94e2");

    fs::copy(pcm_a.path(), "/tmp/w3b_mix_none.wav").unwrap();

    // RUN B: mix_levels: Some
    req_a.track_id = Some("w3b_some".to_string());
    req_a.mix_levels = Some(MixLevels {
        voice: 0.5,
        drums: 1.0,
        bass: 1.0,
        harmonics: 1.0,
        ambience: 1.0,
    });

    let state_tmp_b = tempfile::TempDir::new().unwrap();
    let out_dir_b = tempfile::TempDir::new().unwrap();
    let (_, _, pcm_b, _, _, _) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w3b-B".to_string(), state_tmp_b.path().to_str().unwrap(), out_dir_b.path().to_str().unwrap()).unwrap();
    
    let bytes_b = fs::read(pcm_b.path()).unwrap();
    let mut hasher2 = Sha256::new();
    hasher2.update(&bytes_b);
    let hash_b = format!("{:x}", hasher2.finalize());
    println!("SHA (Some): {}", hash_b);
    assert_ne!(hash_b, hash_a, "Mix levels did not change output!");

    fs::copy(pcm_b.path(), "/tmp/w3b_mix_some.wav").unwrap();
}

#[test]
#[ignore]
fn w4_ceiling_gate() {
    use sha2::{Digest, Sha256};
    use std::fs;
    use m0d::handlers::master::MixLevels;

    let path = fixture_path("duck_splice_synth_snr-15.flac");
    assert!(path.exists(), "Missing fixture: {}", path.display());

    // RUN A: Baseline (no custom mix, no custom ceiling)
    let req_a = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
        project_id: Some("w4".to_string()),
        track_id: Some("w4_baseline".to_string()),
        mix_levels: None, normalizer_ceiling_db: None, preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };

    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let (_, _, pcm_a, _, _, _) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w4-A".to_string(), state_tmp_a.path().to_str().unwrap(), out_dir_a.path().to_str().unwrap()).unwrap();

    let bytes_a = fs::read(pcm_a.path()).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(&bytes_a);
    let hash_a = format!("{:x}", hasher.finalize());
    println!("SHA (None/None): {}", hash_a);
    
    // Helper to compute RMS
    let compute_rms = |bytes: &[u8]| -> f32 {
        let floats: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        let sum_sq: f32 = floats.iter().map(|&x| x * x).sum();
        if floats.is_empty() { 0.0 } else { (sum_sq / floats.len() as f32).sqrt() }
    };
    let rms_a = compute_rms(&bytes_a);
    println!("RMS (A): {}", rms_a);

    // RUN B: extreme_multi, ceiling=None (default 6dB)
    let req_b = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
        project_id: Some("w4".to_string()),
        track_id: Some("w4_undercompensated".to_string()),
        mix_levels: Some(MixLevels {
            voice: 0.1,
            drums: 0.1,
            bass: 0.1,
            harmonics: 0.1,
            ambience: 1.0,
        }),
        normalizer_ceiling_db: None, preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };
    
    let state_tmp_b = tempfile::TempDir::new().unwrap();
    let out_dir_b = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_b) = run_dsp(&req_b, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w4-B".to_string(), state_tmp_b.path().to_str().unwrap(), out_dir_b.path().to_str().unwrap()).unwrap();
    let pre_b = fs::read(artifacts_b.pre_master_guards.as_ref().unwrap().0.path()).unwrap();
    let rms_b = compute_rms(&pre_b);
    println!("RMS (B - extreme_multi, ceil=None, PRE-MASTER): {}", rms_b);

    // RUN C: extreme_multi, ceiling=Some(20.0) [~10x generous]
    let req_c = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
        project_id: Some("w4".to_string()),
        track_id: Some("w4_generous_ceiling".to_string()),
        mix_levels: Some(MixLevels {
            voice: 0.1,
            drums: 0.1,
            bass: 0.1,
            harmonics: 0.1,
            ambience: 1.0,
        }),
        normalizer_ceiling_db: Some(20.0), preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };
    
    let state_tmp_c = tempfile::TempDir::new().unwrap();
    let out_dir_c = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_c) = run_dsp(&req_c, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w4-C".to_string(), state_tmp_c.path().to_str().unwrap(), out_dir_c.path().to_str().unwrap()).unwrap();
    let pre_c = fs::read(artifacts_c.pre_master_guards.as_ref().unwrap().0.path()).unwrap();
    let rms_c = compute_rms(&pre_c);
    println!("RMS (C - extreme_multi, ceil=20.0, PRE-MASTER): {}", rms_c);

    println!("Sanity: rms_a={:.6} (should ≈ true original_rms of unmixed input, verify independently if this gate fails)", rms_a);
    assert!(rms_c > rms_b, "With higher ceiling, PRE-MASTER output RMS should be higher because the gain is not capped at 2.0x");
}

#[test]
#[ignore]
fn w4_ceiling_sweep() {
    use std::path::Path;
    use m0d::handlers::master::MixLevels;
    
    let fixtures = vec![
        ("duck_splice", fixture_path("duck_splice_synth_snr-15.flac")),
        ("bodleasons", Path::new(env!("CARGO_MANIFEST_DIR")).join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")),
    ];

    let profiles = vec![
        ("extreme_multi", Some(MixLevels { voice: 0.1, drums: 0.1, bass: 0.1, harmonics: 0.1, ambience: 1.0 })),
    ];

    for (fix_name, fix_path) in &fixtures {
        assert!(fix_path.exists(), "Missing fixture: {}", fix_path.display());

        for (prof_name, mix_levels) in &profiles {
            let req = MasterRequest {
                audio_path: fix_path.to_str().unwrap().to_string(),
                preset_id: "Transparent".to_string(),
                flavour_id: None, intent_tone: None, intent_dynamics: None, persona_id: None, tone: None, dynamics: None, chaos_seed: None,
                project_id: Some(format!("w4_swp_{}", fix_name)),
                track_id: Some(format!("{}_{}", fix_name, prof_name)),
                mix_levels: mix_levels.clone(),
                normalizer_ceiling_db: None,
                preview_id: None, restoration_enabled: None, macro_router_enabled: None,
                vad_observe_enabled: Some(false),
            };

            let state_tmp = tempfile::TempDir::new().unwrap();
            let out_dir = tempfile::TempDir::new().unwrap();
            
            println!("--- [SWEEP] fixture={} profile={} ---", fix_name, prof_name);
            let _ = run_dsp(&req, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, format!("{}-{}", fix_name, prof_name), state_tmp.path().to_str().unwrap(), out_dir.path().to_str().unwrap()).unwrap();
        }
    }
}
