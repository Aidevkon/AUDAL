//! POST /export — export Golden Blob audio as WAV, FLAC, Opus, or MP3.
//! Authority: Phase 10 task-decomposition P10-002/P10-003/P10-004
//!             Phase 13 P13-003 — MP3 via LAME (LGPL, dynamic linking)
//!
//! Architecture (binding):
//!   Tauri command export_audio(blob_id, format, output_path)
//!       → POST /export (this handler)
//!       → blob_store.get(blob_id) → StoredBlob.audio_bytes (f32 LE PCM)
//!       ├── WAV:  decode bytes → hound write (32-bit float, 48kHz)
//!       ├── FLAC: write audio_bytes directly (zero re-encoding)
//!       ├── Opus: decode bytes → audiopus encode → write
//!       └── MP3:  decode bytes → LAME encode → write (LGPL dynamic link)
//!       → sidecar .stillair.json alongside audio file
//!
//! FORBIDDEN (per Phase 10 master prompt + Amendment A-002 §3 + Phase 13):
//!   ❌ FFmpeg or any subprocess
//!   ❌ Re-running DSP pipeline during export
//!   ❌ Re-measuring loudness during export
//!   ❌ Modifying the Golden Blob
//!   ❌ Returning raw audio bytes to the frontend
//!   ❌ Static linking of LAME (LGPL violation)
//!
//! [2026-09-20] The signal-side writers (export_flac/wav/opus/aiff/adm_bwf,
//! export_mp3, export_mp3_acx, write_unsigned_export_sidecar, and
//! delivery_checks_verdict) moved to conformance::export — they touch no
//! blob_store storage/signing primitive. What stays here needs
//! AppState/Audit/get_or_rehydrate (handler-only), or calls
//! blob_store::sha256_file/find_sidecar/sign_json_envelope and
//! crate::identity (write_deliverable_cert, export_mp3_routed, export_blob).

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::blob_store::StoredBlobV2;
use crate::handlers::blob::{get_or_rehydrate, RehydrateError};

// ── Request/Response types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    pub blob_id: String,
    pub format: String,      // "wav" | "flac" | "opus" | "mp3"
    pub output_path: String, // absolute path chosen by user via native dialog
}

#[derive(Debug, Serialize)]
pub struct ExportResponse {
    pub status: String, // "ok" | "error"
    pub written_path: Option<String>,
    pub message: Option<String>,
    /// Η ΕΤΥΜΗΓΟΡΙΑ ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ — μετρημένο · απαιτούμενο · margin ·
    /// pass/fail ανά μετρική, στο ΙΔΙΟ σχήμα §5.3 που γράφει το
    /// `/projects/:id/deliver`, από τον ΙΔΙΟ κανόνα (`margin_checks`).
    ///
    /// ⚠ ΕΙΝΑΙ ΔΙΠΛΑ ΣΤΟ `status`, ΟΧΙ ΑΝΤΙ ΓΙ' ΑΥΤΟ. Το `status` λέει αν
    /// **το export** πέτυχε· αυτό λέει αν **το αρχείο** συμμορφώνεται. Ένα
    /// αρχείο εκτός προδιαγραφής γράφεται κανονικά και επιστρέφει
    /// `status: "ok"` — ο χρήστης παίρνει το αρχείο ΚΑΙ την αλήθεια.
    ///
    /// `None` = ο προορισμός δεν ορίζει τέτοιους ελέγχους (κάθε μη-ACX
    /// διαδρομή σήμερα). Απουσία, όχι κενή λίστα: κενή λίστα θα δήλωνε
    /// «ελέγχθηκε, τίποτα να πούμε».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_checks: Option<Vec<crate::blob_store::DeliveryCheck>>,
    /// Η ΣΥΝΟΛΙΚΗ ΕΤΥΜΗΓΟΡΙΑ — fold πάνω στις παραπάνω γραμμές.
    ///
    /// ΓΙΑΤΙ ΜΠΑΙΝΕΙ ΚΑΙ ΔΕΝ ΑΦΗΝΕΤΑΙ ΣΤΟΝ ΚΑΤΑΝΑΛΩΤΗ: επιστρέφοντας
    /// γραμμές χωρίς σύνοψη, προσκαλούσαμε κάθε αναγνώστη να συνθέσει
    /// μόνος του — και **θα το έκανε λάθος**. Το ξέρουμε γιατί έγινε:
    /// το πρώτο πράγμα που έσπασε με την είσοδο του `advisory` ήταν
    /// ένας συναθροιστής με `!= "pass"`, που ανέφερε «FAIL» για αρχείο
    /// πλήρως συμμορφούμενο.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_verdict: Option<crate::blob_store::DeliveryVerdict>,
}

// ── Format enum ───────────────────────────────────────────────────────────────

pub enum ExportFormat {
    Wav,
    Flac,
    Opus,
    /// MP3 via LAME — LGPL, dynamic linking only.
    /// See: docs/licenses/LAME-LGPL-NOTICE.md
    Mp3,
    /// AIFF — Logic Pro native, uncompressed 32-bit float BE PCM.
    /// Pure Rust, no new crate. P13-003b.
    Aiff,
    /// ADM BWF — Apple Spatial Audio, 6-channel 24-bit LPCM.
    /// RIFF container with bext + WAVE_FORMAT_EXTENSIBLE. Spatial-4a.
    AdmBwf,
}

impl ExportFormat {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "wav" => Ok(Self::Wav),
            "flac" => Ok(Self::Flac),
            "opus" => Ok(Self::Opus),
            // MP3 via LAME — LGPL dynamic linking only (see docs/licenses/LAME-LGPL-NOTICE.md)
            "mp3" => Ok(Self::Mp3),
            // AIFF — Logic Pro native, uncompressed 32-bit float big-endian
            "aiff" | "aif" => Ok(Self::Aiff),
            "adm_bwf" | "admbwf" | "bwf" => Ok(Self::AdmBwf),
            other => Err(format!(
                "Unsupported format: {other}. Use wav/flac/opus/mp3/aiff/adm_bwf"
            )),
        }
    }

    #[allow(dead_code)]
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Flac => "flac",
            Self::Opus => "opus",
            Self::Mp3 => "mp3",
            Self::Aiff => "aiff",
            Self::AdmBwf => "wav",
        }
    }
}

// ── Axum handler ──────────────────────────────────────────────────────────────

/// POST /export — read Golden Blob, write audio to disk, return written_path.
/// Audio bytes remain in M0 storage — only the path is returned.
pub async fn export_audio(
    State(state): State<AppState>,
    Json(req): Json<ExportRequest>,
) -> Json<ExportResponse> {
    // Validate format
    let format = match ExportFormat::from_str(&req.format) {
        Ok(f) => f,
        Err(e) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(e),
            })
        }
    };

    // Fetch blob
    let blob = match get_or_rehydrate(&state, &req.blob_id).await {
        Ok(b) => b,
        Err(RehydrateError::NotFound) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(format!("blob not found: {}", req.blob_id)),
            })
        }
        Err(RehydrateError::Io(e)) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(format!("blob io error {}: {e}", req.blob_id)),
            })
        }
        Err(RehydrateError::Corrupt(e)) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(format!("blob corrupt {}: {e}", req.blob_id)),
            })
        }
    };

    let output_path = std::path::PathBuf::from(&req.output_path);

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(format!("Cannot create export directory: {e}")),
            });
        }
    }

    // Export audio (blocking I/O)
    let blob_clone = blob.clone();
    let path_str = req.output_path.clone();
    let format_str = req.format.clone();
    let path_for_io = output_path.clone();
    // Ο δεσμός προς τον master χρειάζεται τη ρίζα των masters. Είναι ήδη
    // στο config — ίδια πηγή με το /deliver (deliver.rs:675).
    let masters_for_io = state.config.masters_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Η ετυμηγορία γεννιέται στον writer (μόνο εκείνος μετράει το
        // ΤΕΛΙΚΟ σήμα) και ταξιδεύει ΚΑΙ στο sidecar ΚΑΙ στην απόκριση —
        // μία μέτρηση, δύο αναγνώστες.
        let checks = export_blob(&blob_clone, format, &path_for_io, Some(&masters_for_io))?;
        conformance::export::write_unsigned_export_sidecar(
            &blob_clone,
            &format_str,
            &path_for_io,
            checks.as_deref(),
        )?;
        Ok(checks)
    })
    .await
    .map_err(|e| format!("Export task join error: {e}"))
    .and_then(|r: Result<_, String>| r);

    // Ο ΣΥΝΘΕΤΗΣ — ένας, δύο καλούντες (εδώ και στο /deliver).
    let delivery_verdict = delivery_checks_verdict(&blob, &result);

    match result {
        Ok(delivery_checks) => {
            state
                .audit
                .write(AuditEntry::new(
                    "m0d.export_complete",
                    AuditLevel::Audit,
                    &format!("format={} path={path_str}", req.format),
                ))
                .ok();
            Json(ExportResponse {
                status: "ok".into(),
                written_path: Some(path_str),
                message: None,
                delivery_checks,
                delivery_verdict,
            })
        }
        Err(e) => {
            state
                .audit
                .write(AuditEntry::new("m0d.export_failed", AuditLevel::Audit, &e))
                .ok();
            Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                delivery_verdict: None,
                message: Some(e),
            })
        }
    }
}

// ── P10-002: Export format writers ────────────────────────────────────────────

/// Ο ΣΥΝΘΕΤΗΣ στη διαδρομή του `/export`: βρίσκει το συμβόλαιο του
/// προορισμού και κάνει το fold. `None` όπου δεν υπάρχουν γραμμές —
/// δεν κατασκευάζεται ετυμηγορία από το πουθενά.
fn delivery_checks_verdict(
    blob: &StoredBlobV2,
    result: &Result<Option<Vec<crate::blob_store::DeliveryCheck>>, String>,
) -> Option<crate::blob_store::DeliveryVerdict> {
    let checks = result.as_ref().ok()?.as_ref()?;
    let spec = &lineos_types::presets::lookup(&blob.core.preset_id)?.delivery;
    Some(crate::blob_store::DeliveryVerdict::compose(spec, checks))
}

/// Route to format-specific writer.
///
/// Επιστρέφει την **ετυμηγορία του παραδοτέου** όπου ο προορισμός ορίζει
/// ελέγχους· `None` όπου δεν ορίζει. Οι μορφές που δεν κουβαλούν
/// προδιαγραφή παράδοσης επιστρέφουν `Ok(None)` — απουσία, όχι κενή λίστα.
fn export_blob(
    blob: &StoredBlobV2,
    format: ExportFormat,
    path: &Path,
    masters_dir: Option<&str>,
) -> Result<Option<Vec<crate::blob_store::DeliveryCheck>>, String> {
    match format {
        ExportFormat::Flac => conformance::export::export_flac(blob, path).map(|()| None),
        ExportFormat::Wav => conformance::export::export_wav(blob, path).map(|()| None),
        ExportFormat::Opus => conformance::export::export_opus(blob, path).map(|()| None),
        // MP3: LAME encoder — LGPL dynamic linking only (see docs/licenses/LAME-LGPL-NOTICE.md)
        ExportFormat::Mp3 => export_mp3_routed(blob, path, masters_dir),
        // AIFF: uncompressed 32-bit float big-endian PCM (P13-003b)
        ExportFormat::Aiff => conformance::export::export_aiff(blob, path).map(|()| None),
        // ADM BWF: Apple Spatial Audio, 6-channel 24-bit LPCM (Spatial-4a)
        ExportFormat::AdmBwf => conformance::export::export_adm_bwf(blob, path).map(|()| None),
    }
}

/// MP3 δρομολόγηση: ACX παραδοτέο ή γενικό MP3.
///
/// Η δρομολόγηση γίνεται από ΤΗ ΠΡΟΔΙΑΓΡΑΦΗ ΠΑΡΑΔΟΣΗΣ, όχι από το
/// preset_id string — το rms_window_db είναι σήμερα ACX-only
/// (presets.rs:76-82, δηλωμένο «today» εκεί).
/// Αν αύριο άλλος προορισμός αποκτήσει παράθυρο RMS, ΑΥΤΗ Η ΓΡΑΜΜΗ
/// ΘΕΛΕΙ ΞΑΝΑΚΟΙΤΑΓΜΑ.
///
/// Η ταφόπλακα του `preset_id` (blob_store.rs:1019 — «ΜΗΝ προσθέσεις
/// λογική που το θεωρεί ενιαίο») τηρείται: δεν συγκρίνουμε string,
/// ρωτάμε τον μεταφραστή. Ίδιο μοτίβο με `LoudnessTarget::from_preset`.
///
/// Άγνωστο preset ⇒ `lookup` → `None` ⇒ γενικό MP3. Η ΑΡΝΗΣΗ ΕΙΝΑΙ Η
/// ΑΣΦΑΛΗΣ ΠΛΕΥΡΑ: ένα 48k stereo mp3 είναι λάθος αρχείο για το ACX,
/// αλλά ένα ACX-μορφοποιημένο αρχείο για κάποιον που δεν το ζήτησε
/// είναι σιωπηλό mono-fold + resample που κανείς δεν διάλεξε.
// ── ΤΟ CERT ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ (F-090) ───────────────────────────────────
//
// ΤΟ ΠΡΟΒΛΗΜΑ, ΜΕΤΡΗΜΕΝΟ: ο χρήστης κρατάει mp3 44.1k mono και δίπλα του
// ένα έγγραφο που περιγράφει τον master — 48k stereo, peak πριν τον
// encoder. ΚΑΝΕΝΑ μέγεθος του master cert δεν ισχύει για το εξαγόμενο
// αρχείο· τα στερεοφωνικά πεδία είναι ΑΝΕΥ ΝΟΗΜΑΤΟΣ σε mono.
//
// Η ΑΠΟΦΑΣΗ (06/09): ΔΥΟ ΣΥΝΔΕΔΕΜΕΝΑ έγγραφα. Το master cert μένει
// ΑΜΕΤΑΒΛΗΤΟ· αυτό εδώ περιγράφει ΜΟΝΟ το παραδοτέο και δένεται πάνω
// του με δύο δεσμούς.
//
// ΤΕΚΜΗΡΙΟ ΥΠΕΡ ΤΗΣ ΜΟΡΦΗΣ: το C2PA προσθέτει ΝΕΟ manifest ανά
// επεξεργασία, σχηματίζοντας αλυσίδα (spec.c2pa.org, ανακτ. 06/09).
// Δεν εφευρίσκουμε — ακολουθούμε.
//
// ⚠⚠ Ο ΚΑΝΟΝΑΣ ΑΠΟΥΣΙΑΣ §5.2, ΣΕ ΕΠΙΠΕΔΟ ΕΓΓΡΑΦΟΥ: ΑΝ ΔΕΝ ΜΕΤΡΗΘΗΚΕ
// ΣΤΟ ΠΑΡΑΔΟΤΕΟ, ΔΕΝ ΜΠΑΙΝΕΙ. Ούτε LUFS, ούτε LRA, ούτε quality block,
// ούτε stereo πεδία, ούτε οι πέντε συμμορφώσεις μουσικής. Κάθε μέγεθος
// εδώ μετρήθηκε πάνω στο αρχείο που παραδίδεται.

/// Ό,τι διαβάστηκε ΑΠΟ ΤΟ ΠΑΡΑΓΟΜΕΝΟ ΑΡΧΕΙΟ, μετά τον encoder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverableTechnical {
    /// symphonia decode-back του mp3 — όχι το `blob.core.sample_rate`.
    pub sample_rate: u32,
    pub channels: u16,
    /// bytes×8/secs του τελικού αρχείου.
    pub bitrate_kbps: f32,
}

/// Ο ΔΕΣΜΟΣ προς τον master. Δύο, γιατί απαντούν σε δύο ερωτήματα.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterBond {
    pub blob_id: String,
    /// SHA-256 του master FLAC — δεσμός προς το ΕΓΓΡΑΦΟ.
    /// `None` = το master FLAC δεν βρέθηκε τη στιγμή του export.
    /// ΓΡΑΦΕΤΑΙ ΩΣ ΑΠΟΝ, δεν παραλείπεται: δεσμός προς `None` δεν είναι
    /// δεσμός, και ο αναγνώστης πρέπει να το ξέρει ΧΩΡΙΣ να ρωτήσει.
    pub master_sha256: Option<String>,
    /// BLAKE3 του master PCM — ταυτότητα ΗΧΟΥ, επιβιώνει αλλαγής
    /// container. `None` = δεν μετρήθηκε στον master (είναι `Option` και
    /// στο `StoredBlobCore`).
    pub master_pcm_blake3: Option<String>,
}

/// Το υπογεγραμμένο cert ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ.
///
/// Ίδιος φάκελος υπογραφής με το master cert (`key_id` ·
/// `signer_public_key` · `payload_signature` byte-detached), ώστε ο
/// ΜΟΝΟΣ υπάρχων αναγνώστης — το `scripts/verify_cert.py` — να το
/// διαβάζει ΧΩΡΙΣ καμία αλλαγή: απαιτεί μοναδικό άγκιστρο
/// `payload_signature`, `signer_public_key` 64 hex, και έγκυρο JSON.
/// Τίποτα από το σχήμα.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverableCertificate {
    pub format: String,
    pub format_version: u32,
    pub written_at: String,
    pub engine_version: String,
    pub engine_commit: String,
    /// SHA-256 ΤΟΥ ΑΡΧΕΙΟΥ ΠΟΥ ΠΑΡΑΔΙΔΕΤΑΙ. Υπολογισμένο ΜΕΤΑ τη
    /// συγγραφή, με την ΥΠΑΡΧΟΥΣΑ `blob_store::sha256_file` — μία
    /// υλοποίηση hash, όχι δεύτερη.
    pub deliverable_sha256: String,
    pub technical: DeliverableTechnical,
    /// Η ΤΙΜΗ του προορισμού, όχι όνομα πεδίου (§5.1α).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_target: Option<String>,
    /// Οι γραμμές §5.3, μετρημένες πάνω στο παραδοτέο.
    pub delivery_checks: Vec<crate::blob_store::DeliveryCheck>,
    /// Το fold πάνω στις γραμμές — ίδιος συνθέτης, καμία νέα κρίση.
    pub delivery_verdict: crate::blob_store::DeliveryVerdict,
    /// Δηλώνει αν ο μετρητής είναι και ο παραγωγός.
    pub audio_origin: crate::blob_store::AudioOrigin,
    pub master: MasterBond,
    pub key_id: String,
    pub signer_public_key: String,
    /// ΤΟ ΑΓΚΙΣΤΡΟ ΤΗΣ ΥΠΟΓΡΑΦΗΣ. Serialize με "" ⇒ υπογραφή ⇒ splice.
    pub payload_signature: String,
}

pub const DELIVERABLE_CERT_FORMAT: &str = "creator-os-deliverable-cert";
// ΔΕΝ είναι κατώφλι — είναι η μεμβράνη του σχήματος, όπως το
// SIDECAR_FORMAT_VERSION. Ο φρουρός κατωφλιών το πιάνει επειδή είναι
// αριθμητική σταθερά, και σφραγίζεται για να μη μείνει ασφράγιστο.
// PLACEHOLDER: πρώτη έκδοση του σχήματος του παραδοτέου.
// TRIGGER: κάθε αλλαγή πεδίου — νέα έκδοση, ΠΟΤΕ σιωπηλή μετάλλαξη.
pub const DELIVERABLE_CERT_FORMAT_VERSION: u32 = 1;

/// Γράφει το υπογεγραμμένο cert του παραδοτέου **δίπλα στο αρχείο**.
///
/// ΟΝΟΜΑ: `<αρχείο>.deliverable.json`.
/// ⚠ ΔΙΑΦΟΡΕΤΙΚΗ ΚΑΤΑΛΗΞΗ ΑΠΟ ΤΟ `.stillair.json`, ΣΚΟΠΙΜΑ. Το
/// ανυπόγραφο `ExportSidecar` ΜΕΝΕΙ ΩΣ ΕΧΕΙ σε αυτό το βήμα· δύο αρχεία
/// με ΤΟ ΙΔΙΟ όνομα θα ήταν σύγκρουση, δύο με το ίδιο ΝΟΗΜΑ είναι
/// σύγχυση. Ο καθαρισμός του `ExportSidecar` είναι ΞΕΧΩΡΙΣΤΟ βήμα, με
/// απόφαση.
#[allow(clippy::too_many_arguments)]
fn write_deliverable_cert(
    blob: &StoredBlobV2,
    audio_path: &Path,
    technical: DeliverableTechnical,
    delivery_target: Option<String>,
    checks: &[crate::blob_store::DeliveryCheck],
    spec: &lineos_types::presets::DeliverySpec,
    masters_dir: &str,
) -> Result<std::path::PathBuf, String> {
    // (Α) ΤΟ HASH ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ — η ΥΠΑΡΧΟΥΣΑ συνάρτηση, στο τελικό
    // αρχείο, ΜΕΤΑ τη συγγραφή του.
    let deliverable_sha256 = crate::blob_store::sha256_file(audio_path)
        .map_err(|e| format!("deliverable hash failed: {e:?}"))?;

    // (Γ) Ο ΔΕΣΜΟΣ. Το master FLAC ζει δίπλα στο sidecar του master —
    // ίδιο μοτίβο με το /deliver.
    let master_sha256 = match crate::blob_store::find_sidecar(masters_dir, &blob.core.id) {
        Ok(Some(sidecar_path)) => {
            let master_flac = sidecar_path.with_extension("flac");
            if master_flac.exists() {
                crate::blob_store::sha256_file(&master_flac).ok()
            } else {
                None
            }
        }
        _ => None,
    };

    let identity = crate::identity::load_or_generate_default()
        .map_err(|e| format!("identity load failed: {e}"))?;

    let cert = DeliverableCertificate {
        format: DELIVERABLE_CERT_FORMAT.to_string(),
        format_version: DELIVERABLE_CERT_FORMAT_VERSION,
        written_at: Utc::now().to_rfc3339(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        engine_commit: env!("GIT_HASH").to_string(),
        deliverable_sha256,
        technical,
        delivery_target,
        delivery_checks: checks.to_vec(),
        delivery_verdict: crate::blob_store::DeliveryVerdict::compose(spec, checks),
        audio_origin: blob
            .provenance()
            .map(|p| p.audio_origin)
            .unwrap_or_default(),
        master: MasterBond {
            blob_id: blob.core.id.clone(),
            master_sha256,
            master_pcm_blake3: blob.core.pcm_blake3.clone(),
        },
        key_id: identity.key_id.clone(),
        signer_public_key: identity.public_key_hex(),
        payload_signature: String::new(),
    };

    let json = crate::blob_store::sign_json_envelope(&cert)
        .map_err(|e| format!("deliverable cert signing failed: {e:?}"))?;

    let cert_path = audio_path.with_extension("deliverable.json");
    std::fs::write(&cert_path, &json)
        .map_err(|e| format!("deliverable cert write failed: {e}"))?;
    Ok(cert_path)
}

fn export_mp3_routed(
    blob: &StoredBlobV2,
    path: &Path,
    // `None` = ο καλών δεν έχει masters_dir (π.χ. test χωρίς store) ⇒
    // ΚΑΝΕΝΑ deliverable cert. Απουσία, όχι σιωπή.
    masters_dir: Option<&str>,
) -> Result<Option<Vec<crate::blob_store::DeliveryCheck>>, String> {
    let is_acx_delivery = lineos_types::presets::lookup(&blob.core.preset_id)
        .and_then(|p| p.delivery.rms_window_db)
        .is_some();

    if !is_acx_delivery {
        return conformance::export::export_mp3(blob, path).map(|()| None);
    }

    // ΤΟ OUTCOME ΔΕΝ ΠΕΤΙΕΤΑΙ ΚΑΙ ΠΛΕΟΝ ΔΕΝ ΜΕΝΕΙ ΣΤΟ LOG. Η
    // `export_mp3_acx` μετράει το ΤΕΛΙΚΟ σήμα — μετά το resample, μετά το
    // downmix, μετά από κάθε επέμβαση — και είναι η ΜΟΝΗ μέτρηση του
    // πραγματικού παραδοτέου σε αυτή τη διαδρομή. Η ετυμηγορία της
    // ανεβαίνει τώρα ως τιμή επιστροφής: sidecar + απόκριση.
    //
    // ⚠ ΔΕΝ γίνεται `Err` όταν το αρχείο δεν συμμορφώνεται. Το export
    // ΠΕΤΥΧΕ — παρήγαγε το αρχείο. Το αν το ΑΡΧΕΙΟ περνάει είναι
    // διαφορετικό πράγμα και ταξιδεύει ΔΙΠΛΑ στο `status`, όχι αντί του.
    let outcome = conformance::export::export_mp3_acx(blob, path)?;
    tracing::info!(
        event            = "m0d.export_acx_measured",
        blob_id          = %blob.core.id,
        preset_id        = %blob.core.preset_id,
        head_quiet_secs  = outcome.head_quiet_secs,
        tail_quiet_secs  = outcome.tail_quiet_secs,
        report           = ?outcome.report,
        "export: ACX deliverable written and measured"
    );
    // ΔΥΟ ΠΑΡΑΓΩΓΟΙ, ΣΚΟΠΙΜΑ ΧΩΡΙΣΤΟΙ. Το spacing ΔΕΝ μπαίνει μέσα στο
    // `margin_checks()`: εκείνο τροφοδοτεί τη ΣΥΝΟΛΙΚΗ κρίση
    // (`levels_within_limits_with_margin`, acx_check.rs:188) και ένα `advisory`
    // εκεί θα τη μόλυνε. Χωριστά, η μόλυνση είναι δομικά αδύνατη.
    // ΤΟ ΣΥΜΒΟΛΑΙΟ ΤΟΥ ΠΡΟΟΡΙΣΜΟΥ ΠΟΥ ΖΗΤΗΘΗΚΕ — δυναμικά, από το preset.
    // Εδώ (σε αντίθεση με την export_mp3_acx) η δρομολόγηση ΗΔΗ έγινε με
    // βάση το preset, άρα το spec που διάλεξε τη διαδρομή είναι το ίδιο
    // που κρίνει το αποτέλεσμα. Μία απόφαση, μία πηγή.
    let spec = &lineos_types::presets::lookup(&blob.core.preset_id)
        .expect("η δρομολόγηση πιο πάνω το βρήκε ήδη")
        .delivery;
    let mut checks = conformance::declare::delivery_checks_from_margin_checks(&outcome.report);
    checks.extend(crate::blob_store::DeliveryCheck::from_spacing(
        spec,
        outcome.head_quiet_secs,
        outcome.tail_quiet_secs,
    ));
    checks.extend(crate::blob_store::DeliveryCheck::from_format(
        spec,
        outcome.delivered_sample_rate,
        outcome.delivered_channels,
        outcome.delivered_bitrate_kbps,
    ));
    // ΤΟ CERT ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ γράφεται ΕΔΩ και όχι ψηλότερα, γιατί ΕΔΩ
    // υπάρχουν στο scope ΚΑΙ τα τρία: το `outcome` (decode-back), οι
    // γραμμές, και το `spec` που τις έκρινε. Ψηλότερα θα έπρεπε να
    // ταξιδέψουν ως νέος τύπος επιστροφής — περισσότερη επιφάνεια για
    // την ίδια πληροφορία.
    if let Some(md) = masters_dir {
        let technical = DeliverableTechnical {
            sample_rate: outcome.delivered_sample_rate,
            channels: outcome.delivered_channels,
            bitrate_kbps: outcome.delivered_bitrate_kbps,
        };
        let target = lineos_types::presets::lookup(&blob.core.preset_id)
            .map(|p| p.delivery.platform.to_string());
        match write_deliverable_cert(blob, path, technical, target, &checks, spec, md) {
            Ok(cert_path) => tracing::info!(
                event = "m0d.deliverable_cert_written",
                path = %cert_path.display(),
                "export: deliverable certificate written and signed"
            ),
            // ΔΕΝ σκοτώνει το export: το αρχείο ΓΡΑΦΤΗΚΕ. Αλλά ΔΕΝ
            // σιωπά — μια απόδειξη που δεν γράφτηκε είναι γεγονός.
            Err(e) => tracing::error!(
                event = "m0d.deliverable_cert_failed",
                error = %e,
                "export: deliverable certificate NOT written"
            ),
        }
    }
    Ok(Some(checks))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_from_str() {
        assert!(matches!(
            ExportFormat::from_str("wav"),
            Ok(ExportFormat::Wav)
        ));
        assert!(matches!(
            ExportFormat::from_str("flac"),
            Ok(ExportFormat::Flac)
        ));
        assert!(matches!(
            ExportFormat::from_str("opus"),
            Ok(ExportFormat::Opus)
        ));
        assert!(matches!(
            ExportFormat::from_str("mp3"),
            Ok(ExportFormat::Mp3)
        ));
        assert!(matches!(
            ExportFormat::from_str("aiff"),
            Ok(ExportFormat::Aiff)
        ));
        assert!(matches!(
            ExportFormat::from_str("aif"),
            Ok(ExportFormat::Aiff)
        ));
        assert!(matches!(
            ExportFormat::from_str("WAV"),
            Ok(ExportFormat::Wav)
        ));
        assert!(ExportFormat::from_str("aac").is_err());
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Wav.extension(), "wav");
        assert_eq!(ExportFormat::Flac.extension(), "flac");
        assert_eq!(ExportFormat::Opus.extension(), "opus");
        assert_eq!(ExportFormat::Mp3.extension(), "mp3");
        assert_eq!(ExportFormat::Aiff.extension(), "aiff");
    }
}
