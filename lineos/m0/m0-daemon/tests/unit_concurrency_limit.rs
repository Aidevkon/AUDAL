use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;
use tower::{limit::ConcurrencyLimitLayer, ServiceBuilder, Service, ServiceExt};

#[derive(Clone)]
struct MockSlowService {
    active_requests: Arc<Mutex<usize>>,
    max_observed: Arc<Mutex<usize>>,
}

impl Service<()> for MockSlowService {
    type Response = ();
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: ()) -> Self::Future {
        let active_requests = self.active_requests.clone();
        let max_observed = self.max_observed.clone();

        Box::pin(async move {
            {
                let mut active = active_requests.lock().unwrap();
                *active += 1;
                let mut max = max_observed.lock().unwrap();
                if *active > *max {
                    *max = *active;
                }
            }

            // Sleep for 50ms to simulate slow work
            tokio::time::sleep(Duration::from_millis(50)).await;

            {
                let mut active = active_requests.lock().unwrap();
                *active -= 1;
            }

            Ok(())
        })
    }
}

#[tokio::test]
async fn test_concurrency_limit_blocks_excess_requests() {
    let active_requests = Arc::new(Mutex::new(0));
    let max_observed = Arc::new(Mutex::new(0));

    let mock_service = MockSlowService {
        active_requests: active_requests.clone(),
        max_observed: max_observed.clone(),
    };

    // Wrap the service in a ConcurrencyLimitLayer (limit 2)
    // Then wrap it in a BufferLayer (capacity 10) so we can spawn multiple requests 
    // without the caller immediately blocking awaiting capacity.
    let svc = ServiceBuilder::new()
        .layer(tower::buffer::BufferLayer::new(10))
        .layer(ConcurrencyLimitLayer::new(2))
        .service(mock_service);

    let start = Instant::now();

    let mut handles = vec![];
    // Spawn 4 requests concurrently
    for _ in 0..4 {
        let mut svc_clone = svc.clone();
        handles.push(tokio::spawn(async move {
            svc_clone.ready().await.unwrap().call(()).await.unwrap();
        }));
    }

    // Wait for all 4 requests to complete
    for handle in handles {
        handle.await.unwrap();
    }

    let elapsed = start.elapsed();
    let max = *max_observed.lock().unwrap();

    println!("max_observed: {}", max);
    println!("elapsed: {:?}", elapsed);

    // Verify limit was respected
    assert!(max <= 2, "Observed concurrency exceeded the limit of 2 (was {})", max);
    
    // Verify timing: 
    // 4 tasks / 2 concurrency = 2 batches. 
    // Each batch takes 50ms, so total time must be >= 100ms.
    assert!(elapsed >= Duration::from_millis(100), "Tasks completed too fast, limit wasn't enforced");
}
