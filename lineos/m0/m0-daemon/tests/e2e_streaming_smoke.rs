use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::oneshot;
use xaak::repo::DspState;

#[tokio::test]
async fn test_agent_pipeline_executes_streaming() {
    let audit_dir = "/tmp/m0d_test_audit_streaming";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let db = m0d::db::init_test().await.expect("test db");
    let (album_tx, _) = tokio::sync::broadcast::channel(64);
    let (progress_tx, _) = tokio::sync::broadcast::channel(16);
    let progress_map = Arc::new(dashmap::DashMap::new());
    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());

    let (operator, _handles) = m0d::agents::operator::spawn_agents(
        audit.clone(),
        dummy_head_state,
        db,
        m0d::blob_store::BlobStore::new(),
        album_tx,
        progress_tx,
        progress_map,
        config,
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let params = m0d::agents::operator::StreamingParams {
        audio_path: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/test_stereo_input.wav"
        )
        .into(),
        preset_id: "podcast".into(),
        flavour_id: None,
        intent_tone: Some(0.5),
        intent_dynamics: Some(0.5),
        target_lufs_override: None,
        session_id: "streaming_smoke_001".into(),
    };

    let (tx, rx) = oneshot::channel();
    let intent = m0d::agents::operator::Intent::ExecuteStreaming {
        params,
        response: tx,
    };

    operator
        .dispatch(intent)
        .await
        .expect("Operator dispatch failed");

    let result = tokio::time::timeout(tokio::time::Duration::from_secs(300), rx)
        .await
        .expect("Test timeout after 300s")
        .expect("Conductor dropped response channel");

    match result {
        Ok(output) => {
            println!("✅ Streaming complete");
            println!("   job_id: {}", output.job_id);
            println!("   status: {}", output.status);
            println!("   pcm_data: {:?}", output.pcm_data);
            assert_eq!(output.status, "certified", "status must be certified");

            // Check output file
            let output_path = format!("/tmp/m0d-v3-streaming-{}.wav", output.blob_id);
            let meta = std::fs::metadata(&output_path).expect("Output file must exist");
            assert!(meta.len() > 1000, "Output file must be non-empty");
            println!("   File size: {} bytes", meta.len());
        }
        Err(e) => {
            panic!("Streaming failed: {:?}", e);
        }
    }
}
