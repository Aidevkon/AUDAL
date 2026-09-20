/// Moved from m0-daemon's app_state.rs 21/09 — a clean type that lived
/// next to `AppState { pub db: DbConn, ... }` only by file proximity.
/// `AppState`/`DbConn` stay in m0-daemon; app_state.rs re-exports this
/// in place so its other callers (agents/conductor.rs, agents/
/// operator.rs, handlers/master.rs, domain/dsp_pipeline.rs) keep
/// working unchanged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MasteringProgress {
    pub job_id: String,
    pub stage: String,
    pub elapsed_ms: u64,
    pub blob_id: Option<String>,
    pub error: Option<String>,
    /// Full-file, accurate BPM — telemetry only, for the Kepler UI
    /// instrument. None until the analysis pass computes it (most
    /// stages won't carry this; only the stage that follows the
    /// full-file measurement pass will). Unrelated to and never
    /// overriding PreAnalyzer's 30s-scout bpm, which drives real DSP
    /// ducking decisions elsewhere.
    pub bpm: Option<f32>,
}
