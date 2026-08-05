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
        mix_levels: None,
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
        mix_levels: None,
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
        mix_levels: None,
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
        mix_levels: None,
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
        project_id: Some("w2_synth".to_string()),
        track_id: Some("w2synthA".to_string()),
        mix_levels: None, preview_id: None, restoration_enabled: None, macro_router_enabled: None,
        vad_observe_enabled: Some(false),
    };
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_dir_a = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_a) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w2-synth-A".to_string(), state_tmp_a.path().to_str().unwrap(), out_dir_a.path().to_str().unwrap()).unwrap();
    fs::copy(artifacts_a.pre_master_guards.unwrap().0.path(), "/tmp/w2_synth_noduck_A_pre_l.f32").unwrap();

    req_a.track_id = Some("w2synthC".to_string());
    let state_tmp_c = tempfile::TempDir::new().unwrap();
    let out_dir_c = tempfile::TempDir::new().unwrap();
    let (_, _, _, _, _, artifacts_c) = run_dsp(&req_a, std::time::Instant::now(), Arc::new(ArcSwap::from_pointee(DspState::default())), None, None, "w2-synth-C".to_string(), state_tmp_c.path().to_str().unwrap(), out_dir_c.path().to_str().unwrap()).unwrap();
    fs::copy(artifacts_c.pre_master_guards.unwrap().0.path(), "/tmp/w2_synth_noduck_C_pre_l.f32").unwrap();
}
