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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageRecord {
    pub stage: String,
    pub duration_ms: u64,
    pub stage_hash: String, // FNV of stage output
}

fn default_schema_v1() -> u32 {
    1
}

/// Golden Blob as stored by M0.
/// Audio bytes stored separately — only metrics/metadata serialized to JSON.
/// Field contract: golden-blob-spec.md v1.0 §Structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredBlob {
    // Top-level fields — golden-blob-spec.md §Top-level
    pub id: String,
    pub version: String,
    #[serde(rename = "type")]
    pub blob_type: String,
    pub created_at: String,
    pub input_hash: String,
    #[serde(serialize_with = "serialize_u64_as_string")]
    pub seed: u64,
    pub pipeline_version: String,
    pub preset_id: String,

    // Metrics — golden-blob-spec.md §LoudnessMetrics
    pub loudness: StoredLoudness,

    // Quality — golden-blob-spec.md §QualityMetrics
    pub quality: StoredQuality,

    // Provenance — golden-blob-spec.md §Provenance
    pub provenance: StoredProvenance,

    // Spatial telemetry
    #[serde(default)]
    pub spatial: StoredSpatial,

    // Mirror GoldenBlob v2 fields
    #[serde(default = "default_schema_v1")]
    pub schema_version: u32,
    #[serde(default)]
    pub aether_cert: Option<String>,
    #[serde(default)]
    pub aether_persona: Option<String>,
    #[serde(default)]
    pub aether_config: Option<String>,

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

    /// Dead-air diagnostics (bounded, O(1)).
    /// Default = clean (batch path / no monitor).
    #[serde(default)]
    pub dead_air: crate::dsp::signal_health::DeadAirSummary,

    // Audio payload — not serialized to JSON (never sent to frontend).
    // Authority: Amendment A-002 §3 — FORBIDDEN to return raw audio bytes to surface.
    // Phase 10: interleaved f32 LE PCM at 48kHz from MasteringPipeline output.
    #[serde(skip)]
    pub audio_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
    #[serde(skip)]
    pub sample_rate: u32, // always 48000 after Phase 7 decode
    #[serde(skip)]
    pub channels: u16, // stereo = 2
    #[serde(skip)]
    pub num_frames: usize, // actual audio length without tail
}

/// BS.1770-4 canonical values + platform compliance flags.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredLoudness {
    pub integrated_lufs: f32,
    pub short_term_lufs: f32,
    pub momentary_lufs: f32,
    pub true_peak_dbtp: f32,
    pub lra: f32,
    #[serde(default)]
    pub noise_floor_dbfs: Option<f32>,
    pub k_weighted: bool,
    pub ebu_r128_target_lufs: f32,
    pub ebu_r128_compliant: bool,
    pub spotify_compliant: bool,
    pub youtube_compliant: bool,
    pub apple_music_compliant: bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant: bool,
    pub tidal_compliant: bool,
    #[serde(default)]
    pub too_quiet_for_mobile: bool,
    /// ACX audiobook delivery check (sample peak <= -3 dB, RMS -23..-18,
    /// quietest-500ms noise floor <= -60 dB — the Audacity ACX Check
    /// method, validated against an independent oracle 2026-07-30).
    /// None = not measured (non-ACX preset), which is NOT a pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acx_sample_peak_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acx_rms_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acx_noise_floor_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acx_quietest_window_start_frame: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acx_compliant: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredQuality {
    pub stereo_correlation: f32,
    pub phase_coherence: f32,
    pub stereo_width: f32,
    pub dynamic_range_db: f32,
    pub rms_db: f32,
    pub spectral_centroid: f32,
    pub spectral_flatness: f32,
    pub clips_detected: u32,
    pub clip_free: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredProvenance {
    pub engine_id: String,
    pub engine_version: String,
    pub processing_time_ms: u64,
    pub host_os: String,
    pub created_by: String,
    pub aether_enriched: bool,
    pub aether_devices: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StemFingerprints {
    pub voice: String,
    pub drums: String,
    pub bass: String,
    pub harmonics: String,
    pub ambience: String,
    pub pipeline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BandSpatial {
    pub pan_mean: f32,
    pub pan_width: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredSpatial {
    pub low: BandSpatial,
    pub low_mid: BandSpatial,
    pub mid: BandSpatial,
    pub high_mid: BandSpatial,
    pub high: BandSpatial,
}

/// Thread-safe in-memory blob store.
/// Phase 6: DashMap-equivalent via Arc<Mutex<HashMap>>.
/// Phase 7: replace with content-addressed file store.
#[derive(Clone)]
pub struct BlobStore {
    inner: Arc<Mutex<HashMap<String, StoredBlob>>>,
}

impl BlobStore {
    /// DISK BACKSTOP, not lifecycle policy:
    /// When sessions exist (Mastering Tinder / Git-for-Master), the AudioRepo/session
    /// owns the raw's Arc and project-close is the real drop point; tinder variant
    /// commits live in AudioRepo (params, not blobs) and are NEVER affected by this cap.
    /// The Arc-sharing design is what makes tinder O(1) per swipe: all variants reference
    /// the same raw ManagedPcm. This cap bounds intermediate disk leaks before the
    /// session architecture ships.
    const MAX_BLOBS: usize = 16;

    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&self, blob: StoredBlob) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(blob.id.clone(), blob);

            // Evict oldest when over cap (created_at is chrono RFC 3339 —
            // ISO 8601 with UTC, lexicographically sortable).
            if map.len() > Self::MAX_BLOBS {
                let oldest_id = map
                    .iter()
                    .min_by_key(|(_, b)| &b.created_at)
                    .map(|(id, _)| id.clone());
                if let Some(id) = oldest_id {
                    map.remove(&id);
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<StoredBlob> {
        self.inner.lock().ok()?.get(id).cloned()
    }
}

impl Default for BlobStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_blob(id: &str) -> StoredBlob {
        StoredBlob {
            id: id.to_string(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-04-15T00:00:00Z".into(),
            input_hash: "aabbccdd".into(),
            seed: 1,
            pipeline_version: "0.4.0".into(),
            preset_id: "spotify".into(),
            loudness: StoredLoudness {
                integrated_lufs: -14.0,
                short_term_lufs: -13.5,
                momentary_lufs: -12.0,
                true_peak_dbtp: -1.0,
                lra: 8.0,
                k_weighted: true,
                ebu_r128_target_lufs: -23.0,
                ebu_r128_compliant: false,
                spotify_compliant: true,
                youtube_compliant: true,
                apple_music_compliant: false,
                apple_podcasts_compliant: false,
                broadcast_compliant: false,
                tidal_compliant: true,
                ..Default::default()
            },
            quality: StoredQuality {
                stereo_correlation: 0.94,
                phase_coherence: 0.97,
                stereo_width: 0.74,
                dynamic_range_db: 9.5,
                rms_db: -16.0,
                spectral_centroid: 3_200.0,
                spectral_flatness: 0.12,
                clips_detected: 0,
                clip_free: true,
            },
            provenance: StoredProvenance {
                engine_id: "E11".into(),
                engine_version: "0.4.0".into(),
                processing_time_ms: 1_234,
                host_os: "linux-x86_64".into(),
                created_by: "test".into(),
                aether_enriched: false,
                aether_devices: vec![],
            },
            spatial: StoredSpatial::default(),
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

/// ΒΗΜΑ 1 του certificate-as-type.
/// Παράλληλα με το StoredBlob. Κανείς δεν τα χρησιμοποιεί
/// ακόμα. northstar §Σ.
///
/// ΤΟ ΠΡΟΒΛΗΜΑ ΠΟΥ ΛΥΝΟΥΝ: το StoredBlob λειτουργεί ως
/// C-union — τρία σημεία το κατασκευάζουν με
/// ..Default::default() γεμίζοντας 13 πεδία με μηδενικά,
/// και ένα από τα πέντε παραδοτέα (spatial) φεύγει χωρίς
/// καμία απόδειξη.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBlobCore {
    pub id: String,
    pub version: String,
    pub blob_type: String,
    pub created_at: String,
    pub input_hash: String,
    pub seed: u64,
    pub pipeline_version: String,
    /// = 0. ΣΠΑΕΙ ΧΩΡΙΣ MIGRATION μέχρι το πρώτο
    /// public release. northstar, schema_version.
    pub schema_version: u32,
    /// ΠΡΟΣΩΡΙΝΟ: σήμερα το preset_id κουβαλάει ΤΕΣΣΕΡΑ
    /// είδη πραγμάτων (delivery target, flavour, routing
    /// mode, typos/casing). Μένει String μέχρι το ΜΗΤΡΩΟ
    /// να το σπάσει σε delivery_target / flavour /
    /// routing_mode.
    /// ΜΗΝ προσθέσεις λογική που το θεωρεί ενιαίο.
    pub preset_id: String,
    pub pcm_blake3: Option<String>,
    pub cert_signature: Option<String>,
    #[serde(skip)]
    pub audio_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
    #[serde(skip)]
    pub sample_rate: u32,
    #[serde(skip)]
    pub channels: u16,
    #[serde(skip)]
    pub num_frames: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlobVariant {
    /// Μετρήθηκε. Έχει απόδειξη.
    Certified {
        loudness: crate::blob_store::StoredLoudness,
        quality: crate::blob_store::StoredQuality,
        provenance: crate::blob_store::StoredProvenance,
        spatial: crate::blob_store::StoredSpatial,
        stem_fingerprints: Option<crate::blob_store::StemFingerprints>,
        processing_timeline: Vec<crate::blob_store::StageRecord>,
        dead_air: crate::dsp::signal_health::DeadAirSummary,
        aether_cert: Option<String>,
        aether_persona: Option<String>,
        aether_config: Option<String>,
        qr_base64: Option<String>,
    },
    /// ΔΕΝ μετρήθηκε. ΧΡΕΟΣ με όνομα.
    /// ΔΕΝ παραδίδεται σε χρήστη χωρίς ρητή μετατροπή.
    Uncertified { reason: UncertifiedReason },
}

/// Κάθε λόγος είναι ΧΡΕΟΣ με συνθήκη λήξης.
/// grep UncertifiedReason = η λίστα του χρέους.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum UncertifiedReason {
    /// ΧΡΕΟΣ: το spatial_conformance_path δεν μετράει
    /// lufs/true_peak/lra — δουλεύει in-place σε mmap
    /// χωρίς telemetry pass.
    /// ΣΒΗΝΕΙ όταν αποκτήσει O(1) analyzer. northstar §Σ.
    SpatialPathHasNoTelemetry,

    /// ΧΡΕΟΣ: το deliver.rs::build_minimal_blob ΦΤΙΑΧΝΕΙ
    /// blob αντί να ΔΙΑΒΑΖΕΙ το υπάρχον.
    /// ΣΒΗΝΕΙ όταν το delivery διαβάζει από το store.
    DeliveryManifestStub,

    /// ΧΡΕΟΣ: το conductor λαμβάνει BatchTrackOutput
    /// (7 πεδία) μέσω async καναλιού, όχι πλήρη StoredBlob.
    /// ΣΒΗΝΕΙ όταν το run_batch() γίνει συνάρτηση.
    /// northstar §Β.
    ProxyForAlbumContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBlobV2 {
    pub core: StoredBlobCore,
    pub variant: BlobVariant,
}

/// Το παραδοτέο. Ο τύπος που εγγυάται ότι κάθε αρχείο
/// φεύγει με ό,τι το συνοδεύει.
#[derive(Debug)]
pub enum Deliverable {
    Single(StoredBlobV2),
    Album {
        cert: crate::domain::nodes::album_certificate_node::AlbumCertificate,
        tracks: Vec<StoredBlobV2>,
        /// ΠΛΑΙΣΙΟ: χωρίς αυτό το track #2 δεν
        /// αναπαράγεται μόνο του. Δόγμα Α.
        contexts: Vec<TrackContext>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackContext {
    pub position_in_batch: usize,

    /// ΧΡΕΟΣ (recon 2026-08-11): ο conductor κρατάει ΜΟΝΟ
    /// το fatigue_detected: bool σε Vec<bool> fatigue_map.
    /// Οι πραγματικοί multipliers του EarFatigueDelta
    /// ΠΕΤΙΟΝΤΑΙ μετά την εφαρμογή
    /// (conductor.rs:476-484).
    /// Μένουν None μέχρι το run_batch() να μεταφέρει
    /// αυτούσιο το EarFatigueDelta. northstar §Β.
    pub ear_fatigue_ducking_mult: Option<f32>,
    pub ear_fatigue_width_mult: Option<f32>,

    /// Χωρίς αυτά το track #2 δεν αναπαράγεται μόνο του.
    /// Το EarFatigue διαβάζει integrated_lufs ΚΑΙ
    /// transient_density του index-1. Δόγμα Α.
    pub prev_track_lufs: Option<f32>,
    pub prev_track_transient_density: Option<f32>,
}
