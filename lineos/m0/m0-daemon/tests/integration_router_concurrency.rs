use axum::http::StatusCode;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{Duration, Instant};

#[tokio::test]
async fn test_router_concurrency_limit_applies_http_backpressure() {
    // 1. Force the router to use a max concurrency of 2 (isolated to this test binary)
    std::env::set_var("M0_MAX_CONCURRENT_JOBS", "2");

    // 2. Setup the test router (requires audit log directory)
    let audit_dir = TempDir::new().unwrap();
    let audit = Arc::new(m0d::audit::AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap());
    
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit).await;
    let app = m0d::build_router_for_test(state);

    // 3. Bind to an ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{}", port);

    // 4. Start the server in the background
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Let the server spin up (ensures listener is ready)
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let mut handles = Vec::new();

    let start = Instant::now();

    // 5. Fire 4 parallel requests, each asking the server to sleep for 100ms
    for _ in 0..4 {
        let url = format!("{}/dev/wait", base_url);
        let client_clone = client.clone();
        handles.push(tokio::spawn(async move {
            let res = client_clone
                .post(&url)
                .json(&serde_json::json!({ "ms": 100 }))
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }));
    }

    // 6. Wait for all 4 requests to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    println!("elapsed: {:?}", elapsed);

    // 7. Verify backpressure mathematically:
    // If the limit (2) works, 4 requests of 100ms each will be processed in 2 batches.
    // Batch 1 (2 reqs) finishes at 100ms.
    // Batch 2 (2 reqs) finishes at 200ms.
    // If backpressure failed, all 4 would run in parallel and finish in ~100ms.
    assert!(
        elapsed >= Duration::from_millis(200),
        "MEASUREMENT FAILED: Expected elapsed time >= 200ms due to concurrency limit, but took {:?}",
        elapsed
    );
    
    // Check it's not absurdly slow (e.g. fully sequential taking 400ms+)
    assert!(
        elapsed < Duration::from_millis(350),
        "MEASUREMENT FAILED: Expected elapsed time < 350ms (2 parallel batches), but took {:?}",
        elapsed
    );
}
