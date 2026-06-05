//! M0 IPC client — all M0 communication goes through here.
//! M0 Constitution v2.0 §03: M0 is the trust boundary.
//! This is the ONLY place the Tauri backend calls M0.
//!
//! FORBIDDEN: Calling sp314-dsp directly from this crate.
//! FORBIDDEN: Audio processing in this crate.
//! FORBIDDEN: serde_json::Value crossing the WASM boundary.
//!
//! GoldenBlobJson field contract: golden-blob-spec.md v1.0 §Structure
//! All fields are typed primitives — no serde_json::Value.

use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// M0 health endpoint — port 7401 (dedicated health router).
const M0_HEALTH_BASE: &str = "http://127.0.0.1:7401";

/// M0 mastering/blob/export API — port 7402 (direct, bypassing Caddy).
/// Caddy proxy on :7400 was never configured; talk to M0 directly.
const M0_BASE: &str = "http://127.0.0.1:7402";

/// M0 playback server URL — also port 7402 (xaak playback on same router).
const M0_PLAYBACK_BASE: &str = "http://127.0.0.1:7402";

/// Per-request timeouts.
/// Mastering can take 60-120s for large files — give it 5 minutes.
const TIMEOUT_MASTER_SECS: u64  = 300;   // POST /master — long pipeline
const TIMEOUT_DEFAULT_SECS: u64 =  30;   // health / blob / export

/// Stateless reqwest client. Create per-request (Phase 7: pool with AppState).
pub struct M0Client {
    client: Client,
}

unsafe impl Send for M0Client {}
unsafe impl Sync for M0Client {}

impl M0Client {
    pub fn new() -> Self {
        let client = ClientBuilder::new()
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    /// GET /health — verify M0 is running before any operation.
    /// Guard for ASC 0x05 (WasmPanic / daemon unreachable).
    pub async fn health(&self) -> Result<M0HealthResponse, M0Error> {
        let resp = self.client
            .get(format!("{M0_HEALTH_BASE}/health"))
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::Unhealthy);
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// POST /master — trigger mastering pipeline.
    /// Returns MasterResponse with blob_id on success.
    /// Timeout: 300s — mastering a large file takes 60-120s.
    pub async fn trigger_mastering(
        &self,
        req: MasterRequest,
    ) -> Result<MasterResponse, M0Error> {
        let resp = self.client
            .post(format!("{M0_BASE}/master"))
            .json(&req)
            // Override the default 30s timeout — mastering is slow.
            .timeout(Duration::from_secs(TIMEOUT_MASTER_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(M0Error::RequestFailed(status));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// GET /blob/{id} — fetch Golden Blob as JSON.
    /// No binary data crosses IPC boundary — metrics only.
    pub async fn get_blob(&self, id: &str) -> Result<GoldenBlobJson, M0Error> {
        let resp = self.client
            .get(format!("{M0_BASE}/blob/{id}"))
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::BlobNotFound(id.to_string()));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    pub async fn get_progress(&self, job_id: &str) -> Result<MasteringProgress, String> {
        let url = format!("{M0_BASE}/progress/{job_id}");
        let resp = self.client.get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send().await.map_err(|e| e.to_string())?
            .json::<MasteringProgress>().await.map_err(|e| e.to_string())?;
        Ok(resp)
    }

    /// POST /export — write Golden Blob audio to disk as FLAC or WAV.
    /// FM6 flow: export_clicked → POST /export → FM5 (success) | FM-ERR (fail).
    pub async fn export(
        &self,
        blob_id: &str,
        format: &str,
        path: &str,
    ) -> Result<ExportResponse, M0Error> {
        let resp = self.client
            .post(format!("{M0_BASE}/export"))
            .json(&ExportRequest {
                blob_id:     blob_id.to_string(),
                format:      format.to_string(),
                output_path: path.to_string(),
            })
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::RequestFailed(resp.status().as_u16()));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// POST /playback/control — play | pause | stop | seek
    /// Phase 12A (A-003 §8): cpal playback via xaak kernel.
    pub async fn playback_control(
        &self,
        action:      &str,
        position_ms: Option<u64>,
    ) -> Result<Option<crate::commands::playback::PlaybackStateJson>, M0Error> {
        let resp = self.client
            .post(format!("{M0_PLAYBACK_BASE}/playback/control"))
            .json(&PlaybackControlRequest {
                action:      action.to_string(),
                position_ms,
            })
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::RequestFailed(resp.status().as_u16()));
        }

        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct ControlResp {
            status:  String,
            state:   Option<crate::commands::playback::PlaybackStateJson>,
            message: Option<String>,
        }
        let body: ControlResp = resp.json().await
            .map_err(|e| M0Error::ParseError(e.to_string()))?;

        if body.status == "ok" {
            Ok(body.state)
        } else {
            Err(M0Error::RequestFailed(400))
        }
    }

    /// GET /playback/state — current position (non-blocking).
    pub async fn get_playback_state(
        &self,
    ) -> Result<Option<crate::commands::playback::PlaybackStateJson>, M0Error> {
        let resp = self.client
            .get(format!("{M0_PLAYBACK_BASE}/playback/state"))
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::RequestFailed(resp.status().as_u16()));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// GET /playback/telemetry — live momentary LUFS from active blob (P12B-005).
    pub async fn get_live_telemetry(
        &self,
    ) -> Result<Option<crate::commands::playback::LiveTelemetryJson>, M0Error> {
        let resp = self.client
            .get(format!("{M0_PLAYBACK_BASE}/playback/telemetry"))
            .timeout(Duration::from_secs(TIMEOUT_DEFAULT_SECS))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::RequestFailed(resp.status().as_u16()));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }
}

// ── Request / Response types ──────────────────────────────────────────────────

/// POST /playback/control body (Phase 12A)
#[derive(Debug, Serialize, Deserialize)]
pub struct PlaybackControlRequest {
    pub action:      String,
    pub position_ms: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id:  String,
    pub flavour_id: String,
    pub intent_tone: f32,
    pub intent_dynamics: f32,
}

#[derive(Debug, serde::Deserialize)]
pub struct MasteringProgress {
    pub job_id:     String,
    pub stage:      String,
    pub elapsed_ms: u64,
    pub blob_id:    Option<String>,
    pub error:      Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct MasterResponse {
    pub job_id:  Option<String>,
    #[serde(default)]
    pub blob_id: Option<String>,
    #[serde(default)]
    pub status:  Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct M0HealthResponse {
    pub status: String,              // "ok" | "degraded"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportRequest {
    pub blob_id:     String,
    pub format:      String,             // "wav" | "flac" | "opus"
    pub output_path: String,             // absolute filesystem path
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportResponse {
    pub written_path: String,
    pub status:       String,        // "ok" | "error"
    pub message:      Option<String>,
}

// ── GoldenBlobJson — the IPC contract ────────────────────────────────────────
//
// Authority: golden-blob-spec.md v1.0 §Structure
// Rule: ALL fields are typed primitives. serde_json::Value FORBIDDEN.
// Rule: Cockpit receives metrics only — never raw audio bytes.
// Rule: Fields must match golden-blob-spec.md structure exactly.

/// Golden Blob as JSON — no binary data crosses the Tauri IPC boundary.
/// Cockpit receives metrics only; audio bytes remain in M0 storage.
/// Field contract: golden-blob-spec.md v1.0
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoldenBlobJson {
    /// Globally unique UUID, generated at blob creation.
    pub id:               String,
    /// Schema version, e.g. "1.0".
    pub version:          String,
    /// "audio" | "av" — immutable after creation.
    #[serde(rename = "type")]
    pub blob_type:        String,
    /// ISO 8601 UTC creation timestamp.
    pub created_at:       String,
    /// SHA-256 hex of original input file(s).
    pub input_hash:       String,
    /// Determinism seed, derived from input_hash. Serialized as string to preserve u64 precision in JS.
    pub seed:             String,
    /// Semver of the producing engine (e.g. "0.4.0").
    pub pipeline_version: String,
    /// User-selected preset (e.g. "spotify"). Not in spec directly but needed
    /// for rule-engine evaluation — carried in provenance.pipeline_params.
    pub preset_id:        String,
    /// BS.1770-4 loudness measurements + platform compliance.
    pub loudness:         LoudnessMetricsJson,
    /// Objective quality measurements.
    pub quality:          QualityMetricsJson,
    /// Full audit trail.
    pub provenance:       ProvenanceJson,
    #[serde(default = "default_schema_v1_gc")]
    pub schema_version:   u32,
    #[serde(default)]
    pub aether_cert:      Option<String>,
    #[serde(default)]
    pub aether_persona:   Option<String>,
    #[serde(default)]
    pub aether_config:    Option<String>,
    #[serde(default)]
    pub stem_fingerprints: Option<StemFingerprints>,
    #[serde(default)]
    pub qr_base64: Option<String>,
    #[serde(default)]
    pub pcm_blake3: Option<String>,
    #[serde(default)]
    pub cert_signature: Option<String>,
    #[serde(default)]
    pub processing_timeline: Vec<StageRecord>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StageRecord {
    pub stage:       String,
    pub duration_ms: u64,
    pub stage_hash:  String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StemFingerprints {
    pub voice:     String,
    pub drums:     String,
    pub bass:      String,
    pub harmonics: String,
    pub ambience:  String,
    pub pipeline:  String,
}

fn default_schema_v1_gc() -> u32 { 1 }

/// BS.1770-4 canonical values + EBU R128 + platform compliance flags.
/// Authority: golden-blob-spec.md §LoudnessMetrics
/// integrated_lufs, true_peak_dbtp, lra — measured values (authoritative).
/// All platform targets — derived from BS.1770-4, never re-measured.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoudnessMetricsJson {
    // BS.1770-4 canonical (always present, measured)
    pub integrated_lufs:          f32,
    pub short_term_lufs:          f32,
    pub momentary_lufs:           f32,
    pub true_peak_dbtp:           f32,
    pub lra:                      f32,
    pub k_weighted:               bool,
    // EBU R128 derived
    pub ebu_r128_target_lufs:     f32,
    pub ebu_r128_compliant:       bool,
    // Platform compliance (derived from BS.1770-4)
    pub spotify_compliant:        bool,
    pub youtube_compliant:        bool,
    pub apple_music_compliant:    bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant:      bool,
    pub tidal_compliant:          bool,
}

/// Objective quality measurements from the processed content.
/// Authority: golden-blob-spec.md §QualityMetrics
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QualityMetricsJson {
    // Stereo / phase
    pub stereo_correlation: f32,
    pub phase_coherence:    f32,
    pub stereo_width:       f32,
    // Dynamic range
    pub dynamic_range_db:   f32,
    pub rms_db:             f32,
    // Spectral
    pub spectral_centroid:  f32,
    pub spectral_flatness:  f32,
    // Clipping
    pub clips_detected:     u32,
    pub clip_free:          bool,
}

/// Full audit trail — every blob knows exactly how it was produced.
/// Authority: golden-blob-spec.md §Provenance
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProvenanceJson {
    pub engine_id:            String,   // e.g. "E11"
    pub engine_version:       String,
    pub processing_time_ms:   u64,
    pub host_os:              String,
    pub created_by:           String,
    pub aether_enriched:      bool,
    pub aether_devices:       Vec<String>,
}

// ── M0Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum M0Error {
    #[error("M0 unreachable: {0}")]
    Unreachable(String),
    #[error("M0 health check failed")]
    Unhealthy,
    #[error("M0 request failed with HTTP {0}")]
    RequestFailed(u16),
    #[error("Blob not found: {0}")]
    BlobNotFound(String),
    #[error("M0 parse error: {0}")]
    ParseError(String),
}

/// Allow M0Error to be returned from Tauri commands as String.
impl serde::Serialize for M0Error {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        s.serialize_str(&self.to_string())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_golden_blob_json_has_all_spec_fields() {
        // Verify GoldenBlobJson can be constructed matching spec §Structure
        let blob = GoldenBlobJson {
            id:               "uuid-test".into(),
            version:          "1.0".into(),
            blob_type:        "audio".into(),
            created_at:       "2026-04-15T00:00:00Z".into(),
            input_hash:       "abcdef1234567890".into(),
            seed:             "42".into(),
            pipeline_version: "0.4.0".into(),
            preset_id:        "spotify".into(),
            loudness: LoudnessMetricsJson {
                integrated_lufs:          -14.0,
                short_term_lufs:          -13.5,
                momentary_lufs:           -12.0,
                true_peak_dbtp:           -1.0,
                lra:                       8.0,
                k_weighted:               true,
                ebu_r128_target_lufs:     -23.0,
                ebu_r128_compliant:       false,
                spotify_compliant:        true,
                youtube_compliant:        true,
                apple_music_compliant:    false,
                apple_podcasts_compliant: false,
                broadcast_compliant:      false,
                tidal_compliant:          true,
            },
            quality: QualityMetricsJson {
                stereo_correlation: 0.94,
                phase_coherence:    0.97,
                stereo_width:       0.74,
                dynamic_range_db:   9.5,
                rms_db:             -16.0,
                spectral_centroid:  3_200.0,
                spectral_flatness:  0.12,
                clips_detected:     0,
                clip_free:          true,
            },
            provenance: ProvenanceJson {
                engine_id:          "E11".into(),
                engine_version:     "0.4.0".into(),
                processing_time_ms: 1_234,
                host_os:            "linux-x86_64".into(),
                created_by:         "session-001".into(),
                aether_enriched:    false,
                aether_devices:     vec![],
            },
            schema_version:   1,
            aether_cert:      None,
            aether_persona:   None,
            aether_config:    None,
            stem_fingerprints: None,
            qr_base64:        None,
            pcm_blake3:       None,
            cert_signature:   None,
            processing_timeline: vec![],
        };

        // Spec §Determinism: JSON round-trip must be lossless
        let json = serde_json::to_string(&blob).unwrap();
        let re: GoldenBlobJson = serde_json::from_str(&json).unwrap();
        assert_eq!(re.id, "uuid-test");
        assert_eq!(re.blob_type, "audio");
        assert!((re.loudness.integrated_lufs - (-14.0)).abs() < 1e-6);
        assert_eq!(re.quality.clips_detected, 0);
        assert!(!re.provenance.aether_enriched);
    }

    #[test]
    fn test_m0_error_serializes_as_string() {
        let err = M0Error::Unreachable("connection refused".into());
        let s = serde_json::to_string(&err).unwrap();
        assert!(s.contains("unreachable"));
    }

    #[test]
    fn test_m0_error_blob_not_found() {
        let err = M0Error::BlobNotFound("nonexistent-id".into());
        assert!(err.to_string().contains("nonexistent-id"));
    }
}
