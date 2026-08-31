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
    #[serde(default)]
    pub short_term_lufs: Option<f32>, // None = δεν μετρήθηκε — Δ1α 2026-08-21, ΟΧΙ 0.0-ψέμα
    #[serde(default)]
    pub momentary_lufs: Option<f32>, // None = δεν μετρήθηκε — Δ1α 2026-08-21, ΟΧΙ 0.0-ψέμα
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
    /// Sample peak in dBFS after DC removal: `max |sample - mean|`, computed
    /// exactly as `max(max_sample - mean, mean - min_sample)`. SAMPLE peak —
    /// no oversampling. The limiter's true peak is a stricter, separate
    /// measurement. Source: AcxCheckAnalyzer (acx_check.rs:59, :144).
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "acx_sample_peak_db", alias = "input_acx_sample_peak_db")]
    pub input_delivery_peak_db: Option<f32>,
    /// Whole-file unweighted RMS in dBFS, DC removed. One pass via
    /// `E[x^2] - mean^2` (identical to subtract-then-RMS).
    /// Source: AcxCheckAnalyzer (acx_check.rs:61, :146-147).
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "acx_rms_db", alias = "input_acx_rms_db")]
    pub input_delivery_rms_db: Option<f32>,
    /// RMS of the quietest sliding 500 ms window (100 ms hop) after an
    /// 8th-order Butterworth highpass at 10 Hz. None when the input is
    /// shorter than 1 s. Source: AcxCheckAnalyzer (acx_check.rs:62-64,
    /// :15-18, :149-168).
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "acx_noise_floor_db", alias = "input_acx_noise_floor_db")]
    pub input_delivery_noise_floor_db: Option<f32>,
    /// Start position of the quietest 500 ms window used for the noise floor,
    /// in samples AT THE ANALYZER'S RATE (44.1k in the export path vs 48k in
    /// the trunk — the count is relative to that stream).
    /// Source: AcxCheckAnalyzer (acx_check.rs:66-70, :166).
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "acx_quietest_window_start_frame",
            alias = "input_acx_quietest_window_start_frame")]
    pub input_delivery_quietest_window_start_frame: Option<usize>,
    /// ⚠ ΤΟ ΟΝΟΜΑ ΔΕΝ ΚΑΘΑΡΙΣΕ (§5.1α, 2026-08-23): "compliant"
    /// ΕΝΑΝΤΙ ΠΟΙΟΥ; Μένει `acx` μέχρι να κριθεί ρητά — δίπλα του
    /// θέλει το delivery_profile. Σήμερα: AcxCheckReport::passes_acx()
    /// έναντι των τεσσάρων consts του acx_check.rs:30-33.
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "acx_compliant")]
    pub input_acx_compliant: Option<bool>,

    // ── ΤΑ output_delivery_* ΜΕΤΡΟΥΝ ΤΟ ΠΑΡΑΔΟΤΕΟ ──────
    // μετρημένα στο ΤΕΛΙΚΟ deliverable buffer (export.rs,
    // AcxCheckAnalyzer — ΙΔΙΟ όργανο με τα input_delivery_*·
    // δηλωμένο bias: sample peak, ΟΧΙ true peak). §5.6 Δ2,
    // 2026-08-22. Ορισμοί ανά μετρική: ίδιοι με τα input_delivery_*
    // παραπάνω (ίδιο όργανο, άλλο buffer).
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "output_acx_sample_peak_db")]
    pub output_delivery_peak_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "output_acx_rms_db")]
    pub output_delivery_rms_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "output_acx_noise_floor_db")]
    pub output_delivery_noise_floor_db: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none",
            alias = "output_acx_quietest_window_start_frame")]
    pub output_delivery_quietest_window_start_frame: Option<usize>,

    /// §5.3 — η κρίση υπολογίζεται ΜΙΑ φορά στο render/deliver και
    /// αποθηκεύεται ΜΑΖΙ με τα κατώφλια που χρησιμοποίησε. Οι όψεις
    /// ΤΥΠΩΝΟΥΝ, ΔΕΝ κρίνουν: αλλαγή preset μετά την έκδοση ΔΕΝ
    /// ξαναγράφει ιστορία. Το required_db είναι η ΔΗΜΟΣΙΕΥΜΕΝΗ
    /// προδιαγραφή (ώστε ο εκδότης να την αναγνωρίζει)· το
    /// margin_applied_db είναι δικό μας, μετρημένο (F-077, 2026-08-23),
    /// και το effective threshold προκύπτει από τα δύο. None/απουσία
    /// εγγραφής = δεν μετρήθηκε (κανόνας απουσίας §5.2) — ΟΧΙ ψευδής
    /// τιμή.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_checks: Option<Vec<DeliveryCheck>>,

    /// ΤΟ ΕΜΠΟΡΙΚΟ ΟΝΟΜΑ ΩΣ ΤΙΜΗ, ΟΧΙ ΩΣ ΔΟΜΗ (§5.1α, 2026-08-23).
    /// None = δεν δηλώθηκε προφίλ (κανόνας απουσίας 5.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_profile: Option<DeliveryProfileRef>,
}

/// Μία γραμμή κρίσης του §5.3: ένα κατώφλι, το μετρημένο νούμερο που
/// κρίθηκε έναντι αυτού, και η ετυμηγορία — ΟΥΔΕΤΕΡΑ ΟΝΟΜΑΤΑ, ο
/// ένοικος (ποιο preset/profile) ζει στην ΤΙΜΗ (delivery_profile),
/// όχι στο όνομα του πεδίου (§5.1α).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeliveryCheck {
    /// "rms" | "peak" | "noise_floor" | "head_spacing" | "tail_spacing".
    pub metric: String,
    /// τροποποίηση 24/08 (F-074 ξανά): πεδίο που λέει _db και κρατάει
    /// δευτερόλεπτα είναι ψέμα με τ' όνομά του — η μονάδα γίνεται ΔΕΔΟΜΕΝΟ
    /// (`unit`), το όνομα ουδέτερο. Παλιό όνομα `measured_db` ήταν σωστό
    /// μόνο όσο ΚΑΘΕ check ήταν dB· το spacing το έσπασε.
    #[serde(alias = "measured_db")]
    pub measured: f32,
    /// Η ΔΗΜΟΣΙΕΥΜΕΝΗ προδιαγραφή, χωρίς margin.
    #[serde(alias = "required_db")]
    pub required: f32,
    /// "min" | "max" — προς ποια κατεύθυνση κρίνει το required.
    pub bound: String,
    /// Δικό μας, μετρημένο (F-077). 0.0 όπου δεν ισχύει.
    #[serde(alias = "margin_applied_db")]
    pub margin_applied: f32,
    /// "pass" | "fail".
    pub verdict: String,
    /// "db" | "seconds" — τι μονάδα κρατούν measured/required/margin_applied
    /// σε ΑΥΤΗ την εγγραφή. Absent στα παλιά sidecars (πριν 24/08): default
    /// σε "db", σωστό εκ των υστέρων γιατί ΚΑΘΕ εγγραφή πριν το spacing ήταν
    /// dB — δεν υπήρχε άλλη μονάδα να μπερδευτεί.
    #[serde(default = "default_delivery_check_unit")]
    pub unit: String,
}

fn default_delivery_check_unit() -> String {
    "db".to_string()
}

/// Ποια δημοσιευμένη προδιαγραφή μετρήθηκε — ΕΛΕΓΞΙΜΟΣ ΙΣΧΥΡΙΣΜΟΣ
/// δικός μας, ΟΧΙ σφραγίδα τρίτου (§5.1α).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DeliveryProfileRef {
    /// π.χ. "acx-audiobook".
    pub id: String,
    /// URL της δημοσιευμένης προδιαγραφής. None = ΔΕΝ ΕΧΕΙ ΓΡΑΦΤΕΙ
    /// ΑΚΟΜΑ — το γράφει ΑΝΘΡΩΠΟΣ που το επαλήθευσε, ποτέ ο κώδικας.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Ημερομηνία ανάκτησης της παραπάνω πηγής. Ίδιος κανόνας: άνθρωπος.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredQuality {
    // ΠΕΡΙΓΡΑΦΗ, ΟΧΙ ΕΤΥΜΗΓΟΡΙΑ: κανένας οίκος
    // (ACX/podcast/music distributors) δεν απορρίπτει
    // για συσχέτιση φάσης — μετρημένο 25/08. Κανένα
    // δημοσιευμένο κατώφλι, καμία delivery_check.
    // Το ΧΡΗΣΙΜΟ μέγεθος για μουσική είναι «τι
    // χάνεται σε mono fold-down» (πρότυπο:
    // folddown_gain_db + κατώφλι null ≤ −45 του 5.1)
    // — ΝΕΟ πεδίο όταν φτάσει η στήλη Δ, ΟΧΙ
    // επαναορισμός αυτού.
    pub stereo_correlation: f32,
    // F-085 (2026-08-25): το `phase_coherence` ΑΦΑΙΡΕΘΗΚΕ.
    // Πεδίο ΧΩΡΙΣ ΟΡΙΣΜΟ σε κώδικα ΚΑΙ σε προδιαγραφή —
    // δεν έγινε Option, γιατί το None θα δήλωνε «υπάρχει
    // μέγεθος, δεν μετρήθηκε». Δεν υπήρχε μέγεθος.
    // Τεκμήριο: docs/certificate-schema-v0.md §ΑΦΑΙΡΕΣΗ.
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
    // ── §Σ/Ψ5 Platform provenance block ──
    // #[serde(default)] ώστε παλιά sidecars JSON να διαβάζονται (κενό string = προ-Ψ5 cert, ΟΧΙ σφάλμα)
    #[serde(default)]
    pub target_triple: String,
    #[serde(default)]
    pub target_arch: String,
    #[serde(default)]
    pub target_os: String,
    #[serde(default)]
    pub target_env: String,
    #[serde(default)]
    pub target_cpu: String,
    #[serde(default)]
    pub rustc_version: String,
    #[serde(default)]
    pub opt_level: String,
    #[serde(default)]
    pub codegen_units: String,
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
    /// Gain ΠΟΥ ΕΦΑΡΜΟΖΕΙΣ στο fold (stereo_rms_db - folded_bed_rms_db) για να φτάσεις το stereo.
    /// None = δεν υπάρχει spatial παραδοτέο ή δεν μετρήθηκε (§Σ, Δόγμα Ε)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folddown_gain_db: Option<f32>,
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
    /// Limiter lookahead latency σε samples (0 = προ-πεδίου cert, §Σ)
    #[serde(default)]
    pub declared_latency_samples: u32,
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
    /// Per-install Ed25519 key identity ID (§Σ/Ψ6)
    #[serde(default)]
    pub key_id: String,
    /// Signer public key in HEX format (64 hex chars = 32 bytes)
    #[serde(default)]
    pub signer_public_key: String,
    /// Ed25519 signature over the serialized sidecar bytes with THIS
    /// field set to "" (byte-detached, v0 §6/Σ1α) — base64url no-pad.
    /// "" = προ-Σ1α cert, δεν υπογράφηκε.
    #[serde(default)]
    pub payload_signature: String,
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
    /// Ο anchor `"payload_signature": ""` δεν βρέθηκε ΑΚΡΙΒΩΣ μία φορά
    /// στα serialized bytes — refuse to sign blind (Σ1α).
    SignatureAnchorNotUnique { found: usize },
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
            Self::SignatureAnchorNotUnique { found } => write!(
                f,
                "\"payload_signature\": \"\" anchor found {found} times in serialized sidecar \
                 (expected exactly 1) — refusing to sign blind"
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

// ─────────────────────────────────────────────────────────────────
// ΤΟ SPOOL NAMING REGISTRY.
//
// ΚΑΘΕ scratch αρχείο του daemon παίρνει όνομα ΑΠΟ ΕΔΩ. Inline
// `format!(...)` πάνω σε `spool_dir()` = bug εξ ορισμού: BUG-DECODE-1
// ήταν ακριβώς αυτό — ο decode έφτιαχνε `m0d-raw-{blob_id}.pcm` με
// ασανιτάριστο `blob_id`, το path σκούνταζε σε `/` πριν προλάβει να
// γράψει. Οκτώ ακόμα σημεία (mastering, scratch ×2, premaster ×2,
// mastered, vad-trace, raw-spatial) είχαν το ΙΔΙΟ σχήμα, ίδια νόσος.
// Ζει εδώ, στο blob_store.rs, γιατί εδώ ζει ήδη η
// `sanitize_path_component` και η `blob_storage_path` — ένα αρχείο
// που απαντάει «ποιο όνομα παίρνει ένα blob στον δίσκο», όχι δύο.
// ─────────────────────────────────────────────────────────────────

/// BUG-DECODE-1: ο πρώτος writer (decode).
pub fn raw_dump_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!("m0d-raw-{}.pcm", sanitize_path_component(blob_id)))
}

/// Ίδιο λεξιλόγιο, spatial variant (`render_node`'s 6ch dump writer).
pub fn raw_dump_spatial_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-raw-{}-spatial.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Το mmap-backed working file του NODE 4 render· ίδιο όνομα και
/// στο episode-streaming path (`episode_render.rs`).
pub fn mastering_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-mastering-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Scratch L — file-backed mmap working storage (NODE 4).
pub fn scratch_l_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-scratch-l-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Scratch R — file-backed mmap working storage (NODE 4).
pub fn scratch_r_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-scratch-r-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Pre-master L snapshot — diagnostic mode only (VAD A/B observe).
pub fn premaster_l_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-premaster-l-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Pre-master R snapshot — diagnostic mode only (VAD A/B observe).
pub fn premaster_r_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-premaster-r-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Post-render raw PCM tap — streaming executor path.
pub fn mastered_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-mastered-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// VAD trace CSV — diagnostic sidecar of the render node.
pub fn vad_trace_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "vad-trace-{}.csv",
        sanitize_path_component(blob_id)
    ))
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

    let identity = crate::identity::load_or_generate_default()
        .map_err(|e| SidecarError::Io(e.to_string()))?;

    let latency = if blob.core.sample_rate > 0 {
        let l = sp314_dsp::limiter::core::lookahead_samples(blob.core.sample_rate);
        assert!(
            l > 0 && l < blob.core.sample_rate / 2,
            "declared_latency_samples sanity check failed: {l}"
        );
        l
    } else {
        0
    };

    let envelope = CertificateSidecar {
        format: SIDECAR_FORMAT.to_string(),
        format_version: SIDECAR_FORMAT_VERSION,
        schema: SIDECAR_SCHEMA.to_string(),
        written_at: chrono::Utc::now().to_rfc3339(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        engine_commit: env!("GIT_HASH").to_string(),
        master_sha256,
        key_id: identity.key_id.clone(),
        signer_public_key: identity.public_key_hex(),
        payload_signature: String::new(),
        technical: SidecarTechnical {
            sample_rate: blob.core.sample_rate,
            channels: blob.core.channels,
            num_frames: blob.core.num_frames,
            declared_latency_samples: latency,
        },
        payload: blob.clone(),
    };

    // Σ1α — payload signature, byte-detached (v0 §6):
    // (a) serialize with payload_signature = "" — these are the exact
    //     bytes the signature covers.
    let unsigned_json = serde_json::to_vec_pretty(&envelope)
        .map_err(|e| SidecarError::Serde(e.to_string()))?;
    // (b) sign the unsigned bytes, base64url no-pad.
    let signature = identity.sign(&unsigned_json);
    let sig_b64 = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    };
    // (c) splice the signature into the SAME bytes via string replace —
    // NOT a second struct serialize. A second serialize could reorder
    // or reformat anything serde touches, and the signature would die
    // silently (it would verify against bytes that no longer exist).
    // The anchor must appear exactly once, or we refuse to sign blind.
    let unsigned_str = String::from_utf8(unsigned_json)
        .map_err(|e| SidecarError::Serde(format!("sidecar bytes not valid UTF-8: {e}")))?;
    let anchor = "\"payload_signature\": \"\"";
    let occurrences = unsigned_str.matches(anchor).count();
    if occurrences != 1 {
        return Err(SidecarError::SignatureAnchorNotUnique { found: occurrences });
    }
    let replacement = format!("\"payload_signature\": \"{sig_b64}\"");
    let json = unsigned_str.replacen(anchor, &replacement, 1).into_bytes();

    // (d) atomic write, unchanged: tmp -> rename.
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
                input_path_hash: "aabbccdd".into(),
                input_pcm_sha256: Some("pcm-aabbccdd".into()),
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

    /// F-074: sidecars written between 964c96f and today carry the
    /// intermediate key "input_path_sha256" (neither the oldest
    /// "input_hash" nor the current "input_path_hash"). All three
    /// must deserialize into input_path_hash.
    #[test]
    fn test_stored_blob_core_input_path_hash_aliases() {
        fn core_json(key: &str) -> String {
            format!(
                r#"{{
                    "id": "blob-alias-test",
                    "version": "1.0",
                    "blob_type": "audio",
                    "created_at": "2026-04-15T00:00:00Z",
                    "{key}": "aabbccdd",
                    "seed": 1,
                    "pipeline_version": "0.4.0",
                    "schema_version": 1,
                    "preset_id": "spotify",
                    "pcm_blake3": null,
                    "cert_signature": null
                }}"#
            )
        }

        for key in ["input_hash", "input_path_sha256", "input_path_hash"] {
            let json = core_json(key);
            let core: StoredBlobCore = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("failed to deserialize with key {key:?}: {e}"));
            assert_eq!(
                core.input_path_hash, "aabbccdd",
                "wrong input_path_hash value for key {key:?}"
            );
        }
    }

    /// F-hash-language (2026-08-23): input_pcm_hash -> input_pcm_sha256
    /// rename must still accept sidecars written under the old key.
    #[test]
    fn test_stored_blob_core_input_pcm_sha256_alias() {
        fn core_json(key: &str) -> String {
            format!(
                r#"{{
                    "id": "blob-pcm-alias-test",
                    "version": "1.0",
                    "blob_type": "audio",
                    "created_at": "2026-04-15T00:00:00Z",
                    "input_path_hash": "aabbccdd",
                    "{key}": "eeff0011",
                    "seed": 1,
                    "pipeline_version": "0.4.0",
                    "schema_version": 1,
                    "preset_id": "spotify",
                    "pcm_blake3": null,
                    "cert_signature": null
                }}"#
            )
        }

        for key in ["input_pcm_hash", "input_pcm_sha256"] {
            let json = core_json(key);
            let core: StoredBlobCore = serde_json::from_str(&json)
                .unwrap_or_else(|e| panic!("failed to deserialize with key {key:?}: {e}"));
            assert_eq!(
                core.input_pcm_sha256,
                Some("eeff0011".to_string()),
                "wrong input_pcm_sha256 value for key {key:?}"
            );
        }
    }

    /// F-074 ξανά (2026-08-24): DeliveryCheck's *_db fields renamed to
    /// unit-neutral names (measured_db -> measured, required_db ->
    /// required, margin_applied_db -> margin_applied) because spacing
    /// checks measure seconds, not dB — a field literally named "_db"
    /// holding seconds is the same lie F-074 found in input_hash. Sidecars
    /// written before this rename (all §5.3 checks so far are dB-only)
    /// must still deserialize, WITHOUT a "unit" key present at all.
    #[test]
    fn test_delivery_check_old_db_suffixed_keys_alias_and_default_unit() {
        let json = r#"{
            "metric": "rms",
            "measured_db": -22.9,
            "required_db": -23.0,
            "bound": "min",
            "margin_applied_db": 0.35,
            "verdict": "fail"
        }"#;
        let check: DeliveryCheck = serde_json::from_str(json)
            .unwrap_or_else(|e| panic!("failed to deserialize old-key DeliveryCheck: {e}"));
        assert_eq!(check.measured, -22.9);
        assert_eq!(check.required, -23.0);
        assert_eq!(check.margin_applied, 0.35);
        assert_eq!(
            check.unit, "db",
            "unit absent in old sidecar must default to \"db\" — every \
             pre-spacing check was dB, nothing else to confuse it with"
        );

        // ΤΟ ΖΕΥΓΟΣ: νέο-στυλ JSON (νέα κλειδιά, ρητό unit) διαβάζεται
        // εξίσου — η αλλαγή δεν έσπασε τη γραφή προς τα εμπρός.
        let json_new = r#"{
            "metric": "head_spacing",
            "measured": 6.2,
            "required": 5.0,
            "bound": "max",
            "margin_applied": 0.0,
            "verdict": "fail",
            "unit": "seconds"
        }"#;
        let check_new: DeliveryCheck = serde_json::from_str(json_new)
            .unwrap_or_else(|e| panic!("failed to deserialize new-key DeliveryCheck: {e}"));
        assert_eq!(check_new.measured, 6.2);
        assert_eq!(check_new.unit, "seconds");
    }

    /// GUARD (F-hash-language, 2026-08-23) — external juror, not
    /// self-comparison: independently verifies that the value which ends
    /// up in input_pcm_sha256 really is a SHA-256 over the bytes the
    /// schema claims.
    ///
    /// What is hashed, ΑΥΤΟΥΣΙΟ από τον κώδικα (stream_core.rs push_output,
    /// ~L134-186):
    ///   - channels:      interleaved L,R (stereo, N=2)
    ///   - sample format: f32
    ///   - endianness:    big-endian (`s.to_be_bytes()`, stream_core.rs:178)
    ///   - sample rate:   48 kHz (TARGET_SR — this fixture is written at
    ///                    44.1 kHz to exercise the resample step)
    ///   - chain point:   POST resample, POST sanitize, PRE quantize (this
    ///                    is float PCM; int quantization happens later, in
    ///                    flac_encode.rs — not before this hash)
    ///
    /// The oracle side below never calls `input_hashes()` (stream_core.rs:241)
    /// or anything downstream of it — it re-decodes the same fixture via
    /// the independent decode_smart() batch path (handlers/decode.rs) and
    /// hashes with its own fresh sha2::Sha256, exactly the e2e_acx_certificate
    /// "external juror" pattern (verify by re-deriving from the source
    /// bytes, not by re-reading the value under test).
    #[test]
    fn test_input_pcm_sha256_independent_guard() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path_buf = tmp.path().join("guard_fixture.wav");
        let path = path_buf.to_str().unwrap();
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = 44_100usize;
        for i in 0..n {
            let v = 0.25
                * libm::sinf(2.0 * std::f32::consts::PI * 440.0 * (i as f32 / 44_100.0));
            w.write_sample(v).unwrap(); // L
            w.write_sample(v).unwrap(); // R
        }
        w.finalize().unwrap();

        // The value under test: the real production path that ends up in
        // StoredBlobCore.input_pcm_sha256 (decode_node -> dsp_pipeline ->
        // certificate_node::assemble_blob).
        let decoded = crate::domain::nodes::decode_node::run(path, "podcast", "guard-blob")
            .expect("decode_node::run failed");

        // The oracle: an independent decode (decode_smart batch path),
        // hashed here with a hasher this test owns — no call into
        // input_hashes() or the streaming path that produced `decoded`.
        let payload = crate::handlers::decode::decode_smart(path).expect("decode_smart failed");
        let buf = match payload {
            lineos_types::AudioPayload::Stereo(b) => b,
            _ => panic!("expected stereo payload for this fixture"),
        };
        let mut interleaved = Vec::with_capacity(buf.num_frames * 2);
        for i in 0..buf.num_frames {
            interleaved.push(buf.left[i]);
            interleaved.push(buf.right[i]);
        }
        use sha2::Digest;
        let mut oracle = sha2::Sha256::new();
        for &sample in &interleaved {
            oracle.update(sample.to_be_bytes());
        }
        let oracle_sha256 = format!("{:x}", oracle.finalize());

        assert_eq!(
            decoded.input_sha256_hex, oracle_sha256,
            "input_pcm_sha256 must equal an independently re-derived SHA-256 of the decoded PCM"
        );
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
    /// SHA-256 του path string — internal (cache), ΟΧΙ provenance. F-074.
    #[serde(alias = "input_hash", alias = "input_path_sha256")]
    pub input_path_hash: String,
    /// SHA-256 του decoded input PCM — input provenance, αυτό
    /// επαληθεύει ο τρίτος. Ο αλγόριθμος ΣΤΟ ΟΝΟΜΑ κατά τον
    /// κανόνα γλώσσας των hashes (σχήμα §6, 2026-08-23): αλλαγή
    /// αλγορίθμου = ΝΕΟ πεδίο, ΠΟΤΕ ίδιο όνομα με άλλα bytes.
    #[serde(default, alias = "input_pcm_hash")]
    pub input_pcm_sha256: Option<String>,
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

