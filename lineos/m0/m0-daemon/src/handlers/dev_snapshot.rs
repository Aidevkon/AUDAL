use axum::extract::Query;
use axum::Json;
use serde::Deserialize;
use std::collections::VecDeque;
use std::sync::Mutex;

const MAX_HISTORY: usize = 10;

/// Dev-only diagnostic ring buffer.
/// Stores the last MAX_HISTORY snapshots
/// pushed by the cockpit JS bridge.
/// Each snapshot is { ts, state, dom? }.
pub static SNAPSHOTS: Mutex<VecDeque<serde_json::Value>> = Mutex::new(VecDeque::new());

pub async fn post_snapshot(Json(payload): Json<serde_json::Value>) -> &'static str {
    let mut q = SNAPSHOTS.lock().unwrap();
    if q.len() >= MAX_HISTORY {
        q.pop_front();
    }
    q.push_back(payload);
    "ok"
}

#[derive(Debug, Deserialize, Default)]
pub struct SnapshotQuery {
    /// Last N snapshots (default 1 = latest)
    pub history: Option<usize>,
    /// Find DOM element by id
    pub id: Option<String>,
    /// Return only the .state field
    pub state_only: Option<bool>,
}

pub async fn get_snapshot(Query(params): Query<SnapshotQuery>) -> Json<serde_json::Value> {
    let q = SNAPSHOTS.lock().unwrap();
    if q.is_empty() {
        return Json(serde_json::json!({
            "status": "no snapshot yet"
        }));
    }

    if params.state_only.unwrap_or(false) {
        let latest = q.back().unwrap();
        let state = latest
            .get("state")
            .cloned()
            .unwrap_or(serde_json::json!(null));
        return Json(state);
    }

    if let Some(n) = params.history {
        let n = n.min(q.len());
        let items: Vec<_> = q.iter().rev().take(n).cloned().collect();
        return Json(serde_json::json!(items));
    }

    if let Some(id) = params.id {
        let latest = q.back().unwrap();
        if let Some(dom) = latest.get("dom") {
            let found = find_by_id(dom, &id);
            return Json(serde_json::json!({
                "query": id,
                "found": found.is_some(),
                "element": found,
                "state": latest.get("state"),
            }));
        }
        return Json(serde_json::json!({
            "query": id,
            "found": false,
            "note": "latest snapshot has no dom (throttled — DOM only every ~2s)",
            "state": latest.get("state"),
        }));
    }

    Json(q.back().cloned().unwrap())
}

fn find_by_id(node: &serde_json::Value, target_id: &str) -> Option<serde_json::Value> {
    if let Some(attrs) = node.get("attrs") {
        if attrs.get("id").and_then(|v| v.as_str()) == Some(target_id) {
            return Some(node.clone());
        }
    }
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            if let Some(found) = find_by_id(child, target_id) {
                return Some(found);
            }
        }
    }
    None
}
