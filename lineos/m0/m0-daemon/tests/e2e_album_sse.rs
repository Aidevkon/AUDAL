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
        audio_path: concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/test_stereo_input.wav"
        )
        .into(), // Uses committed fixture, always available on CI
        preset_id: "spotify".into(),
        target_lufs: -14.0,
        max_tp_db: -1.0,
        session_id: "sse_test_001".into(),
        project_id: None,
        track_id: None,
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        chaos_seed: None,
        mix_levels: None,
    };

    let (tx, _res_rx) = oneshot::channel();
    let intent = m0d::agents::operator::Intent::ExecuteBatchMastering {
        batch_id: "batch_sse_123".into(),
        items: vec![params],
        response: tx,
    };

    operator.dispatch(intent).await.expect("Dispatch failed");

    let mut seen_pre_analysis = false;
    let mut seen_forensic = false;
    let mut seen_cohesion = false;
    let mut seen_fatigue = false;

    // Collect events until we've seen all 4 kinds or time out —
    // proves Forensic/Cohesion/Fatigue actually fire with sane data,
    // not just that PreAnalysis (already proven) still works.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    while tokio::time::Instant::now() < deadline
        && !(seen_pre_analysis && seen_forensic && seen_cohesion && seen_fatigue)
    {
        let event =
            match tokio::time::timeout(tokio::time::Duration::from_secs(10), rx.recv()).await {
                Ok(Ok(e)) => e,
                _ => break,
            };

        match event {
            m0d::app_state::AlbumEvent::PreAnalysis {
                track,
                bpm,
                ducking_gain,
            } => {
                println!("✅ PreAnalysis: track={track} bpm={bpm} ducking_gain={ducking_gain}");
                assert_eq!(track, 1);
                assert!(bpm >= 0.0);
                assert!(ducking_gain > 0.0 && ducking_gain <= 1.0);
                seen_pre_analysis = true;
            }
            m0d::app_state::AlbumEvent::Forensic { track, lufs } => {
                println!("✅ Forensic: track={track} lufs={lufs}");
                assert_eq!(track, 1);
                assert!(lufs.is_finite(), "lufs should be a real measured value");
                seen_forensic = true;
            }
            m0d::app_state::AlbumEvent::Cohesion { per_track_targets } => {
                println!("✅ Cohesion: per_track_targets={per_track_targets:?}");
                assert!(
                    !per_track_targets.is_empty(),
                    "cohesion targets should be non-empty for a real batch"
                );
                seen_cohesion = true;
            }
            m0d::app_state::AlbumEvent::Fatigue {
                track,
                ducking,
                width,
            } => {
                println!("✅ Fatigue: track={track} ducking={ducking} width={width}");
                assert_eq!(track, 1);
                assert!(ducking > 0.0);
                assert!(width > 0.0);
                seen_fatigue = true;
            }
        }
    }

    assert!(seen_pre_analysis, "never received a PreAnalysis event");
    assert!(seen_forensic, "never received a Forensic event");
    assert!(seen_cohesion, "never received a Cohesion event");
    assert!(seen_fatigue, "never received a Fatigue event");
}
