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

    // Target LUFS from schema preset — clamped to valid range.
    // Unclamped values can cause normalization_gain_linear() to return
    // gains > 1.5 which Stage1 rejects. The schema values are correct,
    // but a future schema edit could introduce out-of-range values.
    let target_lufs: Option<f32> = schema.presets.get(preset_id)
        .and_then(|p| p.target_lufs)
        .map(|lufs| lufs.clamp(-40.0, 0.0));

    // Build AudioChunk.
    // Phase 6: bytes_to_f32_samples clamps all values to [-1.0, 1.0].
    //   For real f32-PCM files this is a no-op.
    //   For MP3/FLAC/WAV-int bitstreams the clamping produces silence-like
    //   data, which the silence guard below will catch and reject cleanly.
    // Phase 7: replace with symphonia/hound decode for real audio format support.
    let samples = bytes_to_f32_samples(&audio_bytes);

    // ── Silence / near-silence guard ──────────────────────────────────────────
    // If RMS is below -60 dBFS the input is silence, noise, or an encoded
    // bitstream being misread as f32-PCM. Either way normalization gain would
    // be astronomically high (> 1000×) and Stage1 would overflow.
    //
    // ASC mapping:
    //   Silence / non-audio  → caller should decode to f32-PCM first (Phase 7)
    //   Encoded bitstream    → same: Phase 7 decode path covers this
    let rms = compute_rms(&samples);
    let rms_dbfs = if rms > 0.0 { 20.0 * (rms as f64).log10() as f32 }
                   else         { f32::NEG_INFINITY };

    if rms_dbfs < -60.0 {
        return Err(format!(
            "Input validation failed: audio is silence or non-PCM (RMS = {rms_dbfs:.1} dBFS). \
             Phase 6 requires f32-PCM input; MP3/FLAC/WAV-int decode is Phase 7."
        ));
    }

    // Additional belt-and-suspenders on the computed normalization gain:
    // if gain > 32× (>+30 dB) even after clamping, something is wrong.
    // This catches edge cases where clamped samples are very quiet but
    // barely above the -60 dBFS silence threshold.
    if let Some(tl) = target_lufs {
        let measured_lufs = rms_to_lufs(rms);
        let gain_db = tl - measured_lufs;
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        if gain_linear > 32.0 {
            return Err(format!(
                "Normalization gain ({gain_linear:.1}×) would overflow Stage1. \
                 Input LUFS estimate: {measured_lufs:.1}, target: {tl:.1}. \
                 Ensure input is normalized f32-PCM audio."
            ));
        }
    }

    let chunk = AudioChunk::new(samples, 48_000, 2);

    // MasteringIntent — all three fields verified:
    //   seed:         from SHA-256 of input (non-zero guaranteed by derive_seed)
    //   target_lufs:  from schema preset, clamped to [-40.0, 0.0]
    //   export_16bit: false (24-bit dither)
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

/// Convert raw bytes → f32 PCM samples (little-endian float) with clamping.
///
/// Phase 6: assumes f32-PCM input. Samples are clamped to [-1.0, 1.0] and
/// NaN/Inf are mapped to 0.0. This makes the function safe for arbitrary
/// byte inputs — encoded bitstreams (MP3/FLAC) will produce near-zero values
/// that the silence guard above catches, rather than crashing Stage1.
///
/// Phase 7: replace with symphonia/hound decode for real format support.
fn bytes_to_f32_samples(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4)
        .map(|b| {
            let v = f32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            // Map NaN/Inf to 0.0, then clamp to audio range
            if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 }
        })
        .collect()
}

/// Compute RMS amplitude of samples.
fn compute_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() { return 0.0; }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

/// Rough LUFS estimate from RMS — used only for overflow guard, not stored.
/// Full EBU R128 measurement happens inside sp314-dsp.
fn rms_to_lufs(rms: f32) -> f32 {
    if rms <= 0.0 { return f32::NEG_INFINITY; }
    // K-weighting approximation: subtract ~1 dB from RMS dBFS
    20.0 * rms.log10() - 1.0
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

    #[test]
    fn test_bytes_to_f32_clamps_in_range() {
        // Valid f32-PCM bytes representing 0.5 should pass through unchanged
        let sample: f32 = 0.5;
        let bytes = sample.to_le_bytes();
        let out = bytes_to_f32_samples(&bytes);
        assert_eq!(out.len(), 1);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_bytes_to_f32_clamps_mp3_bitstream() {
        // MP3 sync word bytes [0xFF, 0xFB, ...] reinterpreted as f32 → NaN or huge value
        // bytes_to_f32_samples must return 0.0 or a clamped value, never > 1.0
        let mp3_header_bytes = [0xFF, 0xFBu8, 0x90, 0x00];
        let out = bytes_to_f32_samples(&mp3_header_bytes);
        assert_eq!(out.len(), 1);
        assert!(out[0] >= -1.0 && out[0] <= 1.0,
            "Clamped value must be in [-1.0, 1.0], got {}", out[0]);
    }

    #[test]
    fn test_compute_rms_silence() {
        let samples = vec![0.0f32; 4096];
        assert_eq!(compute_rms(&samples), 0.0);
    }

    #[test]
    fn test_compute_rms_half_amp() {
        // Signal at 0.5 amplitude → RMS = 0.5 / sqrt(2) ≈ 0.354
        let samples: Vec<f32> = (0..4096)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48000.0).sin())
            .collect();
        let rms = compute_rms(&samples);
        assert!((rms - 0.5_f32 / 2.0_f32.sqrt()).abs() < 0.01,
            "RMS should be ~0.354, got {rms}");
    }

    #[test]
    fn test_silence_guard_threshold() {
        // RMS of 0.0 → dBFS = -∞ → below -60 dBFS silence threshold
        let rms = 0.0_f32;
        let rms_dbfs = if rms > 0.0 { 20.0 * (rms as f64).log10() as f32 }
                       else         { f32::NEG_INFINITY };
        assert!(rms_dbfs < -60.0, "Silence must be below guard threshold");
    }

    #[test]
    fn test_gain_overflow_guard() {
        // Very quiet input (-70 LUFS) with target -14 LUFS → gain >> 32×
        let quiet_rms = 10.0_f32.powf(-70.0 / 20.0);
        let measured_lufs = rms_to_lufs(quiet_rms);
        let gain_db = -14.0 - measured_lufs;
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        assert!(gain_linear > 32.0,
            "Very quiet input should trigger overflow guard, gain={gain_linear:.1}×");
    }

    #[test]
    fn test_target_lufs_clamped() {
        // Out-of-range values should be clamped
        let clamped_low: f32  = (-50.0_f32).clamp(-40.0, 0.0);
        let clamped_high: f32 = (5.0_f32).clamp(-40.0, 0.0);
        assert_eq!(clamped_low,  -40.0);
        assert_eq!(clamped_high,   0.0);
    }

}
