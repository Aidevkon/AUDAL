//! A/B render fixture — renders bodleasons_mid.wav through the FULL DSP pipeline
//! (run_dsp -> render_node -> FiveDotOneStage -> StereoRenderer) and outputs
//! the true stereo master.
//!
//! This test is #[ignore] — it's invoked explicitly by render_variants.sh.

use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::fs;
use std::sync::Arc;
use xaak::repo::DspState;

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")
}

#[test]
#[ignore]
fn render_ab_full_pipeline() {
    let path = fixture_path();
    assert!(path.exists(), "Missing fixture: {}", path.display());

    let req = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(), // Transparent preset to engage full stereo pipeline without aggressive coloration
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("ab_render".to_string()),
        track_id: Some("bodleasons".to_string()),
        mix_levels: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
    };

    let state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = "/tmp/ab_render_out";
    std::fs::create_dir_all(out_dir).unwrap();

    let (_blob, _, pcm, _, _, _) = run_dsp(
        &req,
        std::time::Instant::now(),
        state,
        None,
        None,
        "ab-render".to_string(),
        state_tmp.path().to_str().unwrap(),
        out_dir,
    )
    .expect("run_dsp failed");

    let final_dest = "/tmp/ab_variant_B_spectral.wav";
    fs::copy(pcm.path(), final_dest).unwrap();
    println!("Written: {}", final_dest);
}
