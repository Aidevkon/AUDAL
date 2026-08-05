use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

#[tokio::test]
async fn test_e2e_golden_pathway_aether_pipeline() {
    let wav_path = "../../m1/sp314-dsp/tests/fixtures/sine_1khz_3s.wav";

    // We expect the file to exist (checked in Phase A4 step 1)
    assert!(std::path::Path::new(wav_path).exists(), "Fixture missing!");

    let req = MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: "spotify".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: Some("warm_analog".to_string()),
        tone: None,
        dynamics: None,
        chaos_seed: Some(42),
        project_id: Some("proj_1".to_string()),
        track_id: Some("track_1".to_string()),
        mix_levels: None, normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
    };

    let start = Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
        let state_tmp = tempfile::TempDir::new().unwrap();

        run_dsp(
            &req,
            start,
            dummy_head_state,
            None,
            None,
            "".to_string(),
            state_tmp.path().to_str().unwrap(),
            "/tmp",
        )
    })
    .await;

    // Must not timeout
    assert!(result.is_ok(), "Test timed out after 60s");

    let dsp_result = result.unwrap();
    // Must succeed
    assert!(dsp_result.is_ok(), "run_dsp failed: {:?}", dsp_result.err());

    let (blob, _, _, _, _, _artifacts) = dsp_result.unwrap();

    // Verify properties
    assert_eq!(
        blob.schema_version, 2,
        "GoldenBlob must be schema_version 2"
    );

    // Check Aether fields are populated
    assert!(blob.aether_persona.is_some(), "aether_persona must be set");
    assert_eq!(
        blob.aether_persona.unwrap(),
        "warm_analog",
        "Persona must match request"
    );

    assert!(blob.aether_config.is_some(), "aether_config must be set");
    assert!(blob.aether_cert.is_some(), "aether_cert must be generated");

    // Deserialize cert and verify hashes
    let cert_str = blob.aether_cert.unwrap();
    let cert: serde_json::Value =
        serde_json::from_str(&cert_str).expect("Failed to deserialize ExecutionCertificate");

    assert_eq!(
        cert["input_pcm_hash"].as_str().unwrap().len(),
        64,
        "Input hash must be SHA-256 hex"
    );
    assert_eq!(
        cert["output_pcm_hash"].as_str().unwrap().len(),
        64,
        "Output hash must be SHA-256 hex"
    );
    assert_ne!(
        cert["input_pcm_hash"], cert["output_pcm_hash"],
        "Input and output hashes must differ after DSP"
    );
    assert_eq!(cert["persona_id"], "warm_analog");
}
