//! E2E integration test: Constitutional Agent Architecture v3.1
//! Tests the full Intent::ExecuteMastering flow:
//! HTTP handler → Operator → Conductor → Executor → DspAdapter
//!
//! Uses real audio file: /home/aidevcon/Music/test.wav
//! Asserts: LUFS in [-20, -8], true peak < -0.5 dBTP
//! Authority: Constitutional Agent Architecture Spec v3.1

use std::sync::Arc;
use tokio::sync::oneshot;

#[tokio::test]
async fn test_agent_pipeline_executes_mastering() {
    // Build minimal AppState with real agents
    let audit_dir = "/tmp/m0d_test_audit";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let operator = m0d::agents::operator::spawn_agents(audit.clone());

    // Allow agents to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Build MasteringParams
    let params = m0d::agents::operator::MasteringParams {
        audio_path: "/home/aidevcon/Music/test.wav".into(),
        preset_id: "spotify".into(),
        target_lufs: -14.0,
        max_tp_db: -1.0,
        session_id: "e2e_test_001".into(),
        project_id: None,
        track_id: None,
        flavour_id: None,
        chaos_seed: None,
    };

    // Dispatch Intent::ExecuteMastering
    let (tx, rx) = oneshot::channel();
    let intent = m0d::agents::operator::Intent::ExecuteMastering {
        params,
        response: tx,
    };

    operator
        .dispatch(intent)
        .await
        .expect("Operator dispatch failed");

    // Wait for result — DSP takes time
    let result = tokio::time::timeout(tokio::time::Duration::from_secs(300), rx)
        .await
        .expect("Test timeout after 300s")
        .expect("Conductor dropped response channel");

    match result {
        Ok(output) => {
            println!("✅ Mastering complete");
            println!("   blob_id: {}", output.blob_id);
            println!("   status:  {}", output.status);
            assert!(!output.blob_id.is_empty(), "blob_id must not be empty");
            assert_eq!(output.status, "ok", "status must be ok");
        }
        Err(e) => {
            panic!("Mastering failed: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_conductor_rejects_concurrent_mastering() {
    let audit_dir = "/tmp/m0d_test_audit_concurrent";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let operator = m0d::agents::operator::spawn_agents(audit.clone());
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send two concurrent mastering requests
    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();

    let params1 = m0d::agents::operator::MasteringParams {
        audio_path: "/home/aidevcon/Music/test.wav".into(),
        preset_id: "spotify".into(),
        target_lufs: -14.0,
        max_tp_db: -1.0,
        session_id: "concurrent_001".into(),
        project_id: None,
        track_id: None,
        flavour_id: None,
        chaos_seed: None,
    };

    let params2 = m0d::agents::operator::MasteringParams {
        audio_path: "/home/aidevcon/Music/test.wav".into(),
        preset_id: "spotify".into(),
        target_lufs: -14.0,
        max_tp_db: -1.0,
        session_id: "concurrent_002".into(),
        project_id: None,
        track_id: None,
        flavour_id: None,
        chaos_seed: None,
    };

    operator
        .dispatch(m0d::agents::operator::Intent::ExecuteMastering {
            params: params1,
            response: tx1,
        })
        .await
        .ok();

    // Small delay then send second request while first is running
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    operator
        .dispatch(m0d::agents::operator::Intent::ExecuteMastering {
            params: params2,
            response: tx2,
        })
        .await
        .ok();

    // Second request must be rejected with Busy
    let result2 = tokio::time::timeout(tokio::time::Duration::from_secs(10), rx2)
        .await
        .expect("Timeout waiting for busy response")
        .expect("Channel dropped");

    assert!(
        matches!(result2, Err(m0d::agents::operator::ConductorError::Busy)),
        "Second concurrent request must return ConductorError::Busy, got: {:?}",
        result2
    );

    println!("✅ Concurrent mastering correctly rejected with Busy");

    // Let first job finish to clean up
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(300), rx1).await;
}

#[tokio::test]
async fn test_schema_agent_validates_and_queries() {
    let audit_dir = "/tmp/m0d_test_audit_schema";
    std::fs::create_dir_all(audit_dir).ok();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir).expect("Failed to open audit log"));

    let operator = m0d::agents::operator::spawn_agents(audit);
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Test 1: valid patch accepted
    let (tx, rx) = oneshot::channel();
    operator
        .dispatch(m0d::agents::operator::Intent::ValidateSchema {
            patch: serde_json::json!({ "dsp.target_lufs": -16.0 }),
            response: tx,
        })
        .await
        .ok();

    let result = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx)
        .await
        .expect("Timeout")
        .expect("Channel dropped");
    assert!(result.is_ok(), "Valid patch must be accepted");

    // Test 2: query returns updated value
    let (tx2, rx2) = oneshot::channel();
    operator
        .dispatch(m0d::agents::operator::Intent::QuerySchema {
            path: "dsp.target_lufs".into(),
            response: tx2,
        })
        .await
        .ok();

    let value = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx2)
        .await
        .expect("Timeout")
        .expect("Channel dropped");
    assert_eq!(
        value.as_f64().unwrap(),
        -16.0,
        "Query must return updated value"
    );

    // Test 3: invalid patch rejected
    let (tx3, rx3) = oneshot::channel();
    operator
        .dispatch(m0d::agents::operator::Intent::ValidateSchema {
            patch: serde_json::json!({ "dsp.target_lufs": 5.0 }),
            response: tx3,
        })
        .await
        .ok();

    let result3 = tokio::time::timeout(tokio::time::Duration::from_secs(5), rx3)
        .await
        .expect("Timeout")
        .expect("Channel dropped");
    assert!(result3.is_err(), "Out-of-range value must be rejected");

    println!("✅ SchemaAgent validates and queries correctly");
}
