use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::sync::Arc;
use tempfile::TempDir;
use tower::ServiceExt;

#[tokio::test]
#[ignore = "Requires local untracked flight_clips_stereo dataset"]
async fn test_streaming_http_hybrid_smoke() {
    let audit_dir = TempDir::new().unwrap();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap());

    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config.clone()).await;
    let app = m0d::build_router_for_test(state, &config);

    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../flight_clips_stereo/clip_podcast_st.wav"
    );
    let output_path = "/tmp/test_streaming_http_hybrid_output.wav";

    // 1. Send POST /master/streaming
    let req_body = serde_json::json!({
        "audioPath": input_path,
        "outputPath": output_path,
        "presetId": "podcast",
        "flavourId": null
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
    for _ in 0..120 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

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

        if stage == "COMPLETED" || stage == "ERROR" {
            final_stage = stage.to_string();
            if stage == "ERROR" {
                println!("Got ERROR: {:?}", j.get("error"));
            }
            break;
        }
    }

    assert_eq!(final_stage, "COMPLETED", "Final stage should be COMPLETED");

    // 3. Check output file
    let out_meta = std::fs::metadata(output_path).expect("Output file should exist");
    assert!(out_meta.len() > 1000, "Output file should be non-empty");

    // 4. Compute MSE to prove Hybrid NMF engaged (2.0s - 13.0s)
    let (stream_interleaved, _, _) =
        m0d::handlers::decode::decode_raw_interleaved(output_path).unwrap();
    let (input_interleaved, sr, _) =
        m0d::handlers::decode::decode_raw_interleaved(input_path).unwrap();

    let stream_left: Vec<f32> = stream_interleaved.iter().step_by(2).copied().collect();
    let input_left: Vec<f32> = input_interleaved.iter().step_by(2).copied().collect();

    let start_idx = (2.0 * sr as f32) as usize;
    let end_idx = (13.0 * sr as f32) as usize;

    let end_idx = end_idx.min(stream_left.len()).min(input_left.len());

    let mut mse_dual = 0.0;
    let mut count = 0;
    for i in start_idx..end_idx {
        let diff = stream_left[i] - input_left[i];
        mse_dual += diff * diff;
        count += 1;
    }
    if count > 0 {
        mse_dual /= count as f32;
    }

    println!("MSE Dual Graph (Hybrid 2.0s - 13.0s): {:.8}", mse_dual);

    assert!(
        mse_dual > 1e-6,
        "Dual graph path (NMF reconstruction) should differ from raw mix. Got MSE: {:.8}",
        mse_dual
    );
}
