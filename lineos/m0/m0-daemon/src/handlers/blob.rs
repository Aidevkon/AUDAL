//! GET /blob/:id — fetch Golden Blob as JSON.
//! Authority: Phase 6 task-decomposition P6-003
//!
//! Returns StoredBlob as JSON. 404 if not found.
//! FORBIDDEN: Returning raw audio bytes (audio stays in M0 storage).
//! FORBIDDEN: serde_json::Value in response type.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::app_state::AppState;
use crate::blob_store::StoredBlob;

/// GET /blob/:id — return Golden Blob metrics as JSON.
/// Audio bytes are NOT returned — Cockpit receives metrics only.
pub async fn get_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<StoredBlob>, StatusCode> {
    state
        .blob_store
        .get(&id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use crate::blob_store::{
        BlobStore, StoredBlob, StoredLoudness, StoredProvenance, StoredQuality,
    };

    fn stub_blob(id: &str) -> StoredBlob {
        StoredBlob {
            id: id.to_string(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            input_hash: "aabb".into(),
            seed: 1,
            pipeline_version: "0.4.0".into(),
            preset_id: "spotify".into(),
            loudness: StoredLoudness {
                integrated_lufs: -14.0,
                short_term_lufs: -13.5,
                momentary_lufs: -12.0,
                true_peak_dbtp: -1.0,
                lra: 8.0,
                noise_floor_dbfs: None,
                k_weighted: true,
                ebu_r128_target_lufs: -23.0,
                ebu_r128_compliant: false,
                spotify_compliant: true,
                youtube_compliant: true,
                apple_music_compliant: false,
                apple_podcasts_compliant: false,
                broadcast_compliant: false,
                tidal_compliant: true,
            },
            quality: StoredQuality {
                stereo_correlation: 0.94,
                phase_coherence: 0.97,
                stereo_width: 0.74,
                dynamic_range_db: 9.5, // TODO: wire real DR when this path carries trunk metrics (register item)
                rms_db: -16.0,
                spectral_centroid: 3200.0,
                spectral_flatness: 0.12,
                clips_detected: 0,
                clip_free: true,
            },
            provenance: StoredProvenance {
                engine_id: "test".into(),
                engine_version: "1.0".into(),
                processing_time_ms: 0,
                host_os: "linux".into(),
                created_by: "test".into(),
                aether_enriched: false,
                aether_devices: vec![],
            },
            spatial: crate::blob_store::StoredSpatial::default(),
            schema_version: 1,
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            stem_fingerprints: None,
            qr_base64: None,
            pcm_blake3: None,
            cert_signature: None,
            processing_timeline: vec![],
            dead_air: Default::default(),
            audio_path: Default::default(), // ManagedPcm::default() — no file to delete
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000,
        }
    }

    #[test]
    fn test_stored_blob_json_no_audio_bytes() {
        // Verify blob serializes without audio bytes field
        let blob = stub_blob("abc-123");
        let json = serde_json::to_string(&blob).unwrap();
        assert!(!json.contains("flac_bytes"), "No audio bytes in JSON");
        assert!(!json.contains("audio_bytes"), "No audio bytes in JSON");
        assert!(json.contains("\"id\":\"abc-123\""));
        assert!(json.contains("\"integrated_lufs\""));
    }

    #[test]
    fn test_blob_store_returns_none_for_unknown_id() {
        let store = BlobStore::new();
        assert!(store.get("unknown-id").is_none());
    }
}
