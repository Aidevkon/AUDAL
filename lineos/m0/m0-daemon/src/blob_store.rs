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

/// ΠΡΟΣΩΡΙΝΟ — βήμα 6α/7. Ίδιες υπογραφές με το
/// StoredBlobV2 ώστε τα ~90 call sites να
/// μεταναστεύσουν ΠΡΙΝ αλλάξει ο τύπος επιστροφής του
/// run_dsp (βήμα 7α).
///
/// ΠΑΝΤΑ Some: ο παλιός τύπος δεν ξέρει από variants.
///
/// ΠΡΟΣΟΧΗ — ΔΕΝ ΕΙΝΑΙ ΛΑΘΟΣ: methods και fields ζουν
/// σε διαφορετικά namespaces στη Rust, οπότε
/// blob.loudness (πεδίο) και blob.loudness() (μέθοδος)
/// συνυπάρχουν ΣΚΟΠΙΜΑ όσο διαρκεί η μετανάστευση.
///
/// ΣΒΗΝΕΙ στο βήμα 7β μαζί με τον παλιό τύπο.
impl StoredBlob {
    pub fn loudness(&self) -> Option<&StoredLoudness> {
        Some(&self.loudness)
    }
    pub fn quality(&self) -> Option<&StoredQuality> {
        Some(&self.quality)
    }
    pub fn provenance(&self) -> Option<&StoredProvenance> {
        Some(&self.provenance)
    }
    pub fn spatial(&self) -> Option<&StoredSpatial> {
        Some(&self.spatial)
    }
    pub fn stem_fingerprints(&self) -> Option<&StemFingerprints> {
        self.stem_fingerprints.as_ref()
    }
    pub fn processing_timeline(&self) -> Option<&[StageRecord]> {
        Some(self.processing_timeline.as_slice())
    }
    pub fn qr_base64(&self) -> Option<&str> {
        self.qr_base64.as_deref()
    }
    pub fn aether_cert(&self) -> Option<&str> {
        self.aether_cert.as_deref()
    }
    pub fn aether_persona(&self) -> Option<&str> {
        self.aether_persona.as_deref()
    }
    pub fn aether_config(&self) -> Option<&str> {
        self.aether_config.as_deref()
    }
    pub fn is_certified(&self) -> bool {
        true
    }
    pub fn uncertified_reason(&self) -> Option<UncertifiedReason> {
        None
    }
    /// ΠΛΗΡΕΣ PATH — το DeadAirSummary ΔΕΝ είναι σε scope
    /// στο blob_store.rs. Γράψ' το αυτούσιο, ΜΗΝ προσθέσεις
    /// use statement.
    pub fn dead_air(&self) -> Option<&crate::dsp::signal_health::DeadAirSummary> {
        Some(&self.dead_air)
    }
    /// Το κενό slice είναι ΣΩΣΤΗ συμπεριφορά όπου ένα for
    /// που δεν τρέχει ή ένα .len()==0 δεν κρύβει τίποτα.
    /// Το _or_empty στο όνομα κάνει την απώλεια ΡΗΤΗ στο
    /// call site — ο αναγνώστης βλέπει ότι κάποιος
    /// ΑΠΟΦΑΣΙΣΕ να μη διακρίνει, δεν το ανακαλύπτει
    /// διαβάζοντας την υλοποίηση.
    ///
    /// ΜΗΝ το χρησιμοποιείς σε artifact χρήστη (PDF, PNG,
    /// sidecar) — εκεί το κενό timeline είναι ΣΙΩΠΗΛΗ
    /// ΠΑΡΑΛΕΙΨΗ ΕΝΟΤΗΤΑΣ. Χρησιμοποίησε τον κύριο
    /// accessor με ρητό if let Some(..).
    pub fn timeline_or_empty(&self) -> &[StageRecord] {
        self.processing_timeline.as_slice()
    }
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

    #[test]
    fn bridge_certified_loses_nothing() {
        let v2 = crate::blob_store::StoredBlobV2 {
            core: crate::blob_store::StoredBlobCore {
                id: "core-id".into(),
                version: "2.0".into(),
                blob_type: "video".into(),
                created_at: "now".into(),
                input_hash: "hash".into(),
                seed: 42,
                pipeline_version: "v1".into(),
                schema_version: 3,
                preset_id: "preset".into(),
                pcm_blake3: Some("blake".into()),
                cert_signature: Some("sig".into()),
                audio_path: std::sync::Arc::new(lineos_types::audio::ManagedPcm::default()),
                sample_rate: 44100,
                channels: 1,
                num_frames: 100,
            },
            variant: crate::blob_store::BlobVariant::Certified {
                loudness: crate::blob_store::StoredLoudness { integrated_lufs: -10.0, ..Default::default() },
                quality: crate::blob_store::StoredQuality { stereo_correlation: 0.5, ..Default::default() },
                provenance: crate::blob_store::StoredProvenance { engine_id: "E2".into(), ..Default::default() },
                spatial: crate::blob_store::StoredSpatial { low: crate::blob_store::BandSpatial { pan_mean: 0.1, pan_width: 0.2 }, ..Default::default() },
                stem_fingerprints: Some(crate::blob_store::StemFingerprints::default()),
                processing_timeline: vec![crate::blob_store::StageRecord::default()],
                dead_air: crate::dsp::signal_health::DeadAirSummary { noise_floor_dbfs: Some(-80.0), ..Default::default() },
                aether_cert: Some("cert".into()),
                aether_persona: Some("persona".into()),
                aether_config: Some("config".into()),
                qr_base64: Some("qr".into()),
            },
        };
        let blob: crate::blob_store::StoredBlob = v2.into();
        assert_eq!(blob.id, "core-id");
        assert_eq!(blob.version, "2.0");
        assert_eq!(blob.blob_type, "video");
        assert_eq!(blob.created_at, "now");
        assert_eq!(blob.input_hash, "hash");
        assert_eq!(blob.seed, 42);
        assert_eq!(blob.pipeline_version, "v1");
        assert_eq!(blob.schema_version, 3);
        assert_eq!(blob.preset_id, "preset");
        assert_eq!(blob.pcm_blake3, Some("blake".into()));
        assert_eq!(blob.cert_signature, Some("sig".into()));
        assert_eq!(blob.sample_rate, 44100);
        assert_eq!(blob.channels, 1);
        assert_eq!(blob.num_frames, 100);

        assert_eq!(blob.loudness.integrated_lufs, -10.0);
        assert_eq!(blob.quality.stereo_correlation, 0.5);
        assert_eq!(blob.provenance.engine_id, "E2");
        assert_eq!(blob.spatial.low.pan_mean, 0.1);
        assert!(blob.stem_fingerprints.is_some());
        assert_eq!(blob.processing_timeline.len(), 1);
        assert_eq!(blob.dead_air.noise_floor_dbfs, Some(-80.0));
        assert_eq!(blob.aether_cert, Some("cert".into()));
        assert_eq!(blob.aether_persona, Some("persona".into()));
        assert_eq!(blob.aether_config, Some("config".into()));
        assert_eq!(blob.qr_base64, Some("qr".into()));
    }

    #[test]
    fn bridge_uncertified_is_default() {
        let v2 = crate::blob_store::StoredBlobV2 {
            core: crate::blob_store::StoredBlobCore {
                id: "core-id".into(),
                version: "2.0".into(),
                blob_type: "video".into(),
                created_at: "now".into(),
                input_hash: "hash".into(),
                seed: 42,
                pipeline_version: "v1".into(),
                schema_version: 3,
                preset_id: "preset".into(),
                pcm_blake3: Some("blake".into()),
                cert_signature: Some("sig".into()),
                audio_path: std::sync::Arc::new(lineos_types::audio::ManagedPcm::default()),
                sample_rate: 44100,
                channels: 1,
                num_frames: 100,
            },
            variant: crate::blob_store::BlobVariant::Uncertified { reason: crate::blob_store::UncertifiedReason::SpatialPathHasNoTelemetry },
        };
        let blob: crate::blob_store::StoredBlob = v2.into();
        assert_eq!(blob.id, "core-id");
        assert_eq!(blob.seed, 42);
        
        assert_eq!(blob.loudness.integrated_lufs, 0.0);
        assert_eq!(blob.aether_cert, None);
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

    /// Το blob ΔΕΝ είναι πηγή αλήθειας — υπάρχει ΜΟΝΟ για
    /// να μεταφέρει audio_path και channels στην
    /// export_mp3_acx. Οι πραγματικές μετρήσεις του
    /// delivery έρχονται από ΕΠΑΝΑΜΕΤΡΗΣΗ του τελικού
    /// αρχείου (AcxCheckReport) και μπαίνουν στο
    /// manifest.json.
    ///
    /// ΓΙΑΤΙ ΜΕΤΡΑΕΙ: τρεις συναρτήσεις δέχονται
    /// &StoredBlob και παράγουν artifact για χρήστη —
    /// write_sidecar · generate_silent_certificate ·
    /// generate_certificate_png. ΚΑΜΙΑ δεν καλείται από το
    /// delivery σήμερα. Αν προστεθεί, ο τύπος εμποδίζει
    /// διαρροή LUFS=0.0 / TP=0.0 / compliance=false /
    /// id="delivery" σε certificate χρήστη.
    ///
    /// ΣΒΗΝΕΙ όταν το delivery διαβάζει πραγματικό blob.
    /// ΑΠΑΙΤΕΙ: PlanEntry.blob_id + το StoredBlob να
    /// γράφεται στη SurrealDB (η βάση ΥΠΑΡΧΕΙ και είναι
    /// persistent — γράφει ήδη Project/Session/Track,
    /// απλώς ΟΧΙ blobs) + λύση για το ότι η βάση είναι
    /// async ενώ το run_deliver_core τρέχει σε
    /// spawn_blocking. northstar §Π.
    TransportOnlyNotASource,

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

impl StoredBlobV2 {
    /// Some μόνο αν Certified. None αν Uncertified.
    /// ΣΚΟΠΙΜΑ Option: ο caller ΠΡΕΠΕΙ να αντιμετωπίσει
    /// την περίπτωση που δεν υπάρχει μέτρηση.
    pub fn loudness(&self) -> Option<&StoredLoudness> {
        match &self.variant {
            BlobVariant::Certified { loudness, .. } => Some(loudness),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn quality(&self) -> Option<&StoredQuality> {
        match &self.variant {
            BlobVariant::Certified { quality, .. } => Some(quality),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn provenance(&self) -> Option<&StoredProvenance> {
        match &self.variant {
            BlobVariant::Certified { provenance, .. } => Some(provenance),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn spatial(&self) -> Option<&StoredSpatial> {
        match &self.variant {
            BlobVariant::Certified { spatial, .. } => Some(spatial),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn stem_fingerprints(&self) -> Option<&StemFingerprints> {
        match &self.variant {
            BlobVariant::Certified { stem_fingerprints, .. } => stem_fingerprints.as_ref(),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn qr_base64(&self) -> Option<&str> {
        match &self.variant {
            BlobVariant::Certified { qr_base64, .. } => qr_base64.as_deref(),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn aether_cert(&self) -> Option<&str> {
        match &self.variant {
            BlobVariant::Certified { aether_cert, .. } => aether_cert.as_deref(),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn aether_persona(&self) -> Option<&str> {
        match &self.variant {
            BlobVariant::Certified { aether_persona, .. } => aether_persona.as_deref(),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    pub fn aether_config(&self) -> Option<&str> {
        match &self.variant {
            BlobVariant::Certified { aether_config, .. } => aether_config.as_deref(),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    /// ΠΛΗΡΕΣ PATH — το DeadAirSummary ΔΕΝ είναι σε scope
    /// στο blob_store.rs. Γράψ' το αυτούσιο, ΜΗΝ προσθέσεις
    /// use statement (θα ήταν αλλαγή σε υπάρχοντα κώδικα).
    pub fn dead_air(&self) -> Option<&crate::dsp::signal_health::DeadAirSummary> {
        match &self.variant {
            BlobVariant::Certified { dead_air, .. } => Some(dead_air),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    /// ΠΡΟΣΟΧΗ ΣΗΜΑΣΙΟΛΟΓΙΑΣ:
    ///   None       = ΔΕΝ μετρήθηκε (Uncertified)
    ///   Some(&[])  = μετρήθηκε, κανένα στάδιο
    /// Είναι ΔΙΑΦΟΡΕΤΙΚΑ. Μην τα συγχέεις.
    pub fn processing_timeline(&self) -> Option<&[StageRecord]> {
        match &self.variant {
            BlobVariant::Certified { processing_timeline, .. } => Some(processing_timeline.as_slice()),
            BlobVariant::Uncertified { .. } => None,
        }
    }

    /// Ρητός έλεγχος πριν από export. Το Uncertified
    /// ΔΕΝ παραδίδεται σε χρήστη χωρίς συνειδητή απόφαση.
    pub fn is_certified(&self) -> bool {
        matches!(self.variant, BlobVariant::Certified { .. })
    }

    /// None αν Certified. Some(reason) αν όχι — ώστε ο
    /// caller να μπορεί να πει ΓΙΑΤΙ λείπει η απόδειξη.
    /// By value: το UncertifiedReason είναι Copy (βήμα 1).
    pub fn uncertified_reason(&self) -> Option<UncertifiedReason> {
        match &self.variant {
            BlobVariant::Certified { .. } => None,
            BlobVariant::Uncertified { reason } => Some(*reason),
        }
    }

    /// Το κενό slice είναι ΣΩΣΤΗ συμπεριφορά όπου ένα for
    /// που δεν τρέχει ή ένα .len()==0 δεν κρύβει τίποτα.
    /// Το _or_empty στο όνομα κάνει την απώλεια ΡΗΤΗ στο
    /// call site — ο αναγνώστης βλέπει ότι κάποιος
    /// ΑΠΟΦΑΣΙΣΕ να μη διακρίνει, δεν το ανακαλύπτει
    /// διαβάζοντας την υλοποίηση.
    ///
    /// ΜΗΝ το χρησιμοποιείς σε artifact χρήστη (PDF, PNG,
    /// sidecar) — εκεί το κενό timeline είναι ΣΙΩΠΗΛΗ
    /// ΠΑΡΑΛΕΙΨΗ ΕΝΟΤΗΤΑΣ. Χρησιμοποίησε τον κύριο
    /// accessor με ρητό if let Some(..).
    pub fn timeline_or_empty(&self) -> &[StageRecord] {
        match &self.variant {
            BlobVariant::Certified { processing_timeline, .. } => processing_timeline.as_slice(),
            BlobVariant::Uncertified { .. } => &[],
        }
    }
}

/// ΠΡΟΣΩΡΙΝΟ — βήμα 3/7. ΣΒΗΝΕΙ στο βήμα 7 όταν φύγει
/// το παλιό StoredBlob.
///
/// Το Certified γεμίζει τα πεδία ένα προς ένα.
/// Το Uncertified τα αφήνει Default — δηλαδή ΞΑΝΑΓΕΜΙΖΕΙ
/// με μηδενικά, που είναι ΑΚΡΙΒΩΣ αυτό που προσπαθούμε
/// να σταματήσουμε. Γι' αυτό η γέφυρα είναι προσωρινή
/// και γι' αυτό ΔΕΝ επιτρέπεται νέος caller της.
///
/// ΜΗΝ γράψεις From<StoredBlob> for StoredBlobV2 —
/// η αντίστροφη κατεύθυνση θα νομιμοποιούσε το παλιό.
impl From<StoredBlobV2> for StoredBlob {
    fn from(v2: StoredBlobV2) -> Self {
        match v2.variant {
            BlobVariant::Certified {
                loudness,
                quality,
                provenance,
                spatial,
                stem_fingerprints,
                processing_timeline,
                dead_air,
                aether_cert,
                aether_persona,
                aether_config,
                qr_base64,
            } => StoredBlob {
                id: v2.core.id,
                version: v2.core.version,
                blob_type: v2.core.blob_type,
                created_at: v2.core.created_at,
                input_hash: v2.core.input_hash,
                seed: v2.core.seed,
                pipeline_version: v2.core.pipeline_version,
                preset_id: v2.core.preset_id,
                schema_version: v2.core.schema_version,
                pcm_blake3: v2.core.pcm_blake3,
                cert_signature: v2.core.cert_signature,
                audio_path: v2.core.audio_path,
                sample_rate: v2.core.sample_rate,
                channels: v2.core.channels,
                num_frames: v2.core.num_frames,
                
                loudness,
                quality,
                provenance,
                spatial,
                stem_fingerprints,
                processing_timeline,
                dead_air,
                aether_cert,
                aether_persona,
                aether_config,
                qr_base64,
            },
            BlobVariant::Uncertified { .. } => StoredBlob {
                id: v2.core.id,
                version: v2.core.version,
                blob_type: v2.core.blob_type,
                created_at: v2.core.created_at,
                input_hash: v2.core.input_hash,
                seed: v2.core.seed,
                pipeline_version: v2.core.pipeline_version,
                preset_id: v2.core.preset_id,
                schema_version: v2.core.schema_version,
                pcm_blake3: v2.core.pcm_blake3,
                cert_signature: v2.core.cert_signature,
                audio_path: v2.core.audio_path,
                sample_rate: v2.core.sample_rate,
                channels: v2.core.channels,
                num_frames: v2.core.num_frames,
                ..Default::default()
            },
        }
    }
}
