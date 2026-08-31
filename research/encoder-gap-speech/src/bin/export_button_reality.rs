//! ΜΕΤΡΗΣΗ: τι ακούει ο χρήστης όταν πατάει EXPORT και διαλέγει "mp3";
//!
//! Η αλυσίδα του κουμπιού είναι:
//!   [EXPORT] → invoke("export_audio") → src-tauri commands::export
//!           → POST /export → m0d::handlers::export::export_audio (axum)
//!           → export_blob → ExportFormat::Mp3 => export_mp3
//!
//! `export_blob` ΚΑΙ `export_mp3` είναι ΙΔΙΩΤΙΚΕΣ (`fn`, όχι `pub fn`).
//! Η ΜΟΝΗ pub είσοδος που τις φτάνει είναι ο ίδιος ο axum handler
//! `export_audio`. Άρα ΤΟΝ ΚΑΛΟΥΜΕ ΑΥΤΟΝ, in-process, με πραγματικό
//! AppState — δηλαδή ΑΚΡΙΒΩΣ ό,τι κάνει το HTTP, μείον το transport.
//! Καμία επανυλοποίηση: ούτε του dispatch, ούτε του encoder.
//!
//! AppState::new_for_test → in-memory DB, ώστε το recon να ΜΗΝ αγγίξει
//! το πραγματικό ~/.creator_os/db. Ο blob μπαίνει στο store με το
//! pub πεδίο `blob_store`, όπως θα τον έβρισκε το get_or_rehydrate.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/w-speech/corpus/2277-149896-0000.flac".to_string());
    let input_abs = std::fs::canonicalize(&input).expect("resolve input");
    let outdir = std::path::PathBuf::from(
        std::env::args()
            .nth(2)
            .unwrap_or_else(|| "/tmp/f087-export-button".to_string()),
    );
    std::fs::create_dir_all(&outdir).expect("mkdir outdir");

    // ── 1. Πραγματικό blob από τη ΖΩΝΤΑΝΗ διαδρομή ────────────────────
    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: input_abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "f087-export-button".to_string(),
    };
    let (_out, blob) = tokio::task::spawn_blocking(move || {
        m0d::agents::executor::execute_streaming_plan(&plan, None)
    })
    .await
    .expect("join")
    .expect("execute_streaming_plan failed");

    let blob_id = blob.core.id.clone();
    println!("MEASURED blob_id = {blob_id}");
    println!(
        "MEASURED blob.audio_path = {}",
        blob.core.audio_path.path().display()
    );

    // ── RAW REFERENCE: το σήμα ΑΚΡΙΒΩΣ ΠΡΙΝ το export ────────────────
    // Το blob.audio_path είναι ΓΝΗΣΙΟ WAV (hound) — δηλαδή το
    // masterαρισμένο σήμα, σωστά γραμμένο. ΑΥΤΟ είναι ο σωστός Α/Β
    // εταίρος: ίδιο υλικό, ίδιο mastering, ΧΩΡΙΣ το export.
    // ΠΡΕΠΕΙ να αντιγραφεί ΤΩΡΑ: το ManagedPcm::drop σβήνει το αρχείο
    // όταν πέσει το blob (lineos-types/audio.rs:117-123).
    let raw_ref = outdir.join("RAW_reference_full.wav");
    match std::fs::copy(blob.core.audio_path.path(), &raw_ref) {
        Ok(n) => println!("MEASURED raw_reference copied = {} ({} bytes)", raw_ref.display(), n),
        Err(e) => println!("FAILED raw_reference copy: {e}"),
    }

    // ── 2. Πραγματικό AppState + ο blob στο store ─────────────────────
    let config = Arc::new(m0d::config::M0Config::from_env());
    let audit = Arc::new(
        m0d::audit::AuditLog::open(&config.audit_log_dir).expect("audit open"),
    );
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config).await;
    state.blob_store.insert(blob);

    // ── 3. Ο ΠΡΑΓΜΑΤΙΚΟΣ axum handler, ανά format ─────────────────────
    for fmt in ["mp3", "flac", "wav"] {
        let out = outdir.join(format!("button-{blob_id}.{fmt}"));
        let req = m0d::handlers::export::ExportRequest {
            blob_id: blob_id.clone(),
            format: fmt.to_string(),
            output_path: out.to_string_lossy().into_owned(),
        };
        let Json(resp) =
            m0d::handlers::export::export_audio(State(state.clone()), Json(req)).await;
        println!(
            "MEASURED /export format={fmt:<5} status={} path={:?} msg={:?}",
            resp.status, resp.written_path, resp.message
        );
        if let Ok(md) = std::fs::metadata(&out) {
            println!("          file bytes = {}", md.len());
        }
    }

    println!("OUTDIR {}", outdir.display());
    println!("DONE");
}
