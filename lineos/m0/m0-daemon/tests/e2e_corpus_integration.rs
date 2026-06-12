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
        run_dsp(&req, start, dummy_head_state)
    })
    .await;

    // Restore working directory before assertions
    std::env::set_current_dir(&original_dir).unwrap();

    // Must not timeout
    assert!(result.is_ok(), "Test timed out");
    let dsp_result = result.unwrap();
    assert!(dsp_result.is_ok(), "run_dsp failed: {:?}", dsp_result.err());

    // New architecture: corpus_node is pure computation (no session files)
    // UserMarkovModel persisted to ~/.creator_os/state/ by executor
    // Verify run_dsp completed successfully (corpus ran internally)
    let state_dir = format!(
        "{}/.creator_os/state",
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    );
    let model_path = format!("{}/user_model_e2e_test_proj.json", state_dir);

    // Give executor async persist a moment to complete
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Verify model was persisted to state dir
    assert!(
        std::path::Path::new(&model_path).exists(),
        "UserMarkovModel not found at {}. Architecture: corpus_node is pure, executor persists.",
        model_path
    );

    // Validate model is valid JSON
    let model_json = std::fs::read_to_string(&model_path).unwrap();
    let model: serde_json::Value =
        serde_json::from_str(&model_json).expect("UserMarkovModel is not valid JSON");
    assert!(model["presets"].is_object(), "presets must be an object");
    assert!(
        model["presets"]["e2e_preset"].is_object(),
        "e2e_preset must exist in presets"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
