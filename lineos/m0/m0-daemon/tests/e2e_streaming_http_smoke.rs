use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
async fn test_streaming_http_smoke() {
    let audit_dir = TempDir::new().unwrap();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap());

    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config.clone()).await;
    let app = m0d::build_router_for_test(state, &config);

    // 1. Send POST /master/streaming
    let req_body = serde_json::json!({
        "audioPath": concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_stereo_input.wav"),
        "presetId": "podcast",
        "flavourId": null,
        "intentTone": 0.5,
        "intentDynamics": 0.5
    });

    let request = Request::builder()
        .method("POST")
        .uri("/master/streaming")
        .header("Content-Type", "application/json")
        .body(Body::from(req_body.to_string()))
        .unwrap();

    let app_clone = app.clone();
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let job_id = json
        .get("job_id")
        .expect("Missing job_id")
        .as_str()
        .unwrap()
        .to_string();
    println!("Got job_id: {}", job_id);

    // 2. Poll GET /progress/:job_id
    let mut final_stage = String::new();
    let mut final_blob_id = String::new();
    for _ in 0..60 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let poll_req = Request::builder()
            .method("GET")
            .uri(format!("/progress/{}", job_id))
            .body(Body::empty())
            .unwrap();

        let res = app_clone.clone().oneshot(poll_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let b = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();

        let stage = j.get("stage").unwrap().as_str().unwrap();
        println!("Polled stage: {}", stage);

        if stage == "CERTIFIED" || stage == "ERROR" {
            final_stage = stage.to_string();
            if let Some(b) = j.get("blob_id").and_then(|v| v.as_str()) {
                final_blob_id = b.to_string();
            }
            if stage == "ERROR" {
                println!("Got ERROR: {:?}", j.get("error"));
            }
            break;
        }
    }

    assert_eq!(final_stage, "CERTIFIED", "Final stage should be CERTIFIED");

    // 3. Check output file
    let out_path = format!("/tmp/m0d-v3-streaming-{}.wav", final_blob_id);
    let out_meta = std::fs::metadata(&out_path).expect("Output file should exist");
    assert!(out_meta.len() > 1000, "Output file should be non-empty");
    println!("Output file size: {}", out_meta.len());
}

#[tokio::test]
async fn test_streaming_progress_reaches_sse_listeners() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tempfile::TempDir;
    use tower::ServiceExt;

    // Same boilerplate as test_streaming_http_smoke — a fresh app instance.
    let audit_dir = TempDir::new().unwrap();
    let audit = std::sync::Arc::new(
        m0d::audit::AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap(),
    );
    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config.clone()).await;
    let app = m0d::build_router_for_test(state, &config);

    // 1. Trigger streaming mastering.
    let req_body = serde_json::json!({
        "audioPath": concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_stereo_input.wav"),
        "presetId": "podcast",
        "flavourId": null,
        "intentTone": 0.5,
        "intentDynamics": 0.5
    });
    let request = Request::builder()
        .method("POST")
        .uri("/master/streaming")
        .header("content-type", "application/json")
        .body(Body::from(req_body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let j: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let job_id = j
        .get("job_id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();

    // 2. Subscribe to the REAL SSE stream — the exact mechanism the
    // Tauri client uses (reqwest_eventsource) and the exact mechanism
    // that silently never woke up before the progress_tx fix. This
    // test would have hung (and eventually timed out) against the
    // pre-fix code, because progress_tx.send was never called.
    let sse_request = Request::builder()
        .method("GET")
        .uri(format!("/progress/{job_id}/stream"))
        .body(Body::empty())
        .unwrap();
    let sse_response = app.clone().oneshot(sse_request).await.unwrap();
    assert_eq!(sse_response.status(), StatusCode::OK);

    use futures_util::StreamExt;
    let mut body_stream = sse_response.into_body().into_data_stream();
    let mut buffer = String::new();
    let mut saw_certified = false;

    let result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while let Some(chunk) = body_stream.next().await {
            let bytes = chunk.expect("SSE stream error");
            buffer.push_str(&String::from_utf8_lossy(&bytes));
            if buffer.contains("\"stage\":\"CERTIFIED\"") {
                saw_certified = true;
                break;
            }
            if buffer.contains("\"stage\":\"ERROR\"") {
                panic!("Streaming mastering reported ERROR: {buffer}");
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "SSE stream never delivered CERTIFIED within 15s — the bell isn't ringing \
         (buffer so far: {buffer})"
    );
    assert!(saw_certified, "Stream ended without a CERTIFIED event");
}
