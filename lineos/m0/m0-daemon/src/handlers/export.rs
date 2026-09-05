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

use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::app_state::AppState;
use crate::audit::{AuditEntry, AuditLevel};
use crate::blob_store::{StoredBlobV2, StoredLoudness, StoredQuality};
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
}

// ── Format enum ───────────────────────────────────────────────────────────────

pub enum ExportFormat {
    Wav,
    Flac,
    Opus,
    /// MP3 via LAME — LGPL, dynamic linking only.
    /// See: lineos/plan/phase-13/LAME-LGPL-NOTICE.md
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
            // MP3 via LAME — LGPL dynamic linking only (see LAME-LGPL-NOTICE.md)
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
                message: Some(format!("blob not found: {}", req.blob_id)),
            })
        }
        Err(RehydrateError::Io(e)) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
                message: Some(format!("blob io error {}: {e}", req.blob_id)),
            })
        }
        Err(RehydrateError::Corrupt(e)) => {
            return Json(ExportResponse {
                status: "error".into(),
                written_path: None,
                delivery_checks: None,
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
                message: Some(format!("Cannot create export directory: {e}")),
            });
        }
    }

    // Export audio (blocking I/O)
    let blob_clone = blob.clone();
    let path_str = req.output_path.clone();
    let format_str = req.format.clone();
    let path_for_io = output_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        // Η ετυμηγορία γεννιέται στον writer (μόνο εκείνος μετράει το
        // ΤΕΛΙΚΟ σήμα) και ταξιδεύει ΚΑΙ στο sidecar ΚΑΙ στην απόκριση —
        // μία μέτρηση, δύο αναγνώστες.
        let checks = export_blob(&blob_clone, format, &path_for_io)?;
        write_sidecar(
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
                message: Some(e),
            })
        }
    }
}

// ── P10-002: Export format writers ────────────────────────────────────────────

/// Route to format-specific writer.
///
/// Επιστρέφει την **ετυμηγορία του παραδοτέου** όπου ο προορισμός ορίζει
/// ελέγχους· `None` όπου δεν ορίζει. Οι μορφές που δεν κουβαλούν
/// προδιαγραφή παράδοσης επιστρέφουν `Ok(None)` — απουσία, όχι κενή λίστα.
fn export_blob(
    blob: &StoredBlobV2,
    format: ExportFormat,
    path: &Path,
) -> Result<Option<Vec<crate::blob_store::DeliveryCheck>>, String> {
    match format {
        ExportFormat::Flac => export_flac(blob, path).map(|()| None),
        ExportFormat::Wav => export_wav(blob, path).map(|()| None),
        ExportFormat::Opus => export_opus(blob, path).map(|()| None),
        // MP3: LAME encoder — LGPL dynamic linking only (see LAME-LGPL-NOTICE.md)
        ExportFormat::Mp3 => export_mp3_routed(blob, path),
        // AIFF: uncompressed 32-bit float big-endian PCM (P13-003b)
        ExportFormat::Aiff => export_aiff(blob, path).map(|()| None),
        // ADM BWF: Apple Spatial Audio, 6-channel 24-bit LPCM (Spatial-4a)
        ExportFormat::AdmBwf => export_adm_bwf(blob, path).map(|()| None),
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
fn export_mp3_routed(
    blob: &StoredBlobV2,
    path: &Path,
) -> Result<Option<Vec<crate::blob_store::DeliveryCheck>>, String> {
    let is_acx_delivery = lineos_types::presets::lookup(&blob.core.preset_id)
        .and_then(|p| p.delivery.rms_window_db)
        .is_some();

    if !is_acx_delivery {
        return export_mp3(blob, path).map(|()| None);
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
    let outcome = export_mp3_acx(blob, path)?;
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
    // (`passes_acx_with_margin`, acx_check.rs:188) και ένα `advisory`
    // εκεί θα τη μόλυνε. Χωριστά, η μόλυνση είναι δομικά αδύνατη.
    let mut checks = crate::blob_store::DeliveryCheck::from_margin_checks(&outcome.report);
    checks.extend(crate::blob_store::DeliveryCheck::from_spacing(
        outcome.head_quiet_secs,
        outcome.tail_quiet_secs,
    ));
    Ok(Some(checks))
}

/// FLAC export — real encoding via io_flac (Phase 11 debt closed).
/// Shares the Tier-2 persist path's encoder: one encoder, two consumers.
/// 24-bit, deterministic round-half-even quantization, no dither — see
/// io_flac.rs for the rationale. (The old body dumped raw f32 bytes
/// under a .flac name; no FLAC reader could open it.)
pub fn export_flac(blob: &StoredBlobV2, path: &Path) -> Result<(), String> {
    let audio_bytes = std::fs::read(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to read audio from disk: {e}"))?;
    if audio_bytes.is_empty() {
        return Err("No audio bytes in file — mastering may have failed".into());
    }
    let pcm = pcm_bytes_to_f32(&audio_bytes);
    crate::io_flac::encode_f32_flac_24(&pcm, blob.core.sample_rate, blob.core.channels, path)
}

/// WAV: decode f32 LE PCM bytes → write 32-bit float WAV via hound.
fn export_wav(blob: &StoredBlobV2, path: &Path) -> Result<(), String> {
    let audio_bytes = std::fs::read(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to read audio from disk: {e}"))?;
    if audio_bytes.is_empty() {
        return Err("No audio bytes in file — cannot write WAV".into());
    }

    let samples = pcm_bytes_to_f32(&audio_bytes);

    let spec = hound::WavSpec {
        channels: blob.core.channels,
        sample_rate: blob.core.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("WAV create failed: {e}"))?;

    for &sample in &samples {
        writer
            .write_sample(sample)
            .map_err(|e| format!("WAV write sample failed: {e}"))?;
    }

    writer
        .finalize()
        .map_err(|e| format!("WAV finalize failed: {e}"))
}

/// ADM BWF: Apple Spatial Audio export.
///
/// Reads 6-channel interleaved f32 LE PCM from Golden Blob, de-interleaves
/// to planar [L, R, C, LFE, Ls, Rs], writes RIFF + WAVE_FORMAT_EXTENSIBLE
/// fmt + bext + data chunks via sp314_dsp::io::wav_writer::write_adm_bwf.
fn export_adm_bwf(blob: &StoredBlobV2, path: &Path) -> Result<(), String> {
    // ADM BWF requires exactly 6 channels
    if blob.core.channels != 6 {
        return Err(format!(
            "ADM BWF export requires 6-channel audio, got {} channels. \
             Use ExportFormat::Wav for stereo.",
            blob.core.channels
        ));
    }

    let meta = std::fs::metadata(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to stat blob PCM: {e}"))?;
    let len_bytes = meta.len();
    if len_bytes % (6 * 4) != 0 {
        return Err("corrupt raw file: byte length not a multiple of 6ch f32 frames".into());
    }
    let num_frames = (len_bytes / 24) as usize;

    use sp314_dsp::io::wav_writer::{AdmBwfStreamWriter, AdmContainerFormat};
    use std::io::Read;

    // Riff32 for now (Bw64 pending consumer validation — P39)
    let mut writer = AdmBwfStreamWriter::create(
        &path.to_string_lossy(),
        blob.core.sample_rate,
        num_frames,
        AdmContainerFormat::Riff32,
    )?;

    let file = std::fs::File::open(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to open blob PCM: {e}"))?;
    let mut reader = std::io::BufReader::new(file);

    let bytes_per_chunk = 65536 * 24;
    let mut chunk_bytes = vec![0u8; bytes_per_chunk];

    loop {
        let mut read_len = 0;
        while read_len < chunk_bytes.len() {
            let n = reader
                .read(&mut chunk_bytes[read_len..])
                .map_err(|e| format!("Read error: {e}"))?;
            if n == 0 {
                break;
            }
            read_len += n;
        }
        if read_len == 0 {
            break;
        }
        if read_len % 24 != 0 {
            return Err("corrupt read: partial frame".to_string());
        }
        let chunk_f32 = pcm_bytes_to_f32(&chunk_bytes[..read_len]);
        writer.write_interleaved_f32(&chunk_f32)?;
    }

    writer.finish()
}

/// Opus: stub — requires libopus-dev system library (Phase 11).
/// Phase 10 delivers WAV + FLAC. Opus wired in Phase 11 after libopus install.
fn export_opus(_blob: &StoredBlobV2, _path: &Path) -> Result<(), String> {
    Err("Opus export requires libopus-dev (Phase 11). \
         Use WAV, FLAC, MP3, or AIFF."
        .into())
}

/// AIFF export — Logic Pro native format.
///
/// Uncompressed 32-bit float big-endian PCM. No new crate — pure Rust.
/// Authority: Phase 13 P13-003b.
///
/// Format layout:
///   FORM chunk (total container)
///     AIFF type tag
///     COMM chunk (18 bytes): channels, num_frames, bit_depth, 80-bit sample rate
///     SSND chunk: offset(4) + blockSize(4) + big-endian PCM
///
/// No DSP re-run — reads f32 LE PCM from Golden Blob, converts to BE in-place.
fn export_aiff(blob: &StoredBlobV2, path: &Path) -> Result<(), String> {
    let audio_bytes = std::fs::read(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to read audio from disk: {e}"))?;
    if audio_bytes.is_empty() {
        return Err("No audio bytes in file — cannot write AIFF".into());
    }

    let pcm = pcm_bytes_to_f32(&audio_bytes);
    let channels = blob.core.channels.max(1);
    let sample_rate = blob.core.sample_rate;
    let num_frames = (pcm.len() / channels as usize) as u32;
    let bit_depth: u16 = 32;

    // Convert interleaved f32 LE PCM → f32 BE (AIFF is big-endian)
    let mut pcm_be: Vec<u8> = Vec::with_capacity(pcm.len() * 4);
    for &s in &pcm {
        // Reinterpret as u32 bits then write big-endian
        pcm_be.extend_from_slice(&s.to_bits().to_be_bytes());
    }

    let pcm_size = pcm_be.len() as u32;
    let ssnd_size = pcm_size + 8; // offset(4) + blockSize(4) + PCM data
    let comm_size = 18u32; // channels(2) + frames(4) + bitDepth(2) + sampleRate(10)
                           // FORM data = 4 ("AIFF") + 8 (COMM header) + 18 (COMM body) + 8 (SSND header) + ssnd_size
    let form_size = 4 + (8 + comm_size) + (8 + ssnd_size);

    let mut buf: Vec<u8> = Vec::with_capacity(12 + 8 + comm_size as usize + 8 + ssnd_size as usize);

    // ── FORM chunk header ────────────────────────────────────────────────
    buf.extend_from_slice(b"FORM");
    buf.extend_from_slice(&form_size.to_be_bytes());
    buf.extend_from_slice(b"AIFF");

    // ── COMM chunk ───────────────────────────────────────────────────────
    buf.extend_from_slice(b"COMM");
    buf.extend_from_slice(&comm_size.to_be_bytes());
    buf.extend_from_slice(&channels.to_be_bytes());
    buf.extend_from_slice(&num_frames.to_be_bytes());
    buf.extend_from_slice(&bit_depth.to_be_bytes());
    // 80-bit IEEE 754 extended for sample rate (AIFF spec requirement)
    buf.extend_from_slice(&f64_to_80bit_extended(sample_rate as f64));

    // ── SSND chunk ───────────────────────────────────────────────────────
    buf.extend_from_slice(b"SSND");
    buf.extend_from_slice(&ssnd_size.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes()); // offset (always 0)
    buf.extend_from_slice(&0u32.to_be_bytes()); // blockSize (always 0)
    buf.extend_from_slice(&pcm_be);

    tracing::info!(
        path       = %path.display(),
        bytes      = buf.len(),
        num_frames = num_frames,
        "m0d: AIFF export complete"
    );

    std::fs::write(path, &buf).map_err(|e| format!("AIFF write failed: {e}"))
}

/// Convert f64 to 80-bit IEEE 754 extended precision.
///
/// Required by the AIFF COMM chunk for the sample rate field.
/// AIFF predates IEEE 754 double — it mandates 80-bit extended (x87 format).
///
/// Format: 1 sign bit | 15-bit biased exponent | 64-bit integer mantissa
/// (no implicit leading 1 unlike double precision).
fn f64_to_80bit_extended(val: f64) -> [u8; 10] {
    let mut bytes = [0u8; 10];
    if val == 0.0 {
        return bytes;
    }

    let bits = val.to_bits();
    let sign: u16 = ((bits >> 63) as u16) << 15;
    let exp_f64: i32 = ((bits >> 52) & 0x7ff) as i32 - 1023; // unbiased double exponent
    let exp_80: u16 = (exp_f64 + 16383) as u16; // rebias for 80-bit extended
    let mantissa_f64 = bits & 0x000f_ffff_ffff_ffff; // 52-bit fraction
                                                     // 80-bit explicit mantissa: leading 1 + 52-bit fraction left-shifted to 63 bits
    let mantissa_80: u64 = (1u64 << 63) | (mantissa_f64 << 11);

    let exp_word = sign | exp_80;
    bytes[0..2].copy_from_slice(&exp_word.to_be_bytes());
    bytes[2..10].copy_from_slice(&mantissa_80.to_be_bytes());
    bytes
}

/// MP3 export via LAME encoder.
///
/// LGPL compliance: uses local lame-sys crate which links libmp3lame DYNAMICALLY.
/// See: lineos/plan/phase-13/LAME-LGPL-NOTICE.md
/// System: libmp3lame.so.0 at /lib/x86_64-linux-gnu/libmp3lame.so.0
///
/// Quality preset 2: mastering grade (0=best, 9=worst).
/// No DSP re-run — reads stored f32 LE PCM bytes from the Golden Blob.
pub struct AcxExportOutcome {
    pub report: sp314_dsp::analysis::acx_check::AcxCheckReport,
    pub head_quiet_secs: f32,
    pub tail_quiet_secs: f32,
}

/// Duration in seconds of contiguous sub-threshold signal at each end.
/// Windows of 100 ms RMS; a window counts as "quiet" below -50 dBFS.
/// Measures PRESENCE OF QUIET (room tone qualifies), not dead silence
/// — gated/denoised masters still measure correctly. Scan from each
/// end until the first non-quiet window; partial trailing window
/// (< 100 ms) is ignored.
pub fn edge_quiet_secs(mono: &[f32], sample_rate: u32) -> (f32, f32) {
    let window_len = (sample_rate / 10) as usize; // 100 ms
    if window_len == 0 || mono.is_empty() {
        return (0.0, 0.0);
    }
    let mut head_windows = 0;
    for w in mono.chunks(window_len) {
        if w.len() < window_len {
            break;
        }
        let sum_sq: f64 = w.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / window_len as f64).sqrt() as f32;
        let dbfs = if rms > 1e-10 {
            20.0 * rms.log10()
        } else {
            -200.0
        };
        if dbfs < -50.0 {
            head_windows += 1;
        } else {
            break;
        }
    }

    let num_full = mono.len() / window_len;
    if head_windows == num_full {
        return ((head_windows as f32) * 0.1, 0.0);
    }

    let mut tail_windows = 0;
    for i in (0..num_full).rev() {
        let start = i * window_len;
        let w = &mono[start..start + window_len];
        let sum_sq: f64 = w.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / window_len as f64).sqrt() as f32;
        let dbfs = if rms > 1e-10 {
            20.0 * rms.log10()
        } else {
            -200.0
        };
        if dbfs < -50.0 {
            tail_windows += 1;
        } else {
            break;
        }
    }

    ((head_windows as f32) * 0.1, (tail_windows as f32) * 0.1)
}

pub fn export_mp3_acx(blob: &StoredBlobV2, path: &Path) -> Result<AcxExportOutcome, String> {
    use lame_sys::{
        lame_encode_buffer_ieee_float, lame_encode_flush_nogap, lame_init, lame_init_params,
        lame_set_VBR, lame_set_brate, lame_set_in_samplerate, lame_set_mode, lame_set_num_channels,
        lame_set_out_samplerate, lame_set_quality, vbr_mode, MPEG_mode,
    };
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
    use sp314_dsp::metering::true_peak_meter::TruePeakMeter;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let audio_bytes = std::fs::read(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to read audio from disk: {e}"))?;
    if audio_bytes.is_empty() {
        return Err("No audio bytes in file — cannot write MP3".into());
    }

    // 1. Read blob PCM, de-interleave to planar 2ch.
    let pcm = pcm_bytes_to_f32(&audio_bytes);
    let channels = blob.core.channels.max(1) as usize;
    if channels != 2 {
        return Err(format!("Expected 2 channels, found {channels}"));
    }

    let half = pcm.len() / 2;
    let mut planar = [Vec::with_capacity(half), Vec::with_capacity(half)];
    for chunk in pcm.chunks_exact(2) {
        planar[0].push(chunk[0]);
        planar[1].push(chunk[1]);
    }

    // 2. Resample 48000 -> 44100
    let original_sr = 48000;
    let target_sr = 44100;
    let ratio = target_sr as f64 / original_sr as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256, // SINC_LEN
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256, // SINC_OVERSAMPLE
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(
        ratio, 2.0, params, 4096, // RESAMPLE_CHUNK_FRAMES
        2,
    )
    .map_err(|e| format!("Resampler init: {e}"))?;

    let total_frames = planar[0].len();
    let mut pos = 0_usize;
    let mut resampled_planar: Vec<Vec<f32>> = vec![Vec::new(); 2];

    while pos < total_frames {
        let end = (pos + 4096).min(total_frames);
        let chunk_len = end - pos;
        let wave_in: Vec<Vec<f32>> = if chunk_len == 4096 {
            planar.iter().map(|ch| ch[pos..end].to_vec()).collect()
        } else {
            planar
                .iter()
                .map(|ch| {
                    let mut v = ch[pos..end].to_vec();
                    v.resize(4096, 0.0);
                    v
                })
                .collect()
        };

        let mut wave_out = resampler
            .process(&wave_in, None)
            .map_err(|e| format!("Resample chunk failed: {e}"))?;

        let valid_out_frames = (chunk_len as f64 * ratio).round() as usize;
        for c in 0..2 {
            wave_out[c].truncate(valid_out_frames);
            resampled_planar[c].extend_from_slice(&wave_out[c]);
        }
        pos += chunk_len;
    }

    // 3. Downmix to mono (l+r)*0.5
    // Note: Stereo delivery is a future option, mono is the norm for ACX.
    let mut mono: Vec<f32> = resampled_planar[0]
        .iter()
        .zip(resampled_planar[1].iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    // 4a. RMS window correction.
    //
    // Το ACX ορίζει ΠΑΡΑΘΥΡΟ, όχι στόχο: [-23, -18] dBFS
    // RMS. Αν το υλικό είναι ΗΔΗ μέσα, ΔΕΝ αγγίζεται.
    // Το προϊόν επιβάλλει ΣΥΜΜΟΡΦΩΣΗ, όχι στάθμη.
    //
    // ΕΛΑΧΙΣΤΗ ΕΠΕΜΒΑΣΗ: στόχος το ΚΟΝΤΙΝΟΤΕΡΟ σημείο
    // μέσα στο παράθυρο, ΟΧΙ το κέντρο. Ένα αρχείο στα
    // -22.8 είναι συμμορφούμενο· το να το μετακινήσεις
    // στο -20.5 είναι αυθαιρεσία — και το -20.5 είναι
    // ακριβώς το proxy που αυτό το βήμα καταργεί.
    //
    // Μετράμε με τον ΙΔΙΟ analyzer που κρίνει παρακάτω.
    // Μία μέτρηση, ένας ορισμός. northstar, δόγμα Δ.
    //
    // ΑΝΟΙΧΤΟ — BATCH COHESION: αυτή η διόρθωση κοιτάει
    // ΕΝΑ αρχείο. Σε audiobook με 20 κεφάλαια, όλα
    // μπορεί να είναι μέσα στο [-23,-18] και να
    // ακούγονται ανομοιόμορφα — ο ακροατής κρίνει τη
    // ΣΕΙΡΑ, το ACX ελέγχει το ΑΡΧΕΙΟ.
    // Το album path λύνει ΗΔΗ το ίδιο πρόβλημα σε LUFS
    // με cohesion pre-pass και target_lufs_override.
    // Απαιτεί ΔΥΟ περάσματα (μέτρα όλα → κοινός στόχος →
    // διόρθωσε όλα)· το run_deliver_core σήμερα είναι
    // σειριακό, ένα πέρασμα.
    // ΣΒΗΝΕΙ μαζί με το run_batch(). northstar §Β.
    let mut acx_pre = AcxCheckAnalyzer::new(target_sr as u32);
    for chunk in mono.chunks(4096) {
        acx_pre.feed_chunk(chunk);
    }
    let report_first_pass = acx_pre.finish();
    let rms_before = report_first_pass.rms_db;

    // PLACEHOLDER: αντιγράφηκε χωρίς πηγή — ο αριθμός είναι σχεδόν
    // βέβαια σωστός (το `presets.rs` δηλώνει «Verified against ACX's
    // published submission requirements, 2026-07-29»), αλλά **κανένα
    // URL δεν καταγράφηκε ποτέ** και δεν εφευρίσκεται εδώ. Το ίδιο
    // ζεύγος ζει ΚΑΙ στο `acx_check.rs:31-32` ΚΑΙ στο `presets.rs`
    // (ACX DeliverySpec) — τρίτη γραφή, δική της απόφαση.
    // TRIGGER: `DeliveryProfileRef.url` (blob_store.rs) γεμισμένο από
    // ΑΝΘΡΩΠΟ που το επαλήθευσε — το ίδιο του το doc λέει «ποτέ ο
    // κώδικας». Τότε γίνεται SOURCE: … RETRIEVED: ….
    const ACX_RMS_MIN: f32 = -23.0;
    // PLACEHOLDER: αντιγράφηκε χωρίς πηγή — ίδια ιστορία με το MIN από
    // πάνω· η σφραγίδα επαναλαμβάνεται επειδή ο φρουρός απαιτεί ΜΙΑ ΑΝΑ
    // ΚΑΤΩΦΛΙ (ομαδική κάλυψη δοκιμάστηκε και παρήγαγε ψευδή σφραγίδα).
    // TRIGGER: `DeliveryProfileRef.url` γεμισμένο από ΑΝΘΡΩΠΟ.
    const ACX_RMS_MAX: f32 = -18.0;

    // Στοχεύουμε λίγο ΜΕΣΑ από το όριο, όχι πάνω του.
    // ΟΧΙ για δική μας μέτρηση — ο analyzer τρέχει ΠΡΙΝ
    // το LAME και δεν βλέπει ποτέ την επίδρασή του.
    // Το περιθώριο προστατεύει την ΕΠΟΜΕΝΗ μέτρηση: ο
    // narrator θα ξαναελέγξει το τελικό mp3 με το
    // Audacity ACX Check, και εκεί το lossy encoding θα
    // έχει μετακινήσει τα δείγματα. Ένα αρχείο ακριβώς
    // στα -23.00 μπορεί να διαβαστεί -23.02 και να κοπεί.
    const RMS_MARGIN_DB: f32 = 0.5;

    let rms_correction_db = if rms_before < ACX_RMS_MIN {
        ACX_RMS_MIN + RMS_MARGIN_DB - rms_before
    } else if rms_before > ACX_RMS_MAX {
        ACX_RMS_MAX - RMS_MARGIN_DB - rms_before
    } else {
        0.0
    };

    if rms_correction_db != 0.0 {
        tracing::info!(
            rms_before,
            rms_correction_db,
            "ACX RMS window correction"
        );
        let gain = libm::powf(10.0, rms_correction_db / 20.0);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }

    // 4b. True peak ceiling. ΜΕΤΑ το RMS, ΟΧΙ πριν.
    //
    // ΠΡΟΣΟΧΗ ΣΤΑ ΔΥΟ ΔΙΑΦΟΡΕΤΙΚΑ PEAK: εδώ μετράει ο
    // TruePeakMeter (oversampled, inter-sample peaks).
    // Ο AcxCheckAnalyzer αναφέρει SAMPLE peak, τυπικά
    // 0.5-1.5 dB χαμηλότερο. Το report και αυτός ο
    // έλεγχος ΔΕΝ συγκρίνουν το ίδιο νούμερο.
    // Ο έλεγχος εδώ είναι ο ΑΥΣΤΗΡΟΤΕΡΟΣ, σκόπιμα.
    //
    // ΑΝ ΤΟ ACX ΕΙΝΑΙ ΑΝΕΦΙΚΤΟ: υλικό με crest factor
    // τέτοιο ώστε RMS -23 να σημαίνει true peak > -3 ΔΕΝ
    // μπορεί να συμμορφωθεί. Το RMS correction ανεβάζει,
    // το peak trim κατεβάζει, και καταλήγεις εκεί που
    // ξεκίνησες.
    // ΔΕΝ κάνουμε δεύτερο γύρο. Ο analyzer του βήματος 5
    // θα το πει με τα πραγματικά νούμερα και το
    // passes_acx() θα είναι false — ΠΡΑΓΜΑΤΙΚΗ
    // πληροφορία για τον narrator (το υλικό δεν έχει
    // αρκετό headroom και θέλει compression ή
    // επανηχογράφηση), όχι σιωπηλός συμβιβασμός.
    let mut tp_trim_db = 0.0_f32;

    let mut tp_meter = TruePeakMeter::new();
    for chunk in mono.chunks(4096) {
        tp_meter.process_chunk(chunk, chunk);
    }
    let tp_db = tp_meter.finish();

    if tp_db > -3.0 {
        // static gain trim to bring it to -3.05
        let diff_db = -3.05 - tp_db;
        tp_trim_db = diff_db;
        tracing::info!("Applying ACX True Peak trim of {} dB", diff_db);
        let gain = libm::powf(10.0, diff_db / 20.0);
        for s in mono.iter_mut() {
            *s *= gain;
        }
    }

    // 5. AcxCheckAnalyzer
    // Αν ΤΙΠΟΤΑ δεν άγγιξε το σήμα, το πρώτο πέρασμα
    // ισχύει ακόμα — ένα ΗΔΗ συμμορφούμενο αρχείο δεν
    // πληρώνει δεύτερη ανάλυση, και το ότι δεν αγγίχτηκε
    // είναι ΟΡΑΤΟ στον κώδικα.
    //
    // Η σύγκριση με 0.0 είναι ασφαλής εδώ: και οι δύο
    // τιμές προκύπτουν από ΡΗΤΗ ανάθεση (= 0.0 ή έναν
    // υπολογισμό), όχι από συσσώρευση — δεν υπάρχει
    // float drift να συγκρίνουμε.
    let report = if rms_correction_db == 0.0 && tp_trim_db == 0.0 {
        report_first_pass
    } else {
        let mut acx = AcxCheckAnalyzer::new(target_sr as u32);
        for chunk in mono.chunks(4096) {
            acx.feed_chunk(chunk);
        }
        acx.finish()
    };

    // 6. LAME encode
    let gfp = unsafe { lame_init() };
    if gfp.is_null() {
        return Err("MP3: lame_init() returned NULL".into());
    }
    struct LameGuard(lame_sys::lame_t);
    impl Drop for LameGuard {
        fn drop(&mut self) {
            unsafe {
                lame_sys::lame_close(self.0);
            }
        }
    }
    let _guard = LameGuard(gfp);

    unsafe {
        lame_set_num_channels(gfp, 1);
        lame_set_in_samplerate(gfp, target_sr as std::os::raw::c_int);
        lame_set_out_samplerate(gfp, target_sr as std::os::raw::c_int); // EXPLICIT - never let LAME pick
        lame_set_mode(gfp, MPEG_mode::MONO);
        lame_set_VBR(gfp, vbr_mode::vbr_off);
        // SOURCE: https://help.acx.com/s/article/what-are-the-acx-audio-submission-requirements
        // RETRIEVED: 2026-08-25 (σελίδα: Apr 15, 2026)
        // «Each file must be a 192 kbps or higher CBR, 44.1kHz MP3. You may
        //  upload 256kbps or 320kbps»
        // ⇒ Το 192 είναι το ΚΑΤΩΤΑΤΟ επιτρεπτό, όχι η απαίτηση. Καθόμαστε
        //   ΑΚΡΙΒΩΣ στο πάτωμα: συμμορφούμενο, χωρίς περιθώριο προς τα κάτω.
        //   ΤΟ ΟΡΙΟ ΕΙΝΑΙ >=192 — αν κάποιος το κατεβάσει, σπάει· αν το
        //   ανεβάσει σε 256/320, εξακολουθεί να συμμορφώνεται.
        // ⚠ Ο threshold-lint ΔΕΝ ΤΟ ΒΛΕΠΕΙ: δεν είναι const, ούτε σύγκριση,
        //   ούτε πεδίο struct — είναι όρισμα κλήσης. Τρίτο δηλωμένο κενό
        //   του φρουρού, μαζί με το g_max_db (JSON) και το f_cutoff.
        lame_set_brate(gfp, 192);
        lame_set_quality(gfp, 2);
        if lame_init_params(gfp) < 0 {
            return Err("lame_init_params failed".into());
        }
    }

    let mut out_file =
        std::fs::File::create(path).map_err(|e| format!("Could not create output file: {e}"))?;

    use std::io::Write;
    let chunk_size = 8192;
    let mut mp3buf = vec![0u8; chunk_size + chunk_size / 4 + 7200];

    for chunk in mono.chunks(chunk_size) {
        let written = unsafe {
            lame_encode_buffer_ieee_float(
                gfp,
                chunk.as_ptr(),
                // Same pointer for both: the installed header (lame.h:756-758)
                // documents nothing about NULL for mono — "as
                // lame_encode_buffer, but for floats" — so we do not hand a C
                // library a NULL it never promised to tolerate. With
                // num_channels=1 + MONO the right buffer is unused; pointing
                // it at the same valid data costs nothing and is safe under
                // every reading of the API.
                chunk.as_ptr(),
                chunk.len() as std::os::raw::c_int,
                mp3buf.as_mut_ptr(),
                mp3buf.len() as std::os::raw::c_int,
            )
        };
        if written < 0 {
            return Err("lame_encode_buffer_ieee_float failed".into());
        }
        if written > 0 {
            out_file
                .write_all(&mp3buf[..written as usize])
                .map_err(|e| e.to_string())?;
        }
    }

    let written = unsafe {
        lame_encode_flush_nogap(
            gfp,
            mp3buf.as_mut_ptr(),
            mp3buf.len() as std::os::raw::c_int,
        )
    };
    if written > 0 {
        out_file
            .write_all(&mp3buf[..written as usize])
            .map_err(|e| e.to_string())?;
    }
    out_file.flush().map_err(|e| e.to_string())?;

    // 7. symphonia decode-back
    {
        let file =
            std::fs::File::open(path).map_err(|e| format!("Could not open encoded file: {e}"))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &MetadataOptions::default())
            .map_err(|e| format!("Symphonia probe error: {e}"))?;

        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
            .ok_or_else(|| "No audio track found".to_string())?;

        let track_id = track.id;
        // X1: το sample rate είναι η ΚΛΙΜΑΚΑ ΤΟΥ ΧΡΟΝΟΥ, όχι
        // μεταδεδομένο. Λάθος τιμή = pitch shift + κάθε
        // μέτρηση μετατοπισμένη (44.1k ως 48k = +1.47
        // ημιτόνια, 8.8% σε LUFS φίλτρα, mel filterbank,
        // frame 480). Τέσσερα σημεία μάντευαν με τρεις
        // διαφορετικές τιμές. ΑΡΝΗΣΗ αντί για μαντεψιά.
        let dec_sr = track.codec_params.sample_rate.ok_or_else(|| "Missing sample rate".to_string())?;
        let dec_ch = track
            .codec_params
            .channels
            .map(|c| c.count() as u16)
            .unwrap_or(0);

        if dec_sr != target_sr as u32 {
            return Err(format!("Expected {} Hz, found {}", target_sr, dec_sr));
        }
        if dec_ch != 1 {
            return Err(format!("Expected 1 channel, found {}", dec_ch));
        }

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| format!("Codec not supported: {e}"))?;

        let mut decoded_frames = 0;
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    break
                }
                Err(_) => break, // simplify
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(audio_buf) => decoded_frames += audio_buf.capacity(),
                _ => continue,
            }
        }

        let expected_frames = mono.len();
        let diff_frames = (decoded_frames as i64 - expected_frames as i64).abs();
        let diff_sec = diff_frames as f64 / target_sr as f64;

        // MP3 codec padding makes exact-length impossible
        if diff_sec > 0.1 {
            return Err(format!(
                "Duration mismatch: expected {} frames, got {} (diff {}s)",
                expected_frames, decoded_frames, diff_sec
            ));
        }
    }

    let (head_quiet_secs, tail_quiet_secs) = edge_quiet_secs(&mono, target_sr as u32);

    // 8. Return report
    Ok(AcxExportOutcome {
        report,
        head_quiet_secs,
        tail_quiet_secs,
    })
}

fn export_mp3(blob: &StoredBlobV2, path: &Path) -> Result<(), String> {
    use lame_sys::{
        lame_close, lame_encode_buffer_interleaved_ieee_float, lame_encode_flush_nogap, lame_init,
        lame_init_params, lame_set_in_samplerate, lame_set_num_channels, lame_set_quality,
    };

    let audio_bytes = std::fs::read(blob.core.audio_path.path())
        .map_err(|e| format!("Failed to read audio from disk: {e}"))?;
    if audio_bytes.is_empty() {
        return Err("No audio bytes in file — cannot write MP3".into());
    }

    let pcm = pcm_bytes_to_f32(&audio_bytes);
    // Samples per channel (LAME interleaved API takes frames, not total samples)
    let num_samples_per_channel = (pcm.len() / blob.core.channels.max(1) as usize) as i32;

    // ── Initialise LAME context ──────────────────────────────────────────
    // SAFETY: lame_sys wraps a C library. All pointers are valid for the scope.
    // The gfp context is created, configured, used for encoding, then closed.
    let gfp = unsafe { lame_init() };
    if gfp.is_null() {
        return Err("MP3: lame_init() returned NULL — LAME init failed".into());
    }

    // Wrap in a guard so lame_close is always called even on error
    struct LameGuard(lame_sys::lame_t);
    impl Drop for LameGuard {
        fn drop(&mut self) {
            unsafe {
                lame_close(self.0);
            }
        }
    }
    let _guard = LameGuard(gfp);

    unsafe {
        let r = lame_set_num_channels(gfp, blob.core.channels as i32);
        if r < 0 {
            return Err(format!("MP3: lame_set_num_channels failed: {r}"));
        }

        let r = lame_set_in_samplerate(gfp, blob.core.sample_rate as i32);
        if r < 0 {
            return Err(format!("MP3: lame_set_in_samplerate failed: {r}"));
        }

        // Quality 2: near-lossless for mastering (0=highest quality, 9=lowest)
        let r = lame_set_quality(gfp, 2);
        if r < 0 {
            return Err(format!("MP3: lame_set_quality failed: {r}"));
        }

        let r = lame_init_params(gfp);
        if r < 0 {
            return Err(format!("MP3: lame_init_params failed: {r}"));
        }
    }

    // ── Encode interleaved PCM ───────────────────────────────────────────
    // Output buffer: LAME guarantees at most 1.25 * num_samples + 7200 bytes.
    let mp3_buf_size = (1.25 * pcm.len() as f64 + 7200.0) as usize;
    let mut mp3_buf: Vec<u8> = vec![0u8; mp3_buf_size];
    let mut mp3_out: Vec<u8> = Vec::with_capacity(mp3_buf_size);

    let encoded_len = unsafe {
        lame_encode_buffer_interleaved_ieee_float(
            gfp,
            pcm.as_ptr(),
            num_samples_per_channel,
            mp3_buf.as_mut_ptr(),
            mp3_buf_size as i32,
        )
    };
    if encoded_len < 0 {
        return Err(format!(
            "MP3: lame_encode_buffer_interleaved_ieee_float failed: {encoded_len}"
        ));
    }
    mp3_out.extend_from_slice(&mp3_buf[..encoded_len as usize]);

    // ── Flush remaining frames (no decoder delay padding) ───────────────
    let flushed_len =
        unsafe { lame_encode_flush_nogap(gfp, mp3_buf.as_mut_ptr(), mp3_buf_size as i32) };
    if flushed_len < 0 {
        return Err(format!(
            "MP3: lame_encode_flush_nogap failed: {flushed_len}"
        ));
    }
    mp3_out.extend_from_slice(&mp3_buf[..flushed_len as usize]);

    tracing::info!(
        path  = %path.display(),
        bytes = mp3_out.len(),
        "m0d: MP3 export complete (LAME dynamic — LGPL compliant)"
    );

    std::fs::write(path, &mp3_out).map_err(|e| format!("MP3: write failed: {e}"))
}

/// Locate the `data` chunk payload of a RIFF/WAVE buffer.
///
/// Walks the chunk list from offset 12 instead of assuming a fixed header
/// length: a WAVE header is NOT fixed-size (`fact`, `LIST`, and other
/// chunks may precede `data`, and our own writer's length has already
/// varied). Chunk bodies are word-aligned, so an odd `size` carries one
/// pad byte that is not part of the payload.
///
/// Returns `None` when the buffer is not RIFF/WAVE at all. Returns an
/// empty slice when it IS RIFF/WAVE but carries no `data` chunk — a WAVE
/// with no audio yields no samples, never its own header bytes.
fn riff_wave_data(bytes: &[u8]) -> Option<&[u8]> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    while pos.saturating_add(8) <= bytes.len() {
        let size = u32::from_le_bytes([
            bytes[pos + 4],
            bytes[pos + 5],
            bytes[pos + 6],
            bytes[pos + 7],
        ]) as usize;
        let body = pos + 8;
        if &bytes[pos..pos + 4] == b"data" {
            // Clamp: a truncated file may declare more than it carries.
            let end = body.saturating_add(size).min(bytes.len());
            return Some(&bytes[body..end]);
        }
        pos = body.saturating_add(size).saturating_add(size & 1);
    }
    Some(&[])
}

/// Decode PCM bytes to samples: f32 **little-endian**, 4 bytes per sample.
/// Accepts either a headerless raw dump or a RIFF/WAVE file, and reads only
/// the audio payload in both cases.
///
/// F-087 (2026-08-25): πέντε καταναλωτές
/// διάβαζαν 68 bytes RIFF header ως 17 float
/// samples, ένα ~1.5×10³³. Αποτελέσματα: σιωπή
/// (mp3_acx, −560dB auto-correction) · σφάλμα +
/// 17MB leftover στα +603dB (wav) · κλικ 0.0 dBFS
/// (flac, ΕΥΚΡΙΝΕΣ στην ακρόαση 25/08) · καθαρό
/// (mp3) · αμέτρητο (aiff). Η ασυμφωνία γεννήθηκε
/// στο bda7fe8 (17/07). Η ομιλία ΔΕΝ επηρεαζόταν:
/// Α/Β έναντι reference ταυτόσημο στο 6ο δεκαδικό.
///
/// Η διόρθωση ζει ΕΔΩ και όχι στους καταναλωτές: ένα σφάλμα ανάγνωσης,
/// πέντε αποτελέσματα — πέντε μπαλώματα θα άφηναν το έκτο.
/// Ο εναλλακτικός δρόμος («πέρασε το headerless `mastered_raw_path` ως
/// `audio_path`») ΑΠΟΡΡΙΦΘΗΚΕ με μέτρηση: το αρχείο σβήνεται από το
/// επόμενο master (playback worker → `old.release()` → `_guard` drop)
/// και από το startup sweep του spool.
/// Τεκμήρια: `.reports/2026-08-25-raw-path-lifetime.md`,
/// `.reports/2026-08-25-pcm-read-fix.md`.
///
/// ⚠ ΣΥΜΒΟΛΑΙΟ, ΔΗΛΩΜΕΝΟ ΩΣ ΑΝΟΙΧΤΟ: ο γραφέας της ζωντανής διαδρομής
/// (`dsp/wav_to_raw.rs`, `wav_to_raw_measured`) γράφει `to_ne_bytes`
/// (native-endian) ενώ εδώ διαβάζουμε little-endian. Ταυτίζονται σε
/// x86-64/ARM64, άρα σήμερα δεν υπάρχει σφάλμα — είναι σιωπηλή παγίδα,
/// όχι τρέχουσα βλάβη. Η ευθυγράμμιση αγγίζει ΑΛΛΟ αρχείο και μένει
/// έξω από αυτή την αλλαγή, ρητά.
fn pcm_bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    let payload = riff_wave_data(bytes).unwrap_or(bytes);
    payload
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

// ── P10-003: Sidecar JSON ─────────────────────────────────────────────────────

/// Sidecar metadata written alongside the audio file as `<name>.stillair.json`.
/// Contains Golden Blob metrics — no audio bytes, no re-measurement.
#[derive(Serialize)]
pub struct ExportSidecar<'a> {
    pub blob_id: &'a str,
    pub preset_id: &'a str,
    pub export_format: &'a str,
    pub exported_at: String,
    pub loudness: &'a StoredLoudness,
    pub quality: &'a StoredQuality,
    pub compliance: ComplianceSummary,
    /// Ποιον προορισμό ζήτησε ο χρήστης — η ΤΙΜΗ του
    /// `DeliverySpec.platform`, όχι όνομα πεδίου (§5.1α: το εμπορικό όνομα
    /// ζει σε τιμή, ποτέ σε δομή). `None` όταν το preset δεν βρίσκεται στο
    /// μητρώο.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_target: Option<&'a str>,
    /// Η ετυμηγορία ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ, ίδιο σχήμα §5.3 με το `/deliver`.
    ///
    /// ⚠ ΤΟ `compliance` ΑΠΟ ΠΑΝΩ ΠΕΡΙΓΡΑΦΕΙ ΤΟΝ MASTER, ΟΧΙ ΑΥΤΟ ΤΟ
    /// ΑΡΧΕΙΟ (F-090) — και απαριθμεί πέντε προορισμούς μουσικής χωρίς να
    /// περιλαμβάνει αυτόν που διάλεξε ο χρήστης. ΔΕΝ διορθώνεται εδώ·
    /// αυτό το πεδίο ΠΡΟΣΘΕΤΕΙ την ετυμηγορία που έλειπε, δεν αντικαθιστά
    /// ό,τι υπάρχει.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_checks: Option<&'a [crate::blob_store::DeliveryCheck]>,
}

#[derive(Serialize)]
pub struct ComplianceSummary {
    pub spotify: bool,
    pub youtube: bool,
    pub apple: bool,
    pub tidal: bool,
    pub broadcast: bool,
}

/// Write sidecar JSON alongside audio file.
/// Path: audio_path with extension replaced by "stillair.json".
pub fn write_sidecar(
    blob: &StoredBlobV2,
    format: &str,
    audio_path: &Path,
    delivery_checks: Option<&[crate::blob_store::DeliveryCheck]>,
) -> Result<(), String> {
    // ΑΡΝΗΣΗ, ΟΧΙ null: το ComplianceSummary έχει πέντε
    // bool και δεν υπάρχει null για bool. Το false θα
    // δήλωνε "ελέγχθηκε και απέτυχε" αντί για "δεν
    // ελέγχθηκε" — ψευδής δήλωση μη συμμόρφωσης σε
    // αρχείο που διαβάζεται αυτόματα.
    // Το sidecar είναι απόδειξη. Χωρίς μετρήσεις δεν
    // υπάρχει τι να αποδειχθεί.
    // ΣΗΜΕΡΑ ΔΕΝ ΠΥΡΟΔΟΤΕΙΤΑΙ: το /export παίρνει blob
    // από το store, πάντα Certified. Είναι φράγμα για
    // το μέλλον.
    if let Some(reason) = blob.uncertified_reason() {
        return Err(format!(
            "cannot write sidecar for uncertified blob: {reason:?}"
        ));
    }

    let sidecar_path = audio_path.with_extension("stillair.json");

    let l = blob.loudness()
        .expect("sidecar: guard above guarantees certified");
    let q = blob.quality()
        .expect("sidecar: guard above guarantees certified");

    let sidecar = ExportSidecar {
        blob_id: &blob.core.id,
        preset_id: &blob.core.preset_id,
        export_format: format,
        exported_at: Utc::now().to_rfc3339(),
        loudness: l,
        quality: q,
        compliance: ComplianceSummary {
            spotify: l.spotify_compliant,
            youtube: l.youtube_compliant,
            apple: l.apple_music_compliant,
            tidal: l.tidal_compliant,
            broadcast: l.broadcast_compliant,
        },
        delivery_target: lineos_types::presets::lookup(&blob.core.preset_id)
            .map(|p| p.delivery.platform),
        delivery_checks,
    };

    let json = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| format!("Sidecar serialize failed: {e}"))?;

    std::fs::write(&sidecar_path, json).map_err(|e| format!("Sidecar write failed: {e}"))
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

    #[test]
    fn test_f64_to_80bit_extended_zero() {
        let bytes = f64_to_80bit_extended(0.0);
        assert_eq!(bytes, [0u8; 10]);
    }

    #[test]
    fn test_f64_to_80bit_extended_48000() {
        // 48000 Hz = 0xBB80 = 1011 1011 1000 0000
        // exp: biased double = 1023 + 15 = 1038 (0x40E) -> 80-bit: 16383 + 15 = 16398 (0x400E)
        let bytes = f64_to_80bit_extended(48000.0);
        // exponent word: 0x400E (sign=0, exp_80=0x400E)
        assert_eq!(bytes[0], 0x40);
        assert_eq!(bytes[1], 0x0E);
        // Mantissa must have leading bit set (explicit integer bit)
        assert!(bytes[2] & 0x80 != 0);
    }

    #[test]
    fn test_pcm_bytes_roundtrip() {
        let samples = [0.5f32, -0.5f32, 1.0f32, 0.0f32];
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let decoded = pcm_bytes_to_f32(&bytes);
        assert_eq!(decoded.len(), samples.len());
        for (a, b) in decoded.iter().zip(samples.iter()) {
            assert!((a - b).abs() < 1e-7);
        }
    }

    #[test]
    fn test_pcm_bytes_empty() {
        let decoded = pcm_bytes_to_f32(&[]);
        assert!(decoded.is_empty());
    }

    // ── F-087 read-fix oracles ───────────────────────────────────────────
    // The defect these pin: a RIFF header read as audio. A fixed-length
    // skip (68) passes (1) and (2) and FAILS (3) — that is exactly why (3)
    // exists.

    /// Interleaved stereo sine at −20 dBFS. Known peak, known even count.
    fn f087_fixture_samples() -> Vec<f32> {
        let amp = libm::powf(10.0, -20.0 / 20.0); // −20 dBFS, exact
        let n_frames = 480usize;
        let mut v = Vec::with_capacity(n_frames * 2);
        for i in 0..n_frames {
            // Quarter-cycle grid: sample 120 lands exactly on sin=1 ⇒ the
            // peak is the amplitude itself, not an interpolation artefact.
            let s = amp * libm::sinf(2.0 * std::f32::consts::PI * (i as f32) / 480.0);
            v.push(s);
            v.push(s);
        }
        v
    }

    fn f087_raw_bytes(samples: &[f32]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }

    /// Build a RIFF/WAVE around `data`, optionally inserting extra chunks
    /// before it (the "header is not a fixed length" case).
    fn f087_wrap_riff(data: &[u8], extra_chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(b"WAVE");

        // fmt chunk — 16-byte PCM-float form, enough to be a real WAVE.
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&16u32.to_le_bytes());
        body.extend_from_slice(&3u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
        body.extend_from_slice(&2u16.to_le_bytes()); // channels
        body.extend_from_slice(&48000u32.to_le_bytes());
        body.extend_from_slice(&(48000u32 * 8).to_le_bytes()); // byte rate
        body.extend_from_slice(&8u16.to_le_bytes()); // block align
        body.extend_from_slice(&32u16.to_le_bytes()); // bits

        for (id, payload) in extra_chunks {
            body.extend_from_slice(*id);
            body.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            body.extend_from_slice(payload);
            if payload.len() % 2 == 1 {
                body.push(0); // word alignment pad
            }
        }

        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);

        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn f087_peak_db(samples: &[f32]) -> f32 {
        let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        20.0 * libm::log10f(peak)
    }

    #[test]
    fn f087_riff_wav_decodes_to_exact_content() {
        let expected = f087_fixture_samples();
        let wav = f087_wrap_riff(&f087_raw_bytes(&expected), &[]);
        let decoded = pcm_bytes_to_f32(&wav);

        assert_eq!(
            decoded.len(),
            expected.len(),
            "sample count must match exactly what was written"
        );
        assert_eq!(decoded.len() % 2, 0, "stereo payload must stay even");
        assert!(
            (f087_peak_db(&decoded) - (-20.0)).abs() < 0.001,
            "peak must be −20 dBFS, got {}",
            f087_peak_db(&decoded)
        );
        assert_eq!(decoded, expected, "payload must be bit-identical");
    }

    #[test]
    fn f087_headerless_raw_matches_riff() {
        let expected = f087_fixture_samples();
        let raw = f087_raw_bytes(&expected);
        let wav = f087_wrap_riff(&raw, &[]);

        let from_raw = pcm_bytes_to_f32(&raw);
        let from_wav = pcm_bytes_to_f32(&wav);

        // The offline path still hands us headerless .pcm — it must be
        // untouched by the RIFF branch.
        assert_eq!(from_raw, expected, "headerless raw must decode unchanged");
        assert_eq!(
            from_raw, from_wav,
            "headerless and RIFF forms of the same audio must be identical"
        );
    }

    #[test]
    fn f087_riff_with_extra_chunk_before_data() {
        let expected = f087_fixture_samples();
        // A LIST chunk (odd length, so it also exercises the pad byte)
        // sitting between fmt and data: any fixed-offset skip dies here.
        let extras: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"LIST", b"INFOISFT\x05\x00\x00\x00probe".to_vec()),
            (b"fact", 480u32.to_le_bytes().to_vec()),
        ];
        let raw = f087_raw_bytes(&expected);
        let wav = f087_wrap_riff(&raw, &extras);

        // EXERCISE-PROOF (E1): this fixture's header is NOT 68 bytes, so a
        // fixed-offset skip cannot pass this test — it would land mid-header
        // and misalign every sample. Pinning the number keeps the guard
        // honest if the fixture is ever edited.
        let header_len = wav.len() - raw.len();
        assert_ne!(
            header_len, 68,
            "fixture must not accidentally have a 68-byte header"
        );
        assert_eq!(header_len, 82, "header length pinned by construction");

        let decoded = pcm_bytes_to_f32(&wav);
        assert_eq!(
            decoded, expected,
            "data chunk must be found past extra chunks, not at a fixed offset"
        );
        assert!(
            (f087_peak_db(&decoded) - (-20.0)).abs() < 0.001,
            "peak must still be −20 dBFS"
        );
    }
}
