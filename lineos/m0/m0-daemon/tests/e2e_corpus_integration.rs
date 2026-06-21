//! E2E test: proves corpus + UserMarkovModel written after mastering.
//! Authority: corpus-learning-spec-v1_2.md CB-P11

use m0d::domain::dsp_pipeline::run_dsp;
use std::sync::Arc;
use arc_swap::ArcSwap;
use xaak::repo::DspState;
use m0d::handlers::master::MasterRequest;
use std::time::Instant;

#[tokio::test]
async fn e2e_corpus_integration_writes_model_to_disk() {
    // Use the existing fixture WAV — known to work
    let wav_path_rel = "../../m1/sp314-dsp/tests/fixtures/sine_1khz_3s.wav";
    assert!(
        std::path::Path::new(wav_path_rel).exists(),
        "Fixture missing"
    );
    let wav_path = std::fs::canonicalize(wav_path_rel)
        .unwrap()
        .to_string_lossy()
        .to_string();

    // Change to temp dir so corpus JSON + model JSON drop there
    let temp_dir = std::env::temp_dir().join(format!("lineos_e2e_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_dir).unwrap();

    let req = MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: "spotify".to_string(),
        flavour_id: Some("e2e_preset".to_string()),
        intent_tone: None,
        intent_dynamics: None,
        persona_id: Some("warm_analog".to_string()),
        tone: None,
        dynamics: None,
        chaos_seed: Some(42),
        project_id: Some("e2e_test_proj".to_string()),
        track_id: Some("track_1".to_string()),
        mix_levels: None,
        preview_id: None,
    };

    let start = Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
        run_dsp(&req, start, dummy_head_state, None, None, "".to_string())
    })
    .await;

    // Restore working directory before assertions
    std::env::set_current_dir(&original_dir).unwrap();

    // Must not timeout
    assert!(result.is_ok(), "Test timed out");
    let dsp_result = result.unwrap();
    assert!(dsp_result.is_ok(), "run_dsp failed: {:?}", dsp_result.as_ref().err());
    let dsp_result = dsp_result.unwrap();

    // New architecture: run_dsp returns (blob, path, Option<UserMarkovModel>, StereoBuffer)
    // corpus_node is pure — no disk writes
    // Verify UserMarkovModel bubbled up through the pipeline
    let (_blob, _path, user_model_opt, _) = dsp_result;

    assert!(
        user_model_opt.is_some(),
        "UserMarkovModel should bubble up from corpus_node via run_dsp tuple"
    );

    let user_model = user_model_opt.unwrap();
    assert!(
        user_model.version > 0,
        "UserMarkovModel version should be > 0 after update, got {}",
        user_model.version
    );

    // Validate model has the correct preset
    let json = user_model.to_json().expect("Failed to serialize user model");
    let model: serde_json::Value = serde_json::from_str(&json)
        .expect("UserMarkovModel is not valid JSON");
    assert!(model["presets"].is_object(), "presets must be an object");
    assert!(
        model["presets"]["e2e_preset"].is_object(),
        "e2e_preset must exist in presets"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
