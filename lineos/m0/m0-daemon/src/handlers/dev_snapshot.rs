use axum::Json;
use std::sync::Mutex;

// Απλή in-memory κρατημα του τελευταίου snapshot — δεν χρειάζεται persistence,
// είναι debug tool, χάνεται σε restart, by design.
pub static LATEST_SNAPSHOT: Mutex<Option<serde_json::Value>> = Mutex::new(None);

pub async fn post_snapshot(Json(payload): Json<serde_json::Value>) -> &'static str {
    *LATEST_SNAPSHOT.lock().unwrap() = Some(payload);
    "ok"
}

pub async fn get_snapshot() -> Json<serde_json::Value> {
    let snap = LATEST_SNAPSHOT.lock().unwrap().clone();
    Json(snap.unwrap_or(serde_json::json!({"status": "no snapshot yet"})))
}
