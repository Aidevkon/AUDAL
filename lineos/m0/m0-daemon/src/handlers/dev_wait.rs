use axum::Json;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct WaitRequest {
    pub ms: u64,
}

/// POST /dev/wait — Debug endpoint to artificially hold a connection open.
/// Used specifically to prove TCP/HTTP backpressure and concurrency limits.
pub async fn post_wait(Json(req): Json<WaitRequest>) -> &'static str {
    tokio::time::sleep(Duration::from_millis(req.ms)).await;
    "ok"
}
