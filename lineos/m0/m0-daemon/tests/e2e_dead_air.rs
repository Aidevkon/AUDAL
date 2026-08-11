use arc_swap::ArcSwap;
use std::sync::Arc;
use xaak::repo::DspState;

#[tokio::test]
async fn dead_air_reaches_the_real_certificate() {
    // 6 seconds total: 1s of tone, 4s of true silence (well past the
    // 3s DEAD_AIR_MIN_SECS threshold, comfortably below the
    // -60dBFS DEAD_AIR_WINDOW_DBFS gate), 1s of tone again.
    // This MUST trigger at least one DeadAirEvent.
    let wav_path = "/tmp/test_dead_air.wav";
    let sr = 48_000u32;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    let tone_secs = 1.0;
    let silence_secs = 4.0;
    let tone_frames = (sr as f32 * tone_secs) as usize;
    let silence_frames = (sr as f32 * silence_secs) as usize;

    for i in 0..tone_frames {
        let v = 0.4 * (i as f32 * 0.05).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v).unwrap();
    }
    for _ in 0..silence_frames {
        writer.write_sample(0.0f32).unwrap();
        writer.write_sample(0.0f32).unwrap();
    }
    for i in 0..tone_frames {
        let v = 0.4 * (i as f32 * 0.05).sin();
        writer.write_sample(v).unwrap();
        writer.write_sample(v).unwrap();
    }
    writer.finalize().unwrap();

    let audit_dir = "/tmp/m0d_test_audit_dead_air";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let db = m0d::db::init_test().await.expect("test db");
    let (album_tx, _) = tokio::sync::broadcast::channel(64);
    let (progress_tx, _) = tokio::sync::broadcast::channel(16);
    let progress_map = Arc::new(dashmap::DashMap::new());
    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());
    let blob_store = m0d::blob_store::BlobStore::new();

    let (operator, _handles) = m0d::agents::operator::spawn_agents(
        audit.clone(),
        dummy_head_state,
        db,
        blob_store.clone(),
        album_tx,
        progress_tx,
        progress_map,
        config,
    );

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let (tx, rx) = tokio::sync::oneshot::channel();
    let params = m0d::agents::operator::StreamingParams {
        audio_path: wav_path.into(),
        preset_id: "podcast".into(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "test_dead_air_001".into(),
    };
    let intent = m0d::agents::operator::Intent::ExecuteStreaming {
        params,
        response: tx,
    };
    operator.dispatch(intent).await.unwrap();

    let result = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .expect("Test timeout after 30s")
        .expect("response channel closed");

    let output = result.expect("ExecuteStreaming should succeed");

    // Fetch the real StoredBlob from the blob store using the
    // returned blob_id — confirm the exact accessor method on
    // blob_store (get/fetch/find — check its real API) rather than
    // guessing.
    let blob = blob_store
        .get(&output.blob_id)
        .expect("blob must exist in store");

    assert!(
        blob.dead_air().expect("test expects Certified").total_count > 0,
        "expected at least one dead-air event for 4s of silence, got 0"
    );
    assert!(
        blob.dead_air().expect("test expects Certified").total_sec >= 3.5,
        "expected the detected dead air to cover most of the 4s silent \
         region, got {}s",
        blob.dead_air().expect("test expects Certified").total_sec
    );

    assert!(
        !blob.timeline_or_empty().is_empty(),
        "expected real timeline stages, got an empty vec"
    );
    assert_eq!(
        blob.timeline_or_empty().len(),
        6,
        "expected exactly 6 stages (Scout/PreAnalysis, Trunk Pass, Decode Setup, \
         Streaming Render, Verification Pass, Certificate Assembly), \
         got {}",
        blob.timeline_or_empty().len()
    );
    // The Verification Pass stage should carry a real hash (the
    // only stage wired with one today) — proves stage-specific data
    // isn't just placeholder-uniform.
    let verification_stage = blob
        .timeline_or_empty()
        .iter()
        .find(|s| s.stage == "Verification Pass")
        .expect("Verification Pass stage must exist");
    assert!(
        !verification_stage.stage_hash.is_empty(),
        "Verification Pass should carry a real pcm_blake3 hash, got empty"
    );
    for stage in blob.timeline_or_empty() {
        println!(
            "  stage={} duration_ms={} hash_len={}",
            stage.stage,
            stage.duration_ms,
            stage.stage_hash.len()
        );
    }

    std::fs::remove_file(wav_path).ok();
}
