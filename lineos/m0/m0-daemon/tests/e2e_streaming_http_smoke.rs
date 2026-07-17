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
        "outputPath": "/tmp/test_streaming_http_output.wav",
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
            if stage == "ERROR" {
                println!("Got ERROR: {:?}", j.get("error"));
            }
            break;
        }
    }

    assert_eq!(final_stage, "CERTIFIED", "Final stage should be CERTIFIED");

    // 3. Check output file
    let out_meta =
        std::fs::metadata("/tmp/test_streaming_http_output.wav").expect("Output file should exist");
    assert!(out_meta.len() > 1000, "Output file should be non-empty");
    println!("Output file size: {}", out_meta.len());
}
