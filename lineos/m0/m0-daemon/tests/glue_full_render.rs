use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use xaak::repo::DspState;

#[test]
#[ignore = "θέλει /tmp/w9/podcast_realistic.wav (F-072, untracked). ⚠ ΧΩΡΙΣ ΤΟ FIXTURE ΤΥΠΩΝΕΙ «SKIPPED» ΚΑΙ ΠΕΡΝΑΕΙ ΠΡΑΣΙΝΟ ΣΕ 0.00s — πράσινο που δεν μέτρησε τίποτα. Ανακατασκευή: research/musdb-lab (W9). Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn glue_full_render() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tempfile::TempDir::new().unwrap();
    let req = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("glue_podcast".to_string()),
        track_id: Some("glue1".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: None,
    };

    let (blob, _, _pcm, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "glue-podcast-1".to_string(),
        state_tmp.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    fs::create_dir_all("/tmp/glue_full").unwrap();
    if let Some(master_path) = artifacts.persisted_master {
        println!("master_path: {:?}", master_path);
        fs::copy(&master_path, "/tmp/glue_full/render.flac").unwrap();
    } else {
        println!("master_path: None");
    }

    println!("[FINAL NMFD8] integrated_lufs={:.2} true_peak={:.2} \
              too_quiet={} spotify_ok={} apple_pod_ok={}",
        blob.loudness().expect("test expects Certified").integrated_lufs,
        blob.loudness().expect("test expects Certified").true_peak_dbtp,
        blob.loudness().expect("test expects Certified").too_quiet_for_mobile,
        blob.loudness().expect("test expects Certified").spotify_compliant,
        blob.loudness().expect("test expects Certified").apple_podcasts_compliant);

    let req2 = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("glue_podcast".to_string()),
        track_id: Some("glue2".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(false),
    };

    let (blob2, _, _pcm2, _, _, _artifacts2) = run_dsp(
        &req2,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "glue-podcast-2".to_string(),
        state_tmp.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
    )
    .expect("run_dsp failed for NMF5");

    println!("[FINAL NMF5] integrated_lufs={:.2} true_peak={:.2} \
              too_quiet={} spotify_ok={} apple_pod_ok={}",
        blob2.loudness().expect("test expects Certified").integrated_lufs,
        blob2.loudness().expect("test expects Certified").true_peak_dbtp,
        blob2.loudness().expect("test expects Certified").too_quiet_for_mobile,
        blob2.loudness().expect("test expects Certified").spotify_compliant,
        blob2.loudness().expect("test expects Certified").apple_podcasts_compliant);
}
