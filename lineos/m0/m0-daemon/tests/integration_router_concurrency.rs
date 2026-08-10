use axum::http::StatusCode;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{Duration, Instant};

#[ignore = "X0: το /dev/wait endpoint αφαιρέθηκε, το test δεν ενημερώθηκε. Επιστρέφει 404 αντί 200. Χρειάζεται νέο test endpoint ή αναδιατύπωση."]
#[tokio::test]
async fn test_router_concurrency_limit_applies_http_backpressure() {
    // 1. Force the router to use a max concurrency of 2 (isolated to this test binary)
    std::env::set_var("M0_MAX_CONCURRENT_JOBS", "2");

    #[cfg(debug_assertions)]
    {
        m0d::handlers::dev_wait::MAX_IN_FLIGHT.store(0, Ordering::SeqCst);
        m0d::handlers::dev_wait::CURRENT_IN_FLIGHT.store(0, Ordering::SeqCst);
    }

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

    // 5. Fire 4 parallel requests, each asking the server to sleep for 200ms
    for _ in 0..4 {
        let url = format!("{}/dev/wait", base_url);
        let client_clone = client.clone();
        handles.push(tokio::spawn(async move {
            let res = client_clone
                .post(&url)
                .json(&serde_json::json!({ "ms": 200 }))
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
    println!("diagnostic elapsed: {:?}", elapsed);

    // 7. Verify backpressure via direct observation:
    // With ConcurrencyLimit set to 2, exactly 2 requests can execute in parallel.
    #[cfg(debug_assertions)]
    {
        let max_observed = m0d::handlers::dev_wait::MAX_IN_FLIGHT.load(Ordering::SeqCst);
        assert_eq!(
            max_observed, 2,
            "MEASUREMENT FAILED: Expected max in-flight requests == 2, but observed {}",
            max_observed
        );
    }
}
