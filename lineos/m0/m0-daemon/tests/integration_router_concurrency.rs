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

    let config = std::sync::Arc::new(m0d::config::M0Config::from_env());
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config.clone()).await;
    let app = m0d::build_router_for_test(state, &config);

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

    // Check it's not absurdly slow (fully sequential — all 4 requests
    // one at a time — would take ~400ms, which would indicate the
    // concurrency limit is silently 1 instead of 2). Widened from
    // 350ms to 390ms (2026-07-19, F-033): observed a real false
    // failure at 374ms during a loaded `just ci` run (correct
    // backpressure behavior, just slower due to system load) — the
    // original 350ms left almost no margin above the expected ~200ms
    // correct case. 390ms keeps real detection power (still well
    // below the ~400ms a fully-serial regression would produce) while
    // giving load-induced jitter a realistic buffer. If this still
    // proves flaky under heavier CI load, the CORRECT fix (not done
    // here — requires production code) is a #[cfg(debug_assertions)]
    // atomic in-flight counter the test can observe directly instead
    // of inferring concurrency from wall-clock time.
    assert!(
        elapsed < Duration::from_millis(390),
        "MEASUREMENT FAILED: Expected elapsed time < 390ms (2 parallel \
         batches, allowing for system load), but took {:?} — if this is \
         consistently close to 400ms, the concurrency limit may not be \
         applying correctly",
        elapsed
    );
}
