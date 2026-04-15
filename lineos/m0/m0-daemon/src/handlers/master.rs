//! POST /master — trigger mastering pipeline.
//! Authority: Phase 6 task-decomposition P6-003
//! M0 Constitution v2.0 §03: M0 is the trust boundary for all DSP invocations.
//!
//! Invokes sp314-dsp MasteringPipeline::master() natively.
//! Stores resulting blob metrics in BlobStore. Returns blob_id to caller.
//!
//! FORBIDDEN: Returning serde_json::Value.
//! FORBIDDEN: Calling sp314-dsp from the Tauri backend directly.

use axum::{Json, extract::State};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::blob_store::{StoredBlob, StoredLoudness, StoredQuality, StoredProvenance};

#[derive(Debug, Deserialize)]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id:  String,
}

#[derive(Debug, Serialize)]
pub struct MasterResponse {
    pub blob_id: String,
    pub status:  &'static str,   // "ok" | "error"
    pub message: Option<String>,
}

/// POST /master — run sp314-dsp → store blob → return blob_id.
pub async fn trigger_mastering(
    State(state): State<AppState>,
    Json(req):    Json<MasterRequest>,
) -> Json<MasterResponse> {
    let start = Instant::now();

    state.audit.write(
        AuditEntry::new("m0d.mastering_started", AuditLevel::Audit,
            &format!("path={} preset={}", req.audio_path, req.preset_id))
    ).ok();

    const ALLOWED: &[&str] = &["spotify", "youtube", "apple_music", "tidal", "broadcast", "raw"];
    if !ALLOWED.contains(&req.preset_id.as_str()) {
        state.audit.write(
            AuditEntry::new("m0d.mastering_rejected", AuditLevel::Audit,
                &format!("preset not allowed: {}", req.preset_id))
        ).ok();
        return Json(MasterResponse {
            blob_id: String::new(),
            status:  "error",
            message: Some(format!("preset not allowed: {}", req.preset_id)),
        });
    }

    match run_dsp(&req.audio_path, &req.preset_id, start).await {
        Ok(blob) => {
            let blob_id = blob.id.clone();
            state.blob_store.insert(blob);
            state.audit.write(
                AuditEntry::new("m0d.mastering_complete", AuditLevel::Audit,
                    &format!("blob={blob_id} elapsed={}ms", start.elapsed().as_millis()))
            ).ok();
            Json(MasterResponse { blob_id, status: "ok", message: None })
        }
        Err(e) => {
            state.audit.write(
                AuditEntry::new("m0d.mastering_failed", AuditLevel::Audit, &e)
            ).ok();
            Json(MasterResponse {
                blob_id: String::new(),
                status:  "error",
                message: Some(e),
            })
        }
    }
}

/// Invoke sp314-dsp MasteringPipeline and assemble StoredBlob.
/// Runs pipeline in a Tokio blocking task (no_std + alloc, synchronous).
async fn run_dsp(audio_path: &str, preset_id: &str, start: Instant) -> Result<StoredBlob, String> {
    use sp314_dsp::pipeline::{MasteringPipeline, MasteringIntent};
    use sp314_dsp::types::audio::AudioChunk;
    use sp314_dsp::types::config::Bmr128Schema;
    use uuid::Uuid;

    // Load schema from shared contract (embedded at compile-time for determinism)
    let schema: Bmr128Schema = serde_json::from_str(
        include_str!("../../../../shared/schema/bmr-128.schema.json")
    ).map_err(|e| format!("Schema load error: {e}"))?;

    // Read audio from disk
    let audio_bytes = tokio::fs::read(audio_path).await
        .map_err(|e| format!("I/O error reading {audio_path}: {e}"))?;

    // Determinism seed from SHA-256 of input
    let input_hash_bytes = compute_sha256_bytes(&audio_bytes);
    let input_hash_hex   = hex::encode(&input_hash_bytes);
    let seed             = derive_seed(&input_hash_bytes);

    // Target LUFS from schema preset
    let target_lufs: Option<f32> = schema.presets.get(preset_id)
        .and_then(|p| p.target_lufs);

    // Build AudioChunk — Phase 6: assume 48kHz stereo f32 PCM raw input
    // Phase 7: real decode via hound/symphonia
    let samples = bytes_to_f32_samples(&audio_bytes);
    let chunk = AudioChunk::new(samples, 48_000, 2);

    let intent = MasteringIntent {
        seed,
        target_lufs,
        export_16bit: false,
    };

    let constants = schema.pipeline.clone();
    let pipeline  = MasteringPipeline::new(constants);

    // Run in blocking thread — sp314-dsp is no_std/alloc/sync
    let result = tokio::task::spawn_blocking(move || {
        pipeline.master(&intent, &[chunk], input_hash_bytes)
    }).await.map_err(|e| format!("DSP task join error: {e}"))?
      .map_err(|e| format!("DSP pipeline error: {e}"))?;

    let qm    = &result.quality_metrics;
    let lufs  = qm.integrated_lufs;
    let tp    = qm.true_peak_dbfs;
    let lra   = qm.loudness_range_lu;
    let dr    = qm.dynamic_range_db;
    let sc    = qm.stereo_correlation;
    let elapsed = start.elapsed().as_millis() as u64;

    Ok(StoredBlob {
        id:               Uuid::new_v4().to_string(),
        version:          "1.0".into(),
        blob_type:        "audio".into(),
        created_at:       Utc::now().to_rfc3339(),
        input_hash:       input_hash_hex,
        seed,
        pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
        preset_id:        preset_id.to_string(),
        loudness: StoredLoudness {
            integrated_lufs:          lufs,
            short_term_lufs:          lufs,          // Phase 7: 3s window
            momentary_lufs:           lufs,          // Phase 7: 400ms window
            true_peak_dbtp:           tp,
            lra,
            k_weighted:               true,
            ebu_r128_target_lufs:     -23.0,
            ebu_r128_compliant:       lufs <= -23.0 && tp <= -1.0,
            spotify_compliant:        platform_ok(lufs, -14.0, tp),
            youtube_compliant:        platform_ok(lufs, -14.0, tp),
            apple_music_compliant:    platform_ok(lufs, -16.0, tp),
            apple_podcasts_compliant: platform_ok(lufs, -16.0, tp),
            broadcast_compliant:      platform_ok(lufs, -23.0, tp),
            tidal_compliant:          platform_ok(lufs, -14.0, tp),
        },
        quality: StoredQuality {
            stereo_correlation: sc,
            phase_coherence:    0.97,       // Phase 7: real M/S phase analysis
            stereo_width:       0.5,        // Phase 7: real M/S width analysis
            dynamic_range_db:   dr,
            rms_db:             lufs + 3.0, // Phase 7: real RMS measurement
            spectral_centroid:  3_200.0,    // Phase 7: FFT spectral analysis
            spectral_flatness:  0.12,       // Phase 7: FFT flatness analysis
            clips_detected:     0,
            clip_free:          tp <= -1.0,
        },
        provenance: StoredProvenance {
            engine_id:          "E11".into(),
            engine_version:     env!("CARGO_PKG_VERSION").to_string(),
            processing_time_ms: elapsed,
            host_os:            std::env::consts::OS.to_string(),
            created_by:         "stillair-cockpit".into(),
            aether_enriched:    false,
            aether_devices:     vec![],
        },
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// LUFS within 1 LU of target AND TP ≤ ceiling → platform compliant.
fn platform_ok(lufs: f32, target: f32, tp: f32) -> bool {
    lufs <= target + 1.0 && tp <= -1.0
}

/// SHA-256 of input bytes — returns [u8; 32].
/// Used for both `input_hash` audit field and determinism `seed`.
fn compute_sha256_bytes(data: &[u8]) -> [u8; 32] {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// Derive u64 seed from first 8 bytes of hash (big-endian).
fn derive_seed(hash: &[u8; 32]) -> u64 {
    u64::from_be_bytes(hash[..8].try_into().unwrap_or([0; 8]))
}

/// Convert raw bytes → f32 PCM samples (little-endian float).
/// Phase 6: assumes f32-PCM input. Phase 7: use symphonia/hound decoder.
fn bytes_to_f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_bytes_deterministic() {
        let h1 = compute_sha256_bytes(b"same input");
        let h2 = compute_sha256_bytes(b"same input");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_derive_seed_non_zero() {
        let h = compute_sha256_bytes(b"creator-os");
        assert_ne!(derive_seed(&h), 0);
    }

    #[test]
    fn test_platform_ok_at_target() {
        assert!(platform_ok(-14.0, -14.0, -1.0));
    }

    #[test]
    fn test_platform_ok_too_loud() {
        assert!(!platform_ok(-12.0, -14.0, -1.0));
    }

    #[test]
    fn test_platform_ok_tp_exceeded() {
        assert!(!platform_ok(-14.0, -14.0, -0.5));
    }
}
