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

// Μετακόμισαν στο lineos-types στις 20/09 (certificate.rs) — μαζί με from_spacing/from_format
// (from_margin_checks βγήκε από το impl και έγινε ελεύθερη συνάρτηση στο conformance, γιατί
// παίρνει &sp314_dsp::…::AcxCheckReport και το sp314-dsp εξαρτάται από το lineos-types).

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
    /// ⚠ ΤΟ ΟΝΟΜΑ ΕΙΝΑΙ ΛΑΘΟΣ ΚΑΙ ΔΕΝ ΑΛΛΑΞΕ — ΣΚΟΠΙΜΑ [F-097].
    ///
    /// ΤΙ ΕΙΝΑΙ: `DeadAirSummary::quietest_active_window_dbfs` — το πιο ήσυχο
    /// παράθυρο 1 s που ΔΕΝ είναι σιωπή (gate −60 dBFS), ΧΩΡΙΣ φιλτράρισμα,
    /// μετρημένο στην ΕΙΣΟΔΟ από τον SignalHealthMonitor.
    /// ΣΕ ΚΑΘΑΡΗ ΑΦΗΓΗΣΗ ΑΥΤΟ ΠΟΥ ΜΕΤΡΙΕΤΑΙ ΕΙΝΑΙ Η ΠΙΟ ΗΣΥΧΗ ΟΜΙΛΙΑ.
    ///
    /// ΤΙ ΔΕΝ ΕΙΝΑΙ: πάτωμα θορύβου. Το πραγματικό πάτωμα ζει δίπλα, στο
    /// `input_delivery_noise_floor_db` (AcxCheckAnalyzer, HP8 @10 Hz,
    /// ελάχιστο κυλιόμενο 500 ms). Τα δύο διέφεραν 43 dB — αυτό ήταν το F-096.
    ///
    /// ΓΙΑΤΙ ΜΕΝΕΙ: είναι ΣΧΗΜΑ. Ταξιδεύει στο ΥΠΟΓΕΓΡΑΜΜΕΝΟ cert και ΔΕΝ έχει
    /// serde alias — σκέτη μετονομασία θα έκανε κάθε υπάρχον sidecar
    /// μη αναγνώσιμο. Η αλλαγή του χρειάζεται `#[serde(rename, alias)]`
    /// κατά το μοτίβο των γραμμών 117-119, και είναι ξεχωριστή απόφαση.
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
    /// θέλει το delivery_profile. Σήμερα: AcxCheckReport::levels_within_limits()
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
    /// "pass" | "fail" | "advisory".
    ///
    /// ΤΡΙΤΗ ΚΑΤΑΣΤΑΣΗ, ΠΡΟΣΘΗΚΗ 2026-08-25 (απόφαση Anestis). Η
    /// δημοσιευμένη πηγή διακρίνει δύο κλάσεις κανόνα με διαφορετική
    /// γλώσσα: «Room tone spacing **must not exceed** 5 seconds»
    /// (ΑΠΑΙΤΗΣΗ) έναντι «We **recommend** between 1 and 5 seconds»
    /// (ΣΥΣΤΑΣΗ). Δύο κλάσεις, δύο καταστάσεις.
    ///
    /// "advisory" περιγράφει **ΤΗΝ ΚΛΑΣΗ ΤΟΥ ΚΑΝΟΝΑ**, όχι σοβαρότητα.
    /// ΓΙ' ΑΥΤΟ ΔΕΝ ΛΕΓΕΤΑΙ "warning": το warning υπονοεί πρόβλημα, ενώ
    /// ένα αρχείο κάτω από σύσταση είναι **πλήρως συμμορφούμενο**.
    ///
    /// ⚠ ΤΟ advisory ΔΕΝ ΜΕΤΡΑΕΙ ΣΤΗ ΣΥΝΟΛΙΚΗ ΣΥΜΜΟΡΦΩΣΗ. Η συνολική
    /// κρίση (`AcxCheckReport::levels_within_limits_with_margin`, acx_check.rs:188)
    /// τρέχει πάνω σε `AcxMarginCheck` με **bool** verdict και **δεν
    /// περιέχει spacing** — δομικά αδύνατο να μολυνθεί. Το spacing
    /// μπαίνει από ΞΕΧΩΡΙΣΤΟ παραγωγό (`from_spacing`), ποτέ μέσα στο
    /// `margin_checks()`.
    ///
    /// ⚠ ΠΑΛΙΑ SIDECARS: η αλλαγή είναι **ΠΡΟΣΘΕΤΙΚΗ στο σύνολο τιμών**
    /// — το πεδίο ήταν και μένει `String`. Sidecar γραμμένο πριν την
    /// 25/08 δεν περιέχει "advisory" και διαβάζεται αμετάβλητο.
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

impl DeliveryCheck {
    /// Οι ΤΕΣΣΕΡΙΣ εγγραφές spacing — Ο ΜΟΝΟΣ παραγωγός τους.
    /// `/deliver` και `/export` καλούν ΑΥΤΗΝ. Μία υλοποίηση, δύο καλούντες.
    ///
    /// ΤΕΣΣΕΡΙΣ ΚΑΙ ΟΧΙ ΔΥΟ — δηλωμένη απόφαση: το σχήμα κρατάει **ένα**
    /// `required` με **ένα** `bound` ("min"|"max"), όχι εύρος. Ένα
    /// παράθυρο [1,5] χρειάζεται δύο εγγραφές ανά άκρο. Είναι ακριβώς το
    /// μοτίβο που ήδη ακολουθεί το `rms` (min ΚΑΙ max), και τηρεί το
    /// «ΜΙΑ ΚΡΙΣΗ ΑΝΑ ΓΡΑΜΜΗ» του threshold-lint.
    ///
    /// ΟΙ ΔΥΟ ΚΛΑΣΕΙΣ, από τη γλώσσα της πηγής:
    ///   bound "max", required 5.0 → ΑΠΑΙΤΗΣΗ  ⇒ "pass" | "fail"
    ///   bound "min", required 1.0 → ΣΥΣΤΑΣΗ   ⇒ "pass" | "advisory"
    ///
    /// SOURCE: https://help.acx.com/s/article/what-are-the-acx-audio-submission-requirements
    /// RETRIEVED: 2026-08-25 (σελίδα: Apr 15, 2026)
    ///
    /// ⚠ `margin_applied = 0.0`, ΔΗΛΩΜΕΝΟ ΚΑΙ ΟΧΙ ΕΦΕΥΡΗΜΕΝΟ: το
    /// spacing margin τέθηκε 0.0 στο F-077 («ΜΕΤΡΗΣΗ ΟΧΙ GATE») επειδή
    /// δεν μεσολαβεί encoder που να μετατοπίζει χρόνο — το
    /// `edge_quiet_secs` μετράει το pre-LAME buffer. Δεν υπάρχει χάσμα
    /// να αντισταθμιστεί.
    pub fn from_spacing(
        spec: &crate::presets::DeliverySpec,
        head_quiet_secs: f32,
        tail_quiet_secs: f32,
    ) -> Vec<DeliveryCheck> {
        // ΤΑ ΟΡΙΑ ΕΡΧΟΝΤΑΙ ΑΠΟ ΤΟ ΣΥΜΒΟΛΑΙΟ, ΟΧΙ ΑΠΟ ΕΔΩ.
        // Ήταν consts σε αυτή τη συνάρτηση μέχρι τις 25/08 — δηλαδή η
        // προδιαγραφή του προορισμού ζούσε μοιρασμένη σε δύο αρχεία με
        // ιστορικό, όχι λογικό διαχωρισμό.
        //
        // ΚΑΝΟΝΑΣ ΑΠΟΥΣΙΑΣ (§5.2) ΣΤΟ ΕΠΙΠΕΔΟ ΤΟΥ ΠΑΡΑΓΩΓΟΥ: όριο που ο
        // προορισμός ΔΕΝ δηλώνει ⇒ ΚΑΜΙΑ εγγραφή. Όχι ψευδές pass, όχι
        // κατασκευασμένο fail. Ένας προορισμός χωρίς κανόνα room tone
        // (κάθε μη-ACX σήμερα) δίνει κενή λίστα.
        let mut out = Vec::with_capacity(4);
        for (metric, measured) in [
            ("head_spacing", head_quiet_secs),
            ("tail_spacing", tail_quiet_secs),
        ] {
            if let Some(max_s) = spec.room_tone_max_s {
                out.push(DeliveryCheck {
                    metric: metric.to_string(),
                    measured,
                    required: max_s,
                    bound: "max".to_string(),
                    margin_applied: 0.0,
                    verdict: if measured <= max_s { "pass" } else { "fail" }.to_string(),
                    unit: "seconds".to_string(),
                });
            }
            if let Some(min_s) = spec.room_tone_recommend_min_s {
                out.push(DeliveryCheck {
                    metric: metric.to_string(),
                    measured,
                    required: min_s,
                    bound: "min".to_string(),
                    margin_applied: 0.0,
                    verdict: if measured >= min_s { "pass" } else { "advisory" }.to_string(),
                    unit: "seconds".to_string(),
                });
            }
        }
        out
    }

    /// Οι ΤΡΕΙΣ εγγραφές μορφής — Ο ΜΟΝΟΣ παραγωγός τους.
    /// `/export` και `/deliver` καλούν ΑΥΤΗΝ.
    ///
    /// ΟΛΑ ΤΑ `measured` ΕΙΝΑΙ ΜΕΤΡΗΣΕΙΣ ΤΟΥ ΠΑΡΑΓΟΜΕΝΟΥ ΑΡΧΕΙΟΥ, ΟΧΙ
    /// ΠΡΟΘΕΣΕΙΣ: sample_rate και channels από το decode-back του
    /// symphonia (`export.rs:951-956`), bitrate από bytes×8/διάρκεια.
    /// Έλεγχος πάνω στο τι ΖΗΤΗΣΑΜΕ θα ήταν αυτοαναφορικός.
    ///
    /// ⚠ ΔΗΛΩΣΗ ΕΠΑΛΗΘΕΥΜΕΝΟΥ ΓΕΓΟΝΟΤΟΣ, ΟΧΙ ΝΕΑ ΠΥΛΗ (sample_rate και
    /// channels): η `export_mp3_acx` ΗΔΗ επιστρέφει `Err` αν αποκλίνουν
    /// (`export.rs:1013-1020`). Verdict "fail" σε αυτές τις δύο είναι
    /// **μη-προσβάσιμο στην παραγωγή** — το export θα είχε αποτύχει
    /// πρώτο. Η αξία τους είναι ότι η επιτυχία **αφήνει πλέον ίχνος**
    /// που φτάνει στον χρήστη. Το bitrate είναι το μόνο από τα τρία που
    /// δεν ελεγχόταν πουθενά.
    ///
    /// SOURCE: https://help.acx.com/s/article/what-are-the-acx-audio-submission-requirements
    /// RETRIEVED: 2026-08-25 (σελίδα: Apr 15, 2026)
    /// «Each file must be a 192 kbps or higher CBR, 44.1kHz MP3.»
    pub fn from_format(
        spec: &crate::presets::DeliverySpec,
        sample_rate_hz: u32,
        channels: u16,
        bitrate_kbps: f32,
    ) -> Vec<DeliveryCheck> {
        // ΤΑ ΟΡΙΑ ΑΠΟ ΤΟ ΣΥΜΒΟΛΑΙΟ. Ίδιος κανόνας απουσίας με το
        // from_spacing: αδήλωτο όριο ⇒ καμία εγγραφή.
        let mut out = Vec::with_capacity(3);

        if let Some(sr) = spec.required_sample_rate_hz {
            out.push(DeliveryCheck {
                metric: "sample_rate".to_string(),
                measured: sample_rate_hz as f32,
                required: sr as f32,
                bound: "max".to_string(),
                margin_applied: 0.0,
                // PLACEHOLDER: ανοχή σύγκρισης f32, ΟΧΙ όριο προδιαγραφής.
                // Η πηγή δεν δίνει ανοχή· το 0.5 υπάρχει μόνο επειδή το
                // sample rate ταξιδεύει ως f32 σε αυτή τη δομή.
                // TRIGGER: αν το `measured` γίνει ακέραιος, φεύγει.
                verdict: if sample_rate_hz == sr { "pass" } else { "fail" }.to_string(),
                unit: "hz".to_string(),
            });
        }

        if let Some(ch) = spec.emitted_channels {
            out.push(DeliveryCheck {
                metric: "channels".to_string(),
                measured: channels as f32,
                required: ch as f32,
                bound: "max".to_string(),
                margin_applied: 0.0,
                verdict: if channels == ch { "pass" } else { "fail" }.to_string(),
                unit: "count".to_string(),
            });
        }

        if let Some(min_kbps) = spec.min_bitrate_kbps {
            out.push(DeliveryCheck {
                metric: "bitrate".to_string(),
                measured: bitrate_kbps,
                required: min_kbps as f32,
                bound: "min".to_string(),
                margin_applied: 0.0,
                verdict: if bitrate_kbps >= min_kbps as f32 { "pass" } else { "fail" }
                    .to_string(),
                unit: "kbps".to_string(),
            });
        }

        out
    }
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

// DeadAirSummary/DeadAirEvent μετακόμισαν στο lineos-types στις 20/09 (certificate.rs):
// το F-129 τα είχε ήδη μετρήσει ως έναν από τους δεκατέσσερις τύπους του πιστοποιητικού
// (μόνο σε άλλη ενότητα, dsp/signal_health.rs). Οι ορισμοί ταξιδεύουν αυτούσιοι· ό,τι
// αγγίζει σήμα (SignalHealthMonitor) μένει στο m0-daemon.

/// A stretch of near-silence in the middle of an
/// episode. Non-fatal — surfaced as metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadAirEvent {
    pub start_sec: f32,
    pub duration_sec: f32,
}

/// Bounded summary of dead-air across a stream.
/// `events` is capped at MAX_DEAD_AIR_EVENTS to
/// preserve O(1) memory on long content (e.g. a
/// 4-hour music set); the counters below retain
/// the FULL picture regardless of the cap, so the
/// certificate never lies about total silence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeadAirSummary {
    /// Detailed gaps, capped at MAX_DEAD_AIR_EVENTS.
    pub events: Vec<DeadAirEvent>,
    /// Total number of gaps found (uncapped count).
    pub total_count: usize,
    /// Sum of all gap durations in seconds (uncapped).
    pub total_sec: f32,
    /// Longest single gap in seconds (uncapped).
    pub longest_sec: f32,
    /// True if events were capped (total_count >
    /// events.len()).
    pub truncated: bool,
    /// Minimum dBFS of any non-dead-air window. None if stream is 100% dead air.
    /// ΔΕΝ είναι πάτωμα θορύβου — είναι η πιο ήσυχη ΕΝΕΡΓΗ στιγμή, χωρίς
    /// φιλτράρισμα. ΗΤΑΝ `noise_floor_dbfs` [F-097].
    pub quietest_active_window_dbfs: Option<f32>,
}

/// Default = a clean summary: no dead air observed.
/// Used by the batch (music) path, which has no
/// SignalHealthMonitor — a mastered track legitimately
/// has zero dead air, so count=0 is accurate, not a
/// placeholder.
impl Default for DeadAirSummary {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            total_count: 0,
            total_sec: 0.0,
            longest_sec: 0.0,
            truncated: false,
            quietest_active_window_dbfs: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlobVariant {
    /// Μετρήθηκε. Έχει απόδειξη.
    Certified {
        loudness: StoredLoudness,
        quality: StoredQuality,
        provenance: StoredProvenance,
        spatial: StoredSpatial,
        stem_fingerprints: Option<StemFingerprints>,
        processing_timeline: Vec<StageRecord>,
        dead_air: DeadAirSummary,
        aether_cert: Option<String>,
        aether_persona: Option<String>,
        aether_config: Option<String>,
        qr_base64: Option<String>,
        /// ΠΕΜΠΤΟ ΜΠΛΟΚ. Τα τέσσερα υπάρχοντα (loudness/quality/provenance/
        /// spatial) περιγράφουν ΤΟ ΣΗΜΑ· αυτό περιγράφει ΤΙ ΕΚΑΝΕ Ή ΤΙ ΜΕΤΡΗΣΕ
        /// Η ΜΗΧΑΝΗ. Δύο ερωτήματα, δύο μπλοκ: το `delivery_checks` ρωτά
        /// «συμμορφώνεται το αρχείο;», αυτό ρωτά «τι έκανε η μηχανή;».
        ///
        /// ΚΕΝΟ όταν ο προορισμός δεν δηλώνει όριο πατώματος: ο analyzer δεν
        /// τρέχει καθόλου, άρα δεν υπάρχει ανάλυση να καταγραφεί. Κενό ΔΕΝ
        /// είναι `absent` — το `absent` σημαίνει «έτρεξε και δεν βρήκε
        /// interior».
        ///
        /// ΠΟΛΛΑΠΛΕΣ ΕΓΓΡΑΦΕΣ ΑΝΑ ΑΡΧΕΙΟ ΕΙΝΑΙ ΤΟ DESIGN: όταν ένας κόμβος
        /// διόρθωσης συνδεθεί, η ανάλυση και η ενέργεια θα είναι δύο
        /// ξεχωριστές εγγραφές.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        corrections: Vec<CorrectionRecord>,
    },
    /// ΔΕΝ μετρήθηκε. ΧΡΕΟΣ με όνομα.
    /// ΔΕΝ παραδίδεται σε χρήστη χωρίς ρητή μετατροπή.
    Uncertified { reason: UncertifiedReason },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBlobV2 {
    pub core: StoredBlobCore,
    pub variant: BlobVariant,
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
    pub fn dead_air(&self) -> Option<&DeadAirSummary> {
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
