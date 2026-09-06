use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::path::Path;
use std::sync::Arc;
use xaak::repo::DspState;

#[test]
#[ignore = "θέλει /tmp/w9/podcast_realistic.wav (F-072, untracked). ΑΓΝΩΣΤΟΣ ΤΡΟΠΟΣ ΑΝΑΚΑΤΑΣΚΕΥΗΣ — συνταγή ΜΟΝΟ περιγραφική στο 52e2a31, το rebuild δίνει ΑΛΛΟ αρχείο. Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn w6c_nmfd_cost() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    assert!(input_path.exists(), "Missing fixture: {} — συνταγή ΜΟΝΟ περιγραφική στο 52e2a31, πηγές ακαταγράφητες (F-072) — το rebuild δίνει ΑΛΛΟ αρχείο", input_path.display());

    // RUN A: NMF5 (use_nmfd = false)
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
        project_id: Some("w6c_cost".to_string()),
        track_id: Some("w6cA".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(false),
    };

    let t_start_a = std::time::Instant::now();
    let _ = run_dsp(
        &req_a,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w6c-nmf5".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_dir_a.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for NMF5");
    let nmf5_total_ms = t_start_a.elapsed().as_millis();

    // RUN B: NMFD8 (use_nmfd = true)
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
        project_id: Some("w6c_cost".to_string()),
        track_id: Some("w6cB".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(true),
    };

    let t_start_b = std::time::Instant::now();
    let _ = run_dsp(
        &req_b,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "w6c-nmfd8".to_string(),
        state_tmp_b.path().to_str().unwrap(),
        out_dir_b.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for NMFD8");
    let nmfd8_total_ms = t_start_b.elapsed().as_millis();

    let delta_ms = nmfd8_total_ms as i128 - nmf5_total_ms as i128;
    let ratio = nmfd8_total_ms as f64 / nmf5_total_ms.max(1) as f64;
    let realtime_factor_nmf5 = 120000.0 / nmf5_total_ms.max(1) as f64;
    let realtime_factor_nmfd8 = 120000.0 / nmfd8_total_ms.max(1) as f64;

    println!(
        "[W6C] nmf5_total_ms={} nmfd8_total_ms={} delta_ms={} ratio={:.2}x realtime_factor_nmf5={:.2}x realtime_factor_nmfd8={:.2}x",
        nmf5_total_ms,
        nmfd8_total_ms,
        delta_ms,
        ratio,
        realtime_factor_nmf5,
        realtime_factor_nmfd8
    );
}
