//! Blob store — in-memory content-addressed store for Golden Blobs.
//! Production: replaced by content-addressed file storage in Phase 7.
//! Phase 6: in-memory only. Blobs are dropped when m0d restarts.
//! Authority: golden-blob-spec.md §Lifecycle

use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Serialize a u64 as a JSON string to preserve precision in JavaScript.
/// JS numbers are IEEE 754 doubles — u64 > 2^53 loses precision as a bare number.
fn serialize_u64_as_string<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

/// Golden Blob as stored by M0.
/// Audio bytes stored separately — only metrics/metadata serialized to JSON.
/// Field contract: golden-blob-spec.md v1.0 §Structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBlob {
    // Top-level fields — golden-blob-spec.md §Top-level
    pub id:               String,
    pub version:          String,
    #[serde(rename = "type")]
    pub blob_type:        String,
    pub created_at:       String,
    pub input_hash:       String,
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub seed:             u64,
    pub pipeline_version: String,
    pub preset_id:        String,

    // Metrics — golden-blob-spec.md §LoudnessMetrics
    pub loudness: StoredLoudness,

    // Quality — golden-blob-spec.md §QualityMetrics
    pub quality: StoredQuality,

    // Provenance — golden-blob-spec.md §Provenance
    pub provenance: StoredProvenance,

    // Audio payload — not serialized to JSON (never sent to frontend).
    // Authority: Amendment A-002 §3 — FORBIDDEN to return raw audio bytes to surface.
    // Phase 10: interleaved f32 LE PCM at 48kHz from MasteringPipeline output.
    #[serde(skip)]
    pub audio_bytes:  Vec<u8>,   // f32 LE PCM, always 48000 Hz
    #[serde(skip)]
    pub sample_rate:  u32,       // always 48000 after Phase 7 decode
    #[serde(skip)]
    pub channels:     u16,       // stereo = 2
}

/// BS.1770-4 canonical values + platform compliance flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLoudness {
    pub integrated_lufs:          f32,
    pub short_term_lufs:          f32,
    pub momentary_lufs:           f32,
    pub true_peak_dbtp:           f32,
    pub lra:                      f32,
    pub k_weighted:               bool,
    pub ebu_r128_target_lufs:     f32,
    pub ebu_r128_compliant:       bool,
    pub spotify_compliant:        bool,
    pub youtube_compliant:        bool,
    pub apple_music_compliant:    bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant:      bool,
    pub tidal_compliant:          bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredQuality {
    pub stereo_correlation: f32,
    pub phase_coherence:    f32,
    pub stereo_width:       f32,
    pub dynamic_range_db:   f32,
    pub rms_db:             f32,
    pub spectral_centroid:  f32,
    pub spectral_flatness:  f32,
    pub clips_detected:     u32,
    pub clip_free:          bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProvenance {
    pub engine_id:            String,
    pub engine_version:       String,
    pub processing_time_ms:   u64,
    pub host_os:              String,
    pub created_by:           String,
    pub aether_enriched:      bool,
    pub aether_devices:       Vec<String>,
}

/// Thread-safe in-memory blob store.
/// Phase 6: DashMap-equivalent via Arc<Mutex<HashMap>>.
/// Phase 7: replace with content-addressed file store.
#[derive(Clone)]
pub struct BlobStore {
    inner: Arc<Mutex<HashMap<String, StoredBlob>>>,
}

impl BlobStore {
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(HashMap::new())) }
    }

    pub fn insert(&self, blob: StoredBlob) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(blob.id.clone(), blob);
        }
    }

    pub fn get(&self, id: &str) -> Option<StoredBlob> {
        self.inner.lock().ok()?.get(id).cloned()
    }
}

impl Default for BlobStore {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_blob(id: &str) -> StoredBlob {
        StoredBlob {
            id:               id.to_string(),
            version:          "1.0".into(),
            blob_type:        "audio".into(),
            created_at:       "2026-04-15T00:00:00Z".into(),
            input_hash:       "aabbccdd".into(),
            seed:             1,
            pipeline_version: "0.4.0".into(),
            preset_id:        "spotify".into(),
            loudness: StoredLoudness {
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
            quality: StoredQuality {
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
            provenance: StoredProvenance {
                engine_id:          "E11".into(),
                engine_version:     "0.4.0".into(),
                processing_time_ms: 1_234,
                host_os:            "linux-x86_64".into(),
                created_by:         "test".into(),
                aether_enriched:    false,
                aether_devices:     vec![],
            },
            audio_bytes:  vec![],   // empty for tests
            sample_rate:  48000,
            channels:     2,
        }
    }

    #[test]
    fn test_blob_store_insert_get() {
        let store = BlobStore::new();
        store.insert(stub_blob("blob-001"));
        let retrieved = store.get("blob-001").unwrap();
        assert_eq!(retrieved.id, "blob-001");
        assert!((retrieved.loudness.integrated_lufs - (-14.0)).abs() < 1e-6);
    }

    #[test]
    fn test_blob_store_get_missing_returns_none() {
        let store = BlobStore::new();
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_stored_blob_serializes_to_json() {
        let blob = stub_blob("test-uuid");
        let json = serde_json::to_string(&blob).unwrap();
        assert!(json.contains("\"id\":\"test-uuid\""));
        assert!(json.contains("\"type\":\"audio\""));
        assert!(json.contains("\"integrated_lufs\""));
    }
}
