//! commands.rs — Response types mirroring Tauri backend command returns.
//! WASM-safe. All fields are typed primitives — no serde_json::Value.
//! Authority: Phase 6 P6-004/P6-006/P6-008

use serde::{Deserialize, Serialize};

/// AudioMeta from load_audio_file / open_audio_file Tauri commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetaResponse {
    /// Full filesystem path — used by trigger_mastering.
    pub path:        String,
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}

/// CoachFindings from evaluate_findings Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoachFindingsResponse {
    pub issues:         Vec<IssueResponse>,
    pub recommendation: String,
}

/// Single issue from evaluate_findings — flat fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueResponse {
    pub id:       String,
    pub severity: String,
    pub current:  f32,
    pub target:   f32,
    pub delta:    f32,
    pub tags:     Vec<String>,
}

/// Export result from export_audio command.
/// Phase 10: includes format field for export status display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
    pub format:       String,   // "wav" | "flac" | "opus"
}
