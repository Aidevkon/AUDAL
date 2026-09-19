use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StageRecord {
    pub stage: String,
    pub duration_ms: u64,
    pub stage_hash: String, // FNV of stage output
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

/// Δηλώνει ΑΝ ο μετρητής είναι και ο παραγωγός του ήχου.
///
// SOURCE: https://portal.amazonstudios.com/hc/en-us/articles/15986851525147-Delivery-QC
// RETRIEVED: 2026-09-06
// Η Amazon απαιτεί ο QC vendor να ΜΗΝ είναι αυτός που δημιούργησε τα
// αρχεία (video/SDH/FN/AD· ΔΕΝ αναφέρει ρητά ήχο — δηλωμένη επιφύλαξη),
// και η παράκαμψη θέλει έγκριση Post Executive.
// Το πεδίο υπάρχει ώστε ο αναγνώστης να ξέρει ΑΝ ο μετρητής είναι και ο
// παραγωγός, ΧΩΡΙΣ να ρωτήσει. ΔΕΝ είναι αποτυχία — είναι δηλωμένο όριο,
// όπως ο κανόνας απουσίας §5.2.
///
/// ⚠ ΟΧΙ bool. Ένα `self_certified: true` διαβάζεται ως ντροπή· ένα enum
/// περιγράφει ΚΑΤΑΣΤΑΣΗ. Ίδιο μάθημα με το "advisory" του DeliveryCheck:
/// η τιμή περιγράφει ΚΛΑΣΗ, όχι σοβαρότητα.
///
/// ⚠ ΔΕΝ μετράει στη συμμόρφωση. Ο DeliveryVerdict::compose παίρνει
/// (spec, checks) — δομικά δεν μπορεί να το δει.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioOrigin {
    /// Ο ήχος παρήχθη από ΑΥΤΗ τη μηχανή, και η ίδια μηχανή τον μετράει.
    /// ΣΗΜΕΡΑ ΠΑΝΤΑ ΑΥΤΟ: δεν υπάρχει διαδρομή εισόδου τρίτου.
    #[default]
    SelfProduced,
    /// Ο ήχος ήρθε απ' έξω· μετρήθηκε μόνο, δεν παρήχθη εδώ.
    /// ΔΕΝ ΥΠΑΡΧΕΙ ΔΙΑΔΡΟΜΗ ΠΟΥ ΤΟ ΠΑΡΑΓΕΙ — η τιμή υπάρχει ώστε το
    /// σχήμα να μη χρειαστεί αλλαγή όταν αποκτήσει.
    ThirdParty,
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
    /// ΠΡΟΣΘΗΚΗ 2026-09-06 — δες AudioOrigin για την πηγή και την
    /// επιφύλαξη. #[serde(default)] ώστε παλιά sidecars (που ήταν ΟΛΑ
    /// self-produced) να διαβάζονται· απουσία = SelfProduced, ΟΧΙ σφάλμα.
    #[serde(default)]
    pub audio_origin: AudioOrigin,
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
    pub audio_path: std::sync::Arc<crate::audio::ManagedPcm>,
    #[serde(skip)]
    pub sample_rate: u32,
    #[serde(skip)]
    pub channels: u16,
    #[serde(skip)]
    pub num_frames: usize,
}

/// Ονομασμένο μέγεθος με τιμή και μονάδα. ΟΧΙ ελεύθερο map, ΟΧΙ πρόζα.
///
/// ΓΙΑΤΙ ΟΧΙ MAP: το threshold-lint σαρώνει `.rs` και ΟΧΙ JSON (μετρήθηκε
/// 2026-09-12) — γι' αυτό ένα placeholder έμεινε δύο μήνες αόρατο. Ελεύθερα
/// κλειδιά σε ΥΠΟΓΕΓΡΑΜΜΕΝΟ έγγραφο είναι το ίδιο κενό, επί δώδεκα στάδια.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedValue {
    pub name: String,
    pub value: f32,
    /// "dB" | "dBFS" | "s" | "Hz" | "count"
    pub unit: String,
}

/// Τι αποφάσισε η μηχανή, ανά διορθωτικό στάδιο.
///
/// ΞΕΧΩΡΙΣΤΟ ΑΠΟ ΤΟ DeliveryCheck: εκείνο κρίνει το ΑΡΧΕΙΟ έναντι
/// προδιαγραφής (pass/fail/advisory). Αυτό καταγράφει τι έκανε ο
/// ΚΟΜΒΟΣ και γιατί. Ένα στάδιο μπορεί να είναι `skipped` ενώ το
/// αρχείο περνά — δεν είναι αντίφαση, είναι άλλο ερώτημα.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CorrectionRecord {
    /// Κλειστό σύνολο. Ίδιο μοτίβο με το DeliveryCheck.verdict:
    /// String για συμβατότητα, αλλά οι τιμές απαριθμούνται εδώ.
    /// Σήμερα παρατηρείται ΜΙΑ: "interior_noise_analysis".
    pub stage: String,
    /// "applied" | "skipped" | "measured" | "absent"
    ///   applied  — ο κόμβος εκτελέστηκε και ενήργησε
    ///   skipped  — ο κόμβος εκτελέστηκε, δεν χρειάστηκε
    ///   measured — η ανάλυση παρήγαγε μέτρηση (δεν ενεργεί)
    ///   absent   — η μέτρηση δεν ήταν δυνατή
    /// ΤΟ ΛΕΞΙΛΟΓΙΟ ΟΡΙΖΕΤΑΙ ΟΛΟΚΛΗΡΟ, ΠΑΡΑΤΗΡΕΙΤΑΙ ΜΕΡΙΚΩΣ: τα δύο πρώτα
    /// περιμένουν στάδιο που εκτελείται.
    pub state: String,
    /// Κλειστό σύνολο ΑΝΑ stage. ΧΩΡΙΣ αριθμούς μέσα του — οι αριθμοί είναι
    /// μετρήσεις και ζουν στο `measurements`. Κενό όπου η μέτρηση μιλάει μόνη.
    pub reason: String,
    pub measurements: Vec<NamedValue>,
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
