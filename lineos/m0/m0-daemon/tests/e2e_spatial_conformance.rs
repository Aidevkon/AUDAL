use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

/// End-to-end test: synthetic 5.1 WAV →
/// spatial_conformance_path →
/// StoredBlob (channels=6, blob_type=spatial_bed)
///
/// Validates the full pipeline path:
/// decode_smart → FiveDotOne arm in decode_node
/// → spatial_conformance_path in dsp_pipeline
/// → StoredBlob with correct metadata
#[test]
fn e2e_5dot1_wav_produces_spatial_blob() {
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_5dot1_input.wav"
    );

    if !std::path::Path::new(input_path).exists() {
        panic!(
            "Test fixture missing: {}\n\
             Generate with python script",
            input_path
        );
    }

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "apple_spatial_bed".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-session-spatial-001".to_string()),
        mix_levels: None,
        preview_id: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None, // progress_tx
        None, // progress_map
        "job-spatial-test".to_string(),
        state_tmp.path().to_str().unwrap(),
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (blob, _spatial, _path, _model, _) = result.unwrap();

    assert_eq!(blob.channels, 6, "Expected 6 channels in spatial blob");
    assert_eq!(
        blob.blob_type, "spatial_bed",
        "Expected blob_type = spatial_bed"
    );
    assert!(blob.sample_rate == 48000, "Expected 48000 Hz sample rate");

    let pcm_path = format!("/tmp/m0d-raw-{}.pcm", blob.id);
    let pcm_bytes = std::fs::read(&pcm_path).expect("PCM dump should exist");
    assert_eq!(pcm_bytes.len(), 48000 * 6 * 4, "PCM dump size mismatch");

    println!(
        "Spatial blob: id={} channels={} sample_rate={} blob_type={}",
        blob.id, blob.channels, blob.sample_rate, blob.blob_type
    );
}
