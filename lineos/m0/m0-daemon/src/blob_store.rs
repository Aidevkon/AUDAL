//! Blob store — in-memory content-addressed store for Golden Blobs.
//! Production: replaced by content-addressed file storage in Phase 7.
//! Phase 6: in-memory only. Blobs are dropped when m0d restarts.
//! Authority: golden-blob-spec.md §Lifecycle

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};



#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageRecord {
    pub stage: String,
    pub duration_ms: u64,
    pub stage_hash: String, // FNV of stage output
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
    //
    // ── ΤΑ ΤΕΣΣΕΡΑ acx_* ΜΕΤΡΟΥΝ ΤΟ INPUT ──────────
    //
    // ΟΧΙ το παραδοτέο. Προέρχονται από
    // run_trunk_metrics_with_acx στο trunk pass
    // (dsp_pipeline.rs:457-466), που τρέχει ΠΡΙΝ το
    // render — πριν το gain staging, πριν το limiter,
    // πριν το resample σε 44.1k και τον downmix σε mono.
    //
    // ΜΕΤΡΗΜΕΝΟ (e2e_acx_certificate): τα νούμερα του
    // certificate είναι ΤΑΥΤΟΣΗΜΑ με το
    // [TRUNK-episode] log. Δεν είναι σύμπτωση — είναι
    // αντιγραφή.
    //
    // Το παραδοτέο μετριέται ΞΑΝΑ στο export_mp3_acx,
    // μετά από RMS window correction και peak trim.
    // Εκείνη η μέτρηση πάει ΜΟΝΟ στο manifest.json του
    // delivery· εδώ δεν φτάνει ποτέ.
    //
    // ΓΙΑΤΙ ΔΕΝ ΔΙΟΡΘΩΝΕΤΑΙ ΕΔΩ: το certificate γράφεται
    // στο certificate_node, ΠΡΙΝ το export. Το παραδοτέο
    // δεν υπάρχει ακόμα. Χρειάζεται certificate που
    // ΕΝΗΜΕΡΩΝΕΤΑΙ, όχι snapshot.
    //
    // ΤΟ INPUT ΕΧΕΙ ΑΞΙΑ: το noise floor ΠΡΕΠΕΙ να
    // μετρηθεί εδώ — αν ο θόρυβος του πηγαίου είναι πολύ
    // ψηλά, δεν σώζεται με mastering (βλ. σχόλιο στο
    // DeliverySpec::max_noise_floor_db). Το πρόβλημα δεν
    // είναι ΟΤΙ μετράμε το input· είναι ότι τα πεδία
    // ΔΕΝ ΛΕΝΕ ποιο αρχείο μέτρησαν.
    //
    // Ένας narrator που διαβάζει "acx_rms_db: -25.65"
    // συμπεραίνει ότι το ΠΑΡΑΔΟΤΕΟ του είναι εκεί.
    // Δεν είναι.
    //
    // ΣΒΗΝΕΙ όταν το certificate γίνει project file που
    // ενημερώνεται και κρατάει ΚΑΙ ΤΙΣ ΔΥΟ μετρήσεις,
    // ονομασμένες. northstar §Σ. Δόγμα Ι.
    // ────────────────────────────────────────────────
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
    inner: Arc<Mutex<HashMap<String, StoredBlobV2>>>,
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

    pub fn insert(&self, blob: StoredBlobV2) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(blob.core.id.clone(), blob);

            // Evict oldest when over cap (created_at is chrono RFC 3339 —
            // ISO 8601 with UTC, lexicographically sortable).
            if map.len() > Self::MAX_BLOBS {
                let oldest_id = map
                    .iter()
                    .min_by_key(|(_, b)| &b.core.created_at)
                    .map(|(id, _)| id.clone());
                if let Some(id) = oldest_id {
                    map.remove(&id);
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<StoredBlobV2> {
        self.inner.lock().ok()?.get(id).cloned()
    }
}

impl Default for BlobStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── §Π: ΤΟ CERTIFICATE ΕΠΙΒΙΩΝΕΙ RESTART ─────────────────────────

/// Η DB ξέρει ΓΙΑ τα πράγματα· ο δίσκος ΕΧΕΙ τα πράγματα.
pub const SIDECAR_FORMAT: &str = "creator-os-certificate-sidecar";
pub const SIDECAR_FORMAT_VERSION: u32 = 1;
pub const SIDECAR_SCHEMA: &str = "StoredBlobV2";

/// Τα ΤΡΙΑ πεδία που το `#[serde(skip)]` πετάει από τον Core —
/// και που το ΔΗΜΟΣΙΟ BlobResponse εκθέτει.
///
/// ΧΩΡΙΣ ΑΥΤΑ: ένα certificate που φορτώνεται από δίσκο θα ανέφερε
/// sample_rate 0 · channels 0 · num_frames 0 ΣΑΝ ΜΕΤΡΗΜΕΝΑ. Σιωπηλό
/// ψέμα μέσα από τη σύμβαση — δόγμα Ι.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SidecarTechnical {
    pub sample_rate: u32,
    pub channels: u16,
    pub num_frames: usize,
}

/// Ο φάκελος. ΔΕΝ είναι το blob — το ΠΕΡΙΕΧΕΙ.
///
/// Το `format_version` είναι η μεμβράνη: το αρχείο στον δίσκο ζει
/// χρόνια, το struct αλλάζει.
/// Το `master_sha256` δένει την απόδειξη στο ΠΡΟΪΟΝ της.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateSidecar {
    pub format: String,
    pub format_version: u32,
    pub schema: String,
    pub written_at: String,
    pub engine_version: String,
    pub engine_commit: String,
    pub master_sha256: String,
    pub technical: SidecarTechnical,
    pub payload: StoredBlobV2,
}

/// ΚΑΘΕ αποτυχία έχει ΟΝΟΜΑ. Καμία δεν καταπίνεται.
/// Το schema.rs καταγράφει τι κοστίζει το αντίθετο.
#[derive(Debug)]
pub enum SidecarError {
    Io(String),
    Serde(String),
    WrongFormat { found: String },
    UnknownFormatVersion { found: u32, supported: u32 },
    MasterMissing(std::path::PathBuf),
    MasterHashMismatch { expected: String, actual: String },
}

impl std::fmt::Display for SidecarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "sidecar io: {e}"),
            Self::Serde(e) => write!(f, "sidecar json: {e}"),
            Self::WrongFormat { found } => {
                write!(f, "not a certificate sidecar: format={found:?}")
            }
            Self::UnknownFormatVersion { found, supported } => write!(
                f,
                "sidecar format_version {found} is not readable by this build \
                 (supports {supported}) — the proof is NOT guessed at"
            ),
            Self::MasterMissing(p) => write!(
                f,
                "certificate exists but its master does not: {} — a proof without its product",
                p.display()
            ),
            Self::MasterHashMismatch { expected, actual } => write!(
                f,
                "master_sha256 mismatch — certificate says {expected}, disk says {actual}. \
                 The certificate certifies BYTES; these are not those bytes"
            ),
        }
    }
}

/// ΤΟ ΜΟΝΟ σημείο που ξέρει πού ζει ένα certificate.
/// AUTH-READY HOOK #1: αύριο `{user}/` prefix μπαίνει ΕΔΩ, πουθενά αλλού.
///
/// ⚠ Η sanitisation ΠΡΕΠΕΙ να ταυτίζεται με αυτήν του FLAC persist
/// (dsp_pipeline), αλλιώς ένα track_id με slash σπάει το ζεύγος ΣΙΩΠΗΛΑ.
pub fn blob_storage_path(
    masters_dir: &str,
    project_id: &str,
    blob_id: &str,
) -> std::path::PathBuf {
    std::path::Path::new(masters_dir)
        .join(sanitize_path_component(project_id))
        .join(format!("{}.json", sanitize_path_component(blob_id)))
}

/// Ίδιος κανόνας με το FLAC persist. Ένας κανόνας, ένα σημείο.
pub fn sanitize_path_component(s: &str) -> String {
    s.replace('/', "").replace('\\', "")
}

fn sha256_file(path: &std::path::Path) -> Result<String, SidecarError> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).map_err(|e| SidecarError::Io(e.to_string()))?;
    let mut h = Sha256::new();
    h.update(&bytes);
    Ok(hex::encode(h.finalize()))
}

/// Γράφει το ζεύγος απόδειξη-δίπλα-στο-προϊόν. ΣΥΓΧΡΟΝΑ, στο ίδιο νήμα.
///
/// Atomic: `.tmp` → rename. Ένα μισογραμμένο certificate είναι χειρότερο
/// από κανένα, γιατί ΜΟΙΑΖΕΙ με certificate.
///
/// ΔΕΝ ΕΝΗΜΕΡΩΝΕΙ ΠΟΤΕ υπάρχον sidecar: το certificate πιστοποιεί BYTES.
/// Re-encode = ΝΕΟ render = νέο ζεύγος. Τα παλιά μένουν έγκυρο ιστορικό.
pub fn write_sidecar(
    masters_dir: &str,
    project_id: &str,
    blob: &StoredBlobV2,
    master_flac: &std::path::Path,
) -> Result<std::path::PathBuf, SidecarError> {
    if !master_flac.exists() {
        return Err(SidecarError::MasterMissing(master_flac.to_path_buf()));
    }
    let master_sha256 = sha256_file(master_flac)?;

    let envelope = CertificateSidecar {
        format: SIDECAR_FORMAT.to_string(),
        format_version: SIDECAR_FORMAT_VERSION,
        schema: SIDECAR_SCHEMA.to_string(),
        written_at: chrono::Utc::now().to_rfc3339(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        engine_commit: env!("GIT_HASH").to_string(),
        master_sha256,
        technical: SidecarTechnical {
            sample_rate: blob.core.sample_rate,
            channels: blob.core.channels,
            num_frames: blob.core.num_frames,
        },
        payload: blob.clone(),
    };

    let json = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| SidecarError::Serde(e.to_string()))?;

    let final_path = blob_storage_path(masters_dir, project_id, &blob.core.id);
    if let Some(dir) = final_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| SidecarError::Io(e.to_string()))?;
    }
    let tmp_path = final_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json).map_err(|e| SidecarError::Io(e.to_string()))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| SidecarError::Io(e.to_string()))?;

    Ok(final_path)
}

/// Διαβάζει και ΕΠΑΛΗΘΕΥΕΙ. Άγνωστη έκδοση ή λάθος hash ΔΕΝ γίνονται
/// μαντεψιά και ΔΕΝ γίνονται σιωπηλό 404 — γίνονται ονομασμένο σφάλμα.
pub fn read_sidecar(path: &std::path::Path) -> Result<StoredBlobV2, SidecarError> {
    let bytes = std::fs::read(path).map_err(|e| SidecarError::Io(e.to_string()))?;
    let envelope: CertificateSidecar =
        serde_json::from_slice(&bytes).map_err(|e| SidecarError::Serde(e.to_string()))?;

    if envelope.format != SIDECAR_FORMAT {
        return Err(SidecarError::WrongFormat {
            found: envelope.format,
        });
    }
    if envelope.format_version != SIDECAR_FORMAT_VERSION {
        return Err(SidecarError::UnknownFormatVersion {
            found: envelope.format_version,
            supported: SIDECAR_FORMAT_VERSION,
        });
    }

    // Η απόδειξη δένεται στο προϊόν της. Το .flac είναι ΟΜΩΝΥΜΟ,
    // δίπλα-δίπλα — blob_id ΕΙΝΑΙ το track_id (dsp_pipeline).
    let master_flac = path.with_extension("flac");
    if !master_flac.exists() {
        return Err(SidecarError::MasterMissing(master_flac));
    }
    let actual = sha256_file(&master_flac)?;
    if actual != envelope.master_sha256 {
        return Err(SidecarError::MasterHashMismatch {
            expected: envelope.master_sha256,
            actual,
        });
    }

    // Τα τρία τεχνικά ΞΑΝΑΓΕΜΙΖΟΥΝ πριν το blob γίνει ορατό, ώστε το
    // BlobResponse να βγαίνει ΤΑΥΤΟΣΗΜΟ με το RAM.
    let mut blob = envelope.payload;
    blob.core.sample_rate = envelope.technical.sample_rate;
    blob.core.channels = envelope.technical.channels;
    blob.core.num_frames = envelope.technical.num_frames;
    Ok(blob)
}

/// Ο δίσκος είναι το κοινό μονοπάτι. Το `blob_id` ΔΕΝ κουβαλάει
/// `project_id` (είναι σκέτο `track_id`), άρα parse-and-construct είναι
/// αδύνατο — σαρώνουμε τα projects.
///
/// ⚠ Ok(None) ΣΗΜΑΙΝΕΙ ΑΠΟΥΣΙΑ. Err ΣΗΜΑΙΝΕΙ ΑΔΥΝΑΜΙΑ ΝΑ ΚΟΙΤΑΞΟΥΜΕ.
/// Τα δύο ΔΕΝ είναι το ίδιο: ένα read_dir που αποτυγχάνει δεν
/// επιτρέπεται να γίνει 404 «δεν υπάρχει».
pub fn find_sidecar(
    masters_root: &str,
    blob_id: &str,
) -> Result<Option<std::path::PathBuf>, SidecarError> {
    let safe = sanitize_path_component(blob_id);
    let wanted = format!("{safe}.json");

    let entries = match std::fs::read_dir(masters_root) {
        Ok(e) => e,
        // Ο κατάλογος δεν υπάρχει ακόμα = κανένα certificate γράφτηκε ποτέ.
        // ΑΥΤΟ είναι θεμιτή απουσία.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(SidecarError::Io(e.to_string())),
    };

    for entry in entries {
        let entry = entry.map_err(|e| SidecarError::Io(e.to_string()))?;
        if !entry
            .file_type()
            .map_err(|e| SidecarError::Io(e.to_string()))?
            .is_dir()
        {
            continue;
        }
        let candidate = entry.path().join(&wanted);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;


    fn stub_blob_v2(id: &str) -> StoredBlobV2 {
        StoredBlobV2 {
            core: crate::blob_store::StoredBlobCore {
                id: id.into(),
                version: "1.0".into(),
                blob_type: "audio".into(),
                created_at: "2026-04-15T00:00:00Z".into(),
                input_hash: "aabbccdd".into(),
                seed: 1,
                pipeline_version: "0.4.0".into(),
                schema_version: 1,
                preset_id: "spotify".into(),
                pcm_blake3: None,
                cert_signature: None,
                audio_path: std::sync::Arc::new(lineos_types::audio::ManagedPcm::default()),
                sample_rate: 48000,
                channels: 2,
                num_frames: 48000,
            },
            variant: crate::blob_store::BlobVariant::Certified {
                loudness: StoredLoudness { integrated_lufs: -14.0, ..Default::default() },
                quality: StoredQuality { ..Default::default() },
                provenance: StoredProvenance { ..Default::default() },
                spatial: StoredSpatial { ..Default::default() },
                stem_fingerprints: None,
                processing_timeline: vec![],
                dead_air: Default::default(),
                aether_cert: None,
                aether_persona: None,
                aether_config: None,
                qr_base64: None,
            },
        }
    }

    #[test]
    fn test_blob_store_insert_get() {
        let store = BlobStore::new();
        store.insert(stub_blob_v2("blob-001"));
        let retrieved = store.get("blob-001").unwrap();
        assert_eq!(retrieved.core.id, "blob-001");
        assert!((retrieved.loudness().unwrap().integrated_lufs - (-14.0)).abs() < 1e-6);
    }

    #[test]
    fn test_blob_store_get_missing_returns_none() {
        let store = BlobStore::new();
        assert!(store.get("nonexistent").is_none());
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

}

impl UncertifiedReason {
    /// Ο ΔΗΜΟΣΙΟΣ λόγος. ΣΚΟΠΙΜΑ γενικός.
    ///
    /// Τα εσωτερικά ονόματα λένε ΠΟΙΟ μονοπάτι μας δεν
    /// μετράει — δική μας δουλειά, όχι του καταναλωτή.
    /// Το "SpatialPathHasNoTelemetry" σε δημόσιο JSON
    /// είναι ομολογία ελαττώματος με όνομα αρχείου μέσα.
    ///
    /// ΔΥΟ ΤΙΜΕΣ, ΟΧΙ ΤΡΕΙΣ — γιατί ο καταναλωτής
    /// αντιδρά διαφορετικά:
    ///   no_measurements    το αντικείμενο είναι σωστό
    ///                      αλλά ελλιπές
    ///   not_a_certificate  ΔΕΝ πρέπει να θεωρηθεί
    ///                      απόδειξη καθόλου
    ///
    /// ⚠ ΚΛΕΙΣΤΟ ΣΥΝΟΛΟ. Αυτές οι τιμές είναι ΣΥΜΒΑΣΗ από
    /// τη στιγμή που φεύγουν. Νέα τιμή = ΑΛΛΑΓΗ API, όχι
    /// λεπτομέρεια υλοποίησης. Ίδιο μοτίβο με το Δόγμα Β
    /// για προβολές και ελέγχους.
    pub fn as_public_str(&self) -> &'static str {
        match self {
            Self::SpatialPathHasNoTelemetry => "no_measurements",
            Self::TransportOnlyNotASource   => "not_a_certificate",
        }
    }
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

