//! SchemaAgent — R1: validates JSON, never generates.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.1
//! Motto: "I only say yes or no. I never create."
//!
//! Owns the canonical ProjectState in memory.
//! The ONLY agent permitted to write state.
//! All reads and writes go through Intent — never direct access.

use super::operator::{Intent, SchemaError};
use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Canonical in-memory project state.
/// SchemaAgent is the sole owner and writer.
struct ProjectState {
    data: HashMap<String, Value>,
}

impl ProjectState {
    fn new() -> Self {
        let mut data = HashMap::new();
        // Default DSP parameters
        data.insert("dsp.target_lufs".into(), Value::from(-14.0_f64));
        data.insert("dsp.max_tp_db".into(), Value::from(-1.0_f64));
        data.insert("dsp.preset_id".into(), Value::from("spotify"));
        data.insert("session.blob_id".into(), Value::Null);
        data.insert("session.status".into(), Value::from("idle"));
        Self { data }
    }

    /// R1: validate patch before applying.
    /// Returns Err if patch contains invalid types or out-of-range values.
    fn validate_patch(&self, patch: &Value) -> Result<(), SchemaError> {
        let obj = patch
            .as_object()
            .ok_or_else(|| SchemaError::ValidationFailed("patch must be a JSON object".into()))?;

        for (key, val) in obj {
            match key.as_str() {
                "dsp.target_lufs" => {
                    let v = val.as_f64().ok_or_else(|| {
                        SchemaError::ValidationFailed("dsp.target_lufs must be f64".into())
                    })?;
                    if !(-40.0..=0.0).contains(&v) {
                        return Err(SchemaError::ValidationFailed(format!(
                            "dsp.target_lufs {v} out of range [-40, 0]"
                        )));
                    }
                }
                "dsp.max_tp_db" => {
                    let v = val.as_f64().ok_or_else(|| {
                        SchemaError::ValidationFailed("dsp.max_tp_db must be f64".into())
                    })?;
                    if !(-6.0..=0.0).contains(&v) {
                        return Err(SchemaError::ValidationFailed(format!(
                            "dsp.max_tp_db {v} out of range [-6, 0]"
                        )));
                    }
                }
                "dsp.preset_id" => {
                    let v = val.as_str().ok_or_else(|| {
                        SchemaError::ValidationFailed("dsp.preset_id must be string".into())
                    })?;
                    const ALLOWED: &[&str] = &[
                        "spotify",
                        "youtube",
                        "apple_music",
                        "apple_podcast",
                        "tidal",
                        "broadcast",
                        "raw",
                        "amazon",
                    ];
                    if !ALLOWED.contains(&v) {
                        return Err(SchemaError::ValidationFailed(format!(
                            "dsp.preset_id '{v}' not allowed"
                        )));
                    }
                }
                "session.blob_id" => {
                    // Null or string — both valid
                    if !val.is_null() && !val.is_string() {
                        return Err(SchemaError::ValidationFailed(
                            "session.blob_id must be string or null".into(),
                        ));
                    }
                }
                "session.status" => {
                    let v = val.as_str().ok_or_else(|| {
                        SchemaError::ValidationFailed("session.status must be string".into())
                    })?;
                    const ALLOWED: &[&str] =
                        &["idle", "analyzing", "processing", "certified", "error"];
                    if !ALLOWED.contains(&v) {
                        return Err(SchemaError::ValidationFailed(format!(
                            "session.status '{v}' not allowed"
                        )));
                    }
                }
                // Unknown keys: reject — R1 never silently accepts unknown fields
                _ => {
                    return Err(SchemaError::ValidationFailed(format!(
                        "unknown schema key: '{key}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Apply validated patch to state.
    /// Only called after validate_patch returns Ok.
    fn apply_patch(&mut self, patch: Value) {
        if let Some(obj) = patch.as_object() {
            for (key, val) in obj {
                self.data.insert(key.clone(), val.clone());
            }
        }
    }

    /// Read a value by dot-path key.
    fn query(&self, path: &str) -> Value {
        self.data.get(path).cloned().unwrap_or(Value::Null)
    }
}

/// SchemaAgent main loop.
/// Receives Intents, validates, reads/writes ProjectState.
/// Never generates JSON. Never calls external services.
pub async fn run(mut rx: mpsc::Receiver<Intent>) {
    let mut state = ProjectState::new();

    while let Some(intent) = rx.recv().await {
        match intent {
            Intent::Shutdown => break,

            Intent::ValidateSchema { patch, response } => {
                let result = state.validate_patch(&patch).map(|()| {
                    state.apply_patch(patch);
                });
                let _ = response.send(result);
            }

            Intent::QuerySchema { path, response } => {
                let _ = response.send(state.query(&path));
            }

            _ => {
                // R1 handles only schema intents — all others ignored
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ProjectState {
        ProjectState::new()
    }

    #[test]
    fn valid_target_lufs_accepted() {
        let s = state();
        let patch = serde_json::json!({ "dsp.target_lufs": -14.0 });
        assert!(s.validate_patch(&patch).is_ok());
    }

    #[test]
    fn out_of_range_lufs_rejected() {
        let s = state();
        let patch = serde_json::json!({ "dsp.target_lufs": 5.0 });
        assert!(s.validate_patch(&patch).is_err());
    }

    #[test]
    fn invalid_preset_rejected() {
        let s = state();
        let patch = serde_json::json!({ "dsp.preset_id": "unknown_preset" });
        assert!(s.validate_patch(&patch).is_err());
    }

    #[test]
    fn unknown_key_rejected() {
        let s = state();
        let patch = serde_json::json!({ "dsp.unknown_field": true });
        assert!(s.validate_patch(&patch).is_err());
    }

    #[test]
    fn r1_never_generates_default_values() {
        let s = state();
        // R1 must reject null loudness — cannot fill in defaults
        let patch = serde_json::json!({ "dsp.target_lufs": null });
        assert!(s.validate_patch(&patch).is_err());
    }

    #[test]
    fn query_returns_default_state() {
        let s = state();
        let lufs = s.query("dsp.target_lufs");
        assert_eq!(lufs.as_f64().unwrap(), -14.0);
    }

    #[test]
    fn apply_then_query_roundtrip() {
        let mut s = state();
        let patch = serde_json::json!({ "dsp.target_lufs": -16.0 });
        s.validate_patch(&patch).unwrap();
        s.apply_patch(patch);
        assert_eq!(s.query("dsp.target_lufs").as_f64().unwrap(), -16.0);
    }
}
