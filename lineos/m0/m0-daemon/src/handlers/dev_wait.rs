use axum::Json;
use serde::Deserialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[cfg(debug_assertions)]
pub static MAX_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
pub static CURRENT_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Deserialize)]
pub struct WaitRequest {
    pub ms: u64,
}

#[cfg(debug_assertions)]
struct InFlightGuard;

#[cfg(debug_assertions)]
impl Drop for InFlightGuard {
    fn drop(&mut self) {
        CURRENT_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// POST /dev/wait — Debug endpoint to artificially hold a connection open.
/// Used specifically to prove TCP/HTTP backpressure and concurrency limits.
pub async fn post_wait(Json(req): Json<WaitRequest>) -> &'static str {
    #[cfg(debug_assertions)]
    let _guard = {
        let cur = CURRENT_IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
        MAX_IN_FLIGHT.fetch_max(cur, Ordering::SeqCst);
        InFlightGuard
    };

    tokio::time::sleep(Duration::from_millis(req.ms)).await;
    "ok"
}
