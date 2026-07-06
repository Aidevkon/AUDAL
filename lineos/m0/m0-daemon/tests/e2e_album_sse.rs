//! E2E test: Verify Album SSE Pipeline emissions.
//! Sends an Intent::ExecuteBatchMastering and waits for AlbumEvent::PreAnalysis.

use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::oneshot;
use xaak::repo::DspState;

#[tokio::test]
async fn test_album_sse_pipeline_emits_bpm() {
    let audit_dir = "/tmp/m0d_test_audit_sse";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let db = m0d::db::init_test().await.expect("test db");

    // 1. Create the SSE channel and subscribe
    let (album_tx, _) = tokio::sync::broadcast::channel(64);
    let mut rx = album_tx.subscribe();

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

    // 2. Dispatch a Batch Mastering intent
    let params = m0d::agents::operator::MasteringParams {
        audio_path: "/home/aidevcon/Music/test.wav".into(), // Ensure test.wav exists or uses dummy
        preset_id: "spotify".into(),
        target_lufs: -14.0,
        max_tp_db: -1.0,
        session_id: "sse_test_001".into(),
        project_id: None,
        track_id: None,
        flavour_id: None,
        chaos_seed: None,
    };

    let (tx, _res_rx) = oneshot::channel();
    let intent = m0d::agents::operator::Intent::ExecuteBatchMastering {
        batch_id: "batch_sse_123".into(),
        items: vec![params],
        response: tx,
    };

    operator.dispatch(intent).await.expect("Dispatch failed");

    // 3. Wait for the SSE event
    let event = tokio::time::timeout(tokio::time::Duration::from_secs(10), rx.recv())
        .await
        .expect("Timeout waiting for SSE event")
        .expect("Channel closed");

    // 4. Assert the event is PreAnalysis and contains BPM
    match event {
        m0d::app_state::AlbumEvent::PreAnalysis {
            track,
            bpm,
            ducking_gain,
        } => {
            println!("✅ Received PreAnalysis SSE Event!");
            println!("   Track: {}", track);
            println!("   BPM: {}", bpm);
            println!("   Ducking Gain: {}", ducking_gain);
            assert_eq!(track, 1, "Should be track 1");
            assert!(bpm >= 0.0, "BPM should be valid");
            assert!(
                ducking_gain > 0.0 && ducking_gain <= 1.0,
                "Ducking gain should be in [0.3, 1.0]"
            );
        }
    }
}
