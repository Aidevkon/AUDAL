//! E2E test: proves corpus + UserMarkovModel written after mastering.
//! Authority: corpus-learning-spec-v1_2.md CB-P11

use m0d::domain::dsp_pipeline::run_dsp;
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
        run_dsp(&req, start)
    })
    .await;

    // Restore working directory before assertions
    std::env::set_current_dir(&original_dir).unwrap();

    // Must not timeout
    assert!(result.is_ok(), "Test timed out");
    let dsp_result = result.unwrap();
    assert!(dsp_result.is_ok(), "run_dsp failed: {:?}", dsp_result.err());

    // Check corpus JSON was written
    let files: Vec<String> = std::fs::read_dir(&temp_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap_or_default())
        .collect();

    let has_corpus = files
        .iter()
        .any(|f| f.starts_with("session_") && f.ends_with(".corpus.json"));
    assert!(
        has_corpus,
        "No session_*.corpus.json found. Files: {:?}",
        files
    );

    // Check UserMarkovModel was written
    let model_file = temp_dir.join("user_model_e2e_test_proj.json");
    assert!(
        model_file.exists(),
        "No user_model_e2e_test_proj.json found. Files: {:?}",
        files
    );

    // Validate model is valid JSON with correct structure
    let model_json = std::fs::read_to_string(&model_file).unwrap();
    let model: serde_json::Value =
        serde_json::from_str(&model_json).expect("UserMarkovModel is not valid JSON");
    assert_eq!(model["user_id"].as_str().unwrap(), "e2e_test_proj");
    assert!(model["presets"].is_object(), "presets must be an object");
    assert!(
        model["presets"]["e2e_preset"].is_object(),
        "e2e_preset must exist in presets"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}
