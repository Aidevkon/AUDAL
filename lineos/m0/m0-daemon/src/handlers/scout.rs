use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::dsp::sparse_scout::{run_sparse_scout, SparseScoutSummary};
use crate::handlers::decode::MAX_FILE_BYTES;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutRequest {
    pub path: String,
    pub n_samples: usize,
    pub window_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoutResponse {
    pub status: String,
    pub summary: Option<SparseScoutSummary>,
    pub message: Option<String>,
}

pub async fn run_scout(
    State(state): State<AppState>,
    Json(req): Json<ScoutRequest>,
) -> Json<ScoutResponse> {
    let meta = match std::fs::metadata(&req.path) {
        Ok(m) => m,
        Err(e) => {
            let msg = format!("File not found or unreadable: {e}");
            state
                .audit
                .write(AuditEntry::new("m0d.scout_failed", AuditLevel::Audit, &msg))
                .ok();
            return Json(ScoutResponse {
                status: "error".into(),
                summary: None,
                message: Some(msg),
            });
        }
    };

    if meta.len() > MAX_FILE_BYTES {
        let msg = format!(
            "File too large ({} bytes > {} limit)",
            meta.len(),
            MAX_FILE_BYTES
        );
        state
            .audit
            .write(AuditEntry::new("m0d.scout_failed", AuditLevel::Audit, &msg))
            .ok();
        return Json(ScoutResponse {
            status: "error".into(),
            summary: None,
            message: Some(msg),
        });
    }

    let path = req.path.clone();
    let n_samples = req.n_samples;
    let window_ms = req.window_ms;

    let result = tokio::task::spawn_blocking(move || {
        run_sparse_scout(std::path::Path::new(&path), n_samples, window_ms)
    })
    .await
    .map_err(|e| format!("Spawn block error: {:?}", e))
    .and_then(|r| r.map_err(|e| format!("{:?}", e)));

    match result {
        Ok(summary) => {
            state
                .audit
                .write(AuditEntry::new(
                    "m0d.scout_completed",
                    AuditLevel::Audit,
                    &req.path,
                ))
                .ok();
            Json(ScoutResponse {
                status: "ok".into(),
                summary: Some(summary),
                message: None,
            })
        }
        Err(e) => {
            state
                .audit
                .write(AuditEntry::new("m0d.scout_failed", AuditLevel::Audit, &e))
                .ok();
            Json(ScoutResponse {
                status: "error".into(),
                summary: None,
                message: Some(e),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppState;
    use crate::audit::AuditLog;
    use std::path::Path;
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn write_test_wav(path: &Path, sr: u32, dur_secs: f32, freq: f32, amp: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * dur_secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
            w.write_sample(s).unwrap(); // L
            w.write_sample(s).unwrap(); // R
        }
        w.finalize().unwrap();
    }

    async fn get_test_state() -> (AppState, tempfile::TempDir) {
        let audit_dir = tempfile::TempDir::new().unwrap();
        let audit = Arc::new(AuditLog::open(audit_dir.path().to_str().unwrap()).unwrap());
        let (state, _) = AppState::new_for_test(audit).await;
        (state, audit_dir)
    }

    #[tokio::test]
    async fn scout_endpoint_returns_summary_for_valid_wav() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        write_test_wav(&path, 48000, 1.0, 500.0, 0.5);

        let req = ScoutRequest {
            path: path.to_string_lossy().to_string(),
            n_samples: 10,
            window_ms: 10.0,
        };
        let (state, _dir) = get_test_state().await;
        let res = run_scout(State(state), Json(req)).await;

        assert_eq!(res.status, "ok");
        assert!(res.summary.is_some());
    }

    #[tokio::test]
    async fn scout_endpoint_returns_error_json_for_missing_file() {
        let req = ScoutRequest {
            path: "/path/to/nowhere.wav".into(),
            n_samples: 10,
            window_ms: 10.0,
        };
        let (state, _dir) = get_test_state().await;
        let res = run_scout(State(state), Json(req)).await;

        assert_eq!(res.status, "error");
        assert!(res.message.is_some());
        assert!(res.summary.is_none());
    }
}
