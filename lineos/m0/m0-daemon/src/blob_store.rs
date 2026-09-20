//! Blob store — in-memory content-addressed store for Golden Blobs.
//! Production: replaced by content-addressed file storage in Phase 7.
//! Phase 6: in-memory only. Blobs are dropped when m0d restarts.
//! Authority: golden-blob-spec.md §Lifecycle

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};



// Ο ορισμός μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — δεν κρατάει DeliveryCheck/DeliveryProfileRef, ελεύθερο να φύγει.
pub use lineos_types::certificate::StageRecord;


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

// StoredLoudness μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — δεν κρατάει DeliveryCheck/DeliveryProfileRef, ελεύθερο να φύγει.
pub use lineos_types::certificate::StoredLoudness;

// DeliveryCheck (με το impl του, from_spacing/from_format) μετακόμισε στο lineos-types στις
// 20/09 (certificate.rs). Το from_margin_checks ΒΓΗΚΕ από το impl και έγινε ελεύθερη συνάρτηση
// στο conformance (δες conformance/src/declare.rs) — παίρνει &sp314_dsp::…::AcxCheckReport, και
// το sp314-dsp εξαρτάται από το lineos-types, άρα δεν μπορούσε να ταξιδέψει με τον τύπο.
pub use lineos_types::certificate::DeliveryCheck;

/// Η ΣΥΝΟΛΙΚΗ ΕΤΥΜΗΓΟΡΙΑ — ΣΥΝΑΡΤΗΣΗ ΠΑΝΩ ΣΤΙΣ ΓΡΑΜΜΕΣ, ΟΧΙ ΓΝΩΣΗ.
///
/// ΓΙΑΤΙ ΥΠΑΡΧΕΙ: μέχρι τις 25/08 ένα αρχείο με `tail 6 s` έγραφε
/// `"fail"` στα `delivery_checks` ΚΑΙ `passes_acx: true` στο manifest —
/// **το ίδιο έγγραφο έλεγε «περνάει» και «παραβίαση»**. Η αιτία δεν ήταν
/// σφάλμα υπολογισμού: δεν υπήρχε πουθενά σημείο που να απαντά «αυτό το
/// αρχείο συμμορφώνεται;». Υπήρχαν δύο μισές απαντήσεις με ονόματα που
/// διεκδικούσαν και οι δύο ολόκληρο το ερώτημα.
///
/// Ο συνθέτης **δεν ξέρει κανένα κριτήριο**. Ξέρει μόνο πώς διαβάζονται
/// οι ετυμηγορίες. Ένα νέο κριτήριο σε οποιοδήποτε crate επιστρέφει
/// γραμμές και μπαίνει μόνο του — τίποτα δεν μετακομίζει εδώ.
// DeliveryVerdict + compose() moved to conformance 21/09 (F-137) —
// a genuine local helper the engine needs. handlers/export.rs (its
// only other caller) now calls conformance::blob_paths::DeliveryVerdict
// directly. Re-exported here so the name stays available in this file.
pub use conformance::blob_paths::DeliveryVerdict;

// DeliveryProfileRef μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — δεν κρατάει DeliveryCheck, ελεύθερο να φύγει.
pub use lineos_types::certificate::DeliveryProfileRef;

// Μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — καθαρό, δεν κρατάει DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::StoredQuality;

// AudioOrigin + StoredProvenance μετακόμισαν στο lineos-types στις 20/09 (certificate.rs) — καθαρά, δεν κρατάνε DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::{AudioOrigin, StoredProvenance};

// StemFingerprints, BandSpatial, StoredSpatial μετακόμισαν στο lineos-types στις 20/09 (certificate.rs) — καθαρά, δεν κρατάνε DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::{BandSpatial, StemFingerprints, StoredSpatial};

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

// raw_dump_path moved to conformance 21/09 (F-137) — a genuine local
// helper the engine needs. decode_node.rs's and run_dsp_internal's
// calls keep working unchanged via this re-export.
pub use conformance::blob_paths::raw_dump_path;

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

// mastered_path moved to conformance 21/09 (F-137) — zero other
// caller besides execute_streaming_plan itself.
pub use conformance::blob_paths::mastered_path;

/// VAD trace CSV — diagnostic sidecar of the render node.
pub fn vad_trace_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "vad-trace-{}.csv",
        sanitize_path_component(blob_id)
    ))
}

pub(crate) fn sha256_file(path: &std::path::Path) -> Result<String, SidecarError> {
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
/// Υπογράφει έναν φάκελο JSON **byte-detached** (v0 §6/Σ1α) και γυρίζει τα
/// τελικά bytes.
///
/// ⚠ ΤΟ ΤΕΛΕΤΟΥΡΓΙΚΟ ΕΙΝΑΙ ΛΕΠΤΟ ΚΑΙ ΓΡΑΦΕΤΑΙ ΜΙΑ ΦΟΡΑ: serialize με
/// `payload_signature: ""`, υπογραφή πάνω σε ΑΥΤΑ ΑΚΡΙΒΩΣ τα bytes, και
/// splice με string replace στα ΙΔΙΑ bytes — ΟΧΙ δεύτερο serialize. Ένα
/// δεύτερο serialize μπορεί να αναδιατάξει ό,τι αγγίζει ο serde, και η
/// υπογραφή θα πέθαινε ΣΙΩΠΗΛΑ (θα επαλήθευε bytes που δεν υπάρχουν πια).
/// Το άγκιστρο πρέπει να εμφανίζεται ΑΚΡΙΒΩΣ μία φορά, αλλιώς αρνούμαστε
/// να υπογράψουμε στα τυφλά.
///
/// ⚠ ΧΡΕΟΣ, ΔΗΛΩΜΕΝΟ 2026-09-06: η `write_sidecar` παραπάνω κρατάει ΤΟ
/// ΔΙΚΟ ΤΗΣ inline αντίγραφο αυτού του τελετουργικού. ΔΕΝ ενοποιήθηκε
/// εδώ — θα άγγιζε τη διαδρομή του master cert, που αυτό το βήμα δεν
/// αγγίζει. Η ενοποίηση είναι ξεχωριστό βήμα, με απόφαση.
pub(crate) fn sign_json_envelope<T: Serialize>(
    envelope: &T,
) -> Result<Vec<u8>, SidecarError> {
    let identity = crate::identity::load_or_generate_default()
        .map_err(|e| SidecarError::Io(e.to_string()))?;

    let unsigned_json =
        serde_json::to_vec_pretty(envelope).map_err(|e| SidecarError::Serde(e.to_string()))?;
    let signature = identity.sign(&unsigned_json);
    let sig_b64 = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    };
    let unsigned_str = String::from_utf8(unsigned_json)
        .map_err(|e| SidecarError::Serde(format!("sidecar bytes not valid UTF-8: {e}")))?;
    let anchor = "\"payload_signature\": \"\"";
    let occurrences = unsigned_str.matches(anchor).count();
    if occurrences != 1 {
        return Err(SidecarError::SignatureAnchorNotUnique { found: occurrences });
    }
    let replacement = format!("\"payload_signature\": \"{sig_b64}\"");
    Ok(unsigned_str.replacen(anchor, &replacement, 1).into_bytes())
}

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
                corrections: Vec::new(),
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
// StoredBlobCore μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — καθαρό, δεν κρατάει DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::StoredBlobCore;

// NamedValue + CorrectionRecord μετακόμισαν στο lineos-types στις 20/09 (certificate.rs) — καθαρά, δεν κρατάνε DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::{CorrectionRecord, NamedValue};

// UncertifiedReason (με το impl του, as_public_str) μετακόμισε στο lineos-types στις 20/09 (certificate.rs) — καθαρό match, δεν κρατάει DeliveryCheck/DeliveryProfileRef.
pub use lineos_types::certificate::UncertifiedReason;

// BlobVariant + StoredBlobV2 (με το impl του, οι δεκατέσσερις προσβάσεις) μετακόμισαν στο
// lineos-types στις 20/09 (certificate.rs) — μαζί με το DeadAirSummary/DeadAirEvent (δες
// dsp/signal_health.rs), το μόνο πεδίο του BlobVariant που δεν ζούσε ήδη εδώ ή στο
// lineos-types. Το SignalHealthMonitor, που ΠΑΡΑΓΕΙ το DeadAirSummary αγγίζοντας σήμα, μένει.
pub use lineos_types::certificate::{BlobVariant, StoredBlobV2};

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

