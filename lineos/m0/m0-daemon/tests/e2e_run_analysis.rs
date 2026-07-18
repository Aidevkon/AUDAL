//! Dedicated coverage for Intent::RunAnalysis — previously only
//! ever exercised indirectly through album cohesion's
//! ExecuteBatchMastering. Confirms the streaming migration
//! (c85f56a, off decode_smart onto measure_input_metrics) produces
//! sane values through the real executor dispatch path, and locks
//! in the current, deliberately deferred bpm=0.0 state as an
//! explicit assertion — if a future streaming BeatDetector lands
//! and this starts failing, that's the correct signal to update
//! this test, not a regression.

use arc_swap::ArcSwap;
use m0d::agents::operator::{AnalysisResult, ExecutorError, Intent};
use std::sync::Arc;
use xaak::repo::DspState;

fn write_test_wav(path: &str, sample_rate: u32, seconds: f32) {
    let n = (sample_rate as f32 * seconds) as usize;
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    let bpm_target = 120.0f32;
    let interval_samples = (60.0 / bpm_target * sample_rate as f32) as usize;
    let mut samples = vec![0.0f32; n];
    let mut pos = 0;
    while pos + 20 < n {
        for i in 0..20 {
            samples[pos + i] = 0.9 * (1.0 - i as f32 / 20.0);
        }
        pos += interval_samples;
    }
    for &s in &samples {
        writer.write_sample(s).unwrap();
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

#[tokio::test]
async fn run_analysis_produces_sane_streaming_metrics() {
    // 5 real seconds @ 48k = ~58 chunks at the 4096-frame boundary
    // used internally by measure_input_metrics — long enough to
    // actually exercise the streaming accumulator across multiple
    // reads, not just a single-chunk pass.
    let wav_path = "/tmp/test_run_analysis_long.wav";
    write_test_wav(wav_path, 48_000, 5.0);

    let audit_dir = "/tmp/m0d_test_audit_run_analysis";
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

    let (tx, rx) = tokio::sync::oneshot::channel();
    let intent = Intent::RunAnalysis {
        audio_path: wav_path.to_string(),
        session_id: "test_analysis_001".into(),
        response: tx,
    };
    operator.dispatch(intent).await.unwrap();

    let result: Result<AnalysisResult, ExecutorError> =
        tokio::time::timeout(std::time::Duration::from_secs(10), rx)
            .await
            .expect("RunAnalysis timed out")
            .expect("response channel closed unexpectedly");

    let analysis = result.expect("RunAnalysis should succeed on valid audio");

    assert_eq!(analysis.session_id, "test_analysis_001");
    assert!(
        analysis.integrated_lufs.is_finite() && analysis.integrated_lufs < 0.0,
        "expected a plausible negative LUFS, got {}",
        analysis.integrated_lufs
    );
    assert!(
        analysis.true_peak_dbtp.is_finite() && analysis.true_peak_dbtp < 0.0,
        "expected a plausible negative true peak, got {}",
        analysis.true_peak_dbtp
    );
    // The streaming BeatDetector work landed (7e32e32, input_lufs.rs
    // wiring) — this now asserts a REAL, correct value instead of
    // documenting a deferred 0.0.
    assert!(
        (analysis.bpm - 120.0).abs() < 1.0,
        "expected ~120bpm from the click-track fixture, got {}",
        analysis.bpm
    );

    std::fs::remove_file(wav_path).ok();
}
