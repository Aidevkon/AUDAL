use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

#[test]
fn e2e_stereo_input_spatial_upmix_produces_both_blobs() {
    // Fixture: stereo 48kHz 2s sine
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_stereo_input.wav"
    );
    assert!(
        std::path::Path::new(input_path).exists(),
        "Stereo fixture missing"
    );

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "spatial_upmix".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-session-decoupled-001".to_string()),
        mix_levels: None,
        preview_id: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None, // progress_tx
        None, // progress_map
        "job-decoupled-test".to_string(),
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (stereo_blob, spatial_blob_opt, _path, _model) = result.unwrap();

    // Fork A — stereo πάντα
    assert_eq!(
        stereo_blob.channels, 2,
        "Stereo blob should have 2 channels"
    );

    // Fork B — spatial ενεργό λόγω preset
    assert!(
        spatial_blob_opt.is_some(),
        "spatial_upmix preset must produce a spatial blob (Fork B)"
    );
    let spatial_blob = spatial_blob_opt.unwrap();
    assert_eq!(
        spatial_blob.channels, 6,
        "Spatial blob should have 6 channels"
    );
    assert_eq!(
        spatial_blob.blob_type, "spatial_bed",
        "Spatial blob_type mismatch"
    );

    // Verify spatial blob id convention
    assert!(
        spatial_blob.id.ends_with("-spatial"),
        "Spatial blob id should end -spatial, got: {}",
        spatial_blob.id
    );

    println!(
        "Decoupled Fork OK:\n  stereo: {} ({}ch)\n  spatial: {} ({}ch, {})",
        stereo_blob.id,
        stereo_blob.channels,
        spatial_blob.id,
        spatial_blob.channels,
        spatial_blob.blob_type
    );
}

#[test]
fn e2e_stereo_master_preset_no_spatial_blob() {
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_stereo_input.wav"
    );

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "stereo_master".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-session-decoupled-002".to_string()),
        mix_levels: None,
        preview_id: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None, // progress_tx
        None, // progress_map
        "job-decoupled-test-no-spatial".to_string(),
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (_stereo, spatial_opt, _, _) = result.unwrap();

    assert!(
        spatial_opt.is_none(),
        "stereo_master preset must NOT produce a spatial blob"
    );
}
