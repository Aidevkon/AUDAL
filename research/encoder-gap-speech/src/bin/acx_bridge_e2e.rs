//! ΠΛΗΡΗΣ ΔΙΑΔΡΟΜΗ: πραγματική αφήγηση → master με preset "acx" →
//! EXPORT mp3 → το παραδοτέο ACX.
//!
//! Καλεί ΜΟΝΟ παραγωγικές συναρτήσεις:
//!   m0d::agents::executor::execute_streaming_plan  (ό,τι κάνει το
//!     handlers/master.rs::trigger_streaming στο background)
//!   m0d::handlers::export::export_audio            (ο ΠΡΑΓΜΑΤΙΚΟΣ axum
//!     handler — η μόνη pub είσοδος που φτάνει το ιδιωτικό export_blob
//!     και άρα τη ΝΕΑ δρομολόγηση export_mp3_routed)
//! Καμία επανυλοποίηση. curl στον daemon είναι μπλοκαρισμένο από τα
//! permissions αυτής της συνεδρίας· in-process = ίδιες συναρτήσεις,
//! μείον το transport.
//!
//! ΕΛΕΓΧΟΣ ΤΗΣ ΔΙΑΚΛΑΔΩΣΗΣ (exercise-proof): το ΙΔΙΟ blob εξάγεται
//! δεύτερη φορά με preset_id="spotify". Ίδιο ακριβώς audio, μόνο το
//! preset αλλάζει ⇒ ό,τι διαφορά μετρηθεί ανήκει ΣΤΗ ΔΡΟΜΟΛΟΓΗΣΗ, όχι
//! στον render.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let input = std::env::args().nth(1).expect("usage: acx_bridge_e2e <audio> <outdir>");
    let input_abs = std::fs::canonicalize(&input).expect("resolve input");
    let outdir = std::path::PathBuf::from(
        std::env::args().nth(2).expect("usage: acx_bridge_e2e <audio> <outdir>"),
    );
    std::fs::create_dir_all(&outdir).expect("mkdir outdir");

    // ΑΠΟΔΕΙΞΗ ΟΤΙ ΤΟ OUTCOME ΔΕΝ ΠΕΤΙΕΤΑΙ: χωρίς subscriber το
    // tracing::info! της export_mp3_routed είναι σιωπηλό. Με αυτόν, η
    // γραμμή m0d.export_acx_measured τυπώνεται με τα πραγματικά νούμερα.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "m0d::handlers::export=info".into()),
        )
        .init();

    println!("MEASURED input = {}", input_abs.display());

    // ── 1. Master με preset "acx" ─────────────────────────────────────
    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: input_abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "acx-bridge-e2e".to_string(),
    };
    let (out, blob) = tokio::task::spawn_blocking(move || {
        m0d::agents::executor::execute_streaming_plan(&plan, None)
    })
    .await
    .expect("join")
    .expect("execute_streaming_plan failed");

    println!("MEASURED blob_id            = {}", blob.core.id);
    println!("MEASURED blob.preset_id     = {}", blob.core.preset_id);
    println!("MEASURED blob.sample_rate   = {}", blob.core.sample_rate);
    println!("MEASURED blob.channels      = {}", blob.core.channels);
    println!("MEASURED render output_lufs = {}", out.output_lufs);

    // Το masterαρισμένο WAV, ΠΡΙΝ το export — Α/Β εταίρος.
    // ΠΡΕΠΕΙ να αντιγραφεί ΤΩΡΑ: ManagedPcm::drop σβήνει το αρχείο.
    let raw_ref = outdir.join("BEFORE_export_master.wav");
    match std::fs::copy(blob.core.audio_path.path(), &raw_ref) {
        Ok(n) => println!("MEASURED before-export master copied ({n} bytes) → {}", raw_ref.display()),
        Err(e) => println!("FAILED copy before-export master: {e}"),
    }

    // ── 2. Πραγματικό AppState (in-memory DB — ΔΕΝ αγγίζει το ~/.creator_os/db)
    let config = Arc::new(m0d::config::M0Config::from_env());
    let audit = Arc::new(m0d::audit::AuditLog::open(&config.audit_log_dir).expect("audit open"));
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config).await;

    let acx_id = blob.core.id.clone();

    // ── 3. ΕΛΕΓΧΟΣ: κλώνος του ΙΔΙΟΥ blob με preset "spotify" ─────────
    //     Ίδιο audio_path, ίδια bytes. Μόνο το preset αλλάζει.
    let mut control = blob.clone();
    control.core.id = format!("{acx_id}-control-spotify");
    control.core.preset_id = "spotify".to_string();
    let control_id = control.core.id.clone();

    state.blob_store.insert(blob);
    state.blob_store.insert(control);

    // ── 4. Ο ΠΡΑΓΜΑΤΙΚΟΣ handler, δύο φορές, format=mp3 ───────────────
    for (tag, id) in [("acx", &acx_id), ("control-spotify", &control_id)] {
        let out_path = outdir.join(format!("{tag}.mp3"));
        let req = m0d::handlers::export::ExportRequest {
            blob_id: id.clone(),
            format: "mp3".to_string(),
            output_path: out_path.to_string_lossy().into_owned(),
        };
        let Json(resp) = m0d::handlers::export::export_audio(State(state.clone()), Json(req)).await;
        println!(
            "MEASURED /export preset={tag:<16} status={} path={:?} msg={:?}",
            resp.status, resp.written_path, resp.message
        );
        // Η ΕΤΥΜΗΓΟΡΙΑ, όπως τη γυρίζει ο handler.
        match &resp.delivery_checks {
            None => println!("          delivery_checks = ΚΑΝΕΝΑ (None)"),
            Some(cs) => {
                println!("          delivery_checks ({}):", cs.len());
                for c in cs {
                    println!(
                        "            {:<12} {:>10.4} {} {:>8.2} (margin {:.2}) {} → {}",
                        c.metric, c.measured, c.unit, c.required,
                        c.margin_applied, c.bound, c.verdict.to_uppercase()
                    );
                }
                // ⚠ ΤΟ advisory ΔΕΝ ΕΙΝΑΙ ΑΠΟΤΥΧΙΑ. Η πρώτη μορφή αυτής της
                // σύνοψης έγραφε `c.verdict != "pass"` και ανέφερε
                // «FAIL σε: head_spacing» για αρχείο ΠΛΗΡΩΣ συμμορφούμενο.
                // Είναι ακριβώς το λάθος που η τρίτη κατάσταση προσκαλεί:
                // κάθε συναθροιστής που ρωτάει «είναι pass;» αντί «είναι
                // fail;» μετατρέπει σύσταση σε αποτυχία.
                let failed: Vec<&str> =
                    cs.iter().filter(|c| c.verdict == "fail").map(|c| c.metric.as_str()).collect();
                let advis: Vec<&str> =
                    cs.iter().filter(|c| c.verdict == "advisory").map(|c| c.metric.as_str()).collect();
                if let Some(v) = &resp.delivery_verdict {
                    println!(
                        "          ΕΤΥΜΗΓΟΡΙΑ (fold): complies={} · failed={:?} · advisory={:?} · missing={:?}",
                        v.complies, v.failed, v.advisory, v.missing
                    );
                }
                println!(
                    "          ΣΥΝΟΨΗ ΟΡΓΑΝΟΥ: {}{}",
                    if failed.is_empty() { "ΚΑΜΙΑ ΠΑΡΑΒΙΑΣΗ".to_string() }
                    else { format!("FAIL σε: {}", failed.join(", ")) },
                    if advis.is_empty() { String::new() }
                    else { format!("  ·  ΣΥΣΤΑΣΗ σε: {}", advis.join(", ")) }
                );
            }
        }
        if let Ok(md) = std::fs::metadata(&out_path) {
            println!("          bytes = {}", md.len());
        }
        // Τι sidecar γράφτηκε δίπλα;
        let sc = out_path.with_extension("stillair.json");
        if sc.exists() {
            let txt = std::fs::read_to_string(&sc).unwrap_or_default();
            println!("          sidecar = {} ({} bytes)", sc.display(), txt.len());
            println!("          sidecar keys = {}", sidecar_keys(&txt));
        } else {
            println!("          sidecar = ΚΑΝΕΝΑ");
        }
    }

    println!("OUTDIR {}", outdir.display());
    println!("DONE");
}

/// Τα top-level κλειδιά του sidecar, ώστε να φαίνεται αν υπάρχει
/// υπογραφή χωρίς να τυπωθεί ολόκληρο το JSON.
fn sidecar_keys(txt: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(txt) {
        Ok(serde_json::Value::Object(m)) => {
            let mut k: Vec<&str> = m.keys().map(|s| s.as_str()).collect();
            k.sort();
            k.join(", ")
        }
        _ => "<μη-αναγνώσιμο>".to_string(),
    }
}
