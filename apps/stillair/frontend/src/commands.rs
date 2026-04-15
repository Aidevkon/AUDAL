//! commands.rs — Response types mirroring Tauri backend command returns.
//! WASM-safe. All fields are typed primitives — no serde_json::Value.
//! Authority: Phase 6 P6-004/P6-006/P6-008

use serde::{Deserialize, Serialize};

/// AudioMeta from load_audio_file Tauri command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMetaResponse {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub written_path: String,
}
