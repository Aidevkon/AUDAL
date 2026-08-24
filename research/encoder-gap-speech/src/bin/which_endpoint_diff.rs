//! ΑΠΟΔΕΙΞΗ 2026-08-24 — τρέχει ΚΑΙ ΤΑ ΔΥΟ production entry points
//! (/master -> run_dsp, /master/streaming -> execute_streaming_plan)
//! με το ΙΔΙΟ input, sha256 τα masters. Καμία re-implementation —
//! καλεί τις πραγματικές συναρτήσεις που καλούν τα HTTP handlers.
//!
//! usage: which_endpoint_diff <wav_path> <preset_id>

use arc_swap::ArcSwap;
use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::io::Write;
use std::sync::Arc;
use xaak::repo::DspState;

fn sha256_of<P: AsRef<std::path::Path>>(path: P) -> String {
    let out = std::process::Command::new("sha256sum")
        .arg(path.as_ref())
        .output()
        .expect("sha256sum");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: which_endpoint_diff <wav_path> <preset_id>");
    let wav_path = args[0].clone();
    let preset_id = args[1].clone();

    eprintln!("=== ΔΙΑΔΡΟΜΗ Α: /master -> Intent::ExecuteMastering -> Intent::RunDsp -> run_dsp() ===");
    let req = MasterRequest {
        audio_path: wav_path.clone(),
        preset_id: preset_id.clone(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("which-endpoint".to_string()),
        track_id: Some("which-endpoint-a".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false),
        use_nmfd: Some(false),
    };
    let state_tmp_a = tempfile::TempDir::new().unwrap();
    let out_tmp_a = tempfile::TempDir::new().unwrap();
    let t0 = std::time::Instant::now();
    let result_a = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "which-endpoint-a".to_string(),
        state_tmp_a.path().to_str().unwrap(),
        out_tmp_a.path().to_str().unwrap(),
    );
    let elapsed_a = t0.elapsed().as_secs_f64();
    let (verdict_a, sha_a) = match &result_a {
        Ok((_, _, _, _, _, artifacts)) => {
            let sha = artifacts
                .persisted_master
                .as_ref()
                .map(|p| sha256_of(p));
            (
                "OK".to_string(),
                sha.unwrap_or_else(|| "NO persisted_master".to_string()),
            )
        }
        Err(e) => (format!("FAILED: {e}"), "N/A".to_string()),
    };
    eprintln!("  verdict={verdict_a}  sha256={sha_a}  elapsed={elapsed_a:.2}s");
    let _ = std::io::stderr().flush();

    eprintln!();
    eprintln!("=== ΔΙΑΔΡΟΜΗ Β: /master/streaming -> Intent::ExecuteStreaming -> Intent::RunStreaming -> execute_streaming_plan() ===");
    let plan = StreamingPlan {
        audio_path: wav_path.clone(),
        preset_id: preset_id.clone(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "which-endpoint-b".to_string(),
    };
    let t1 = std::time::Instant::now();
    let result_b = execute_streaming_plan(&plan, None);
    let elapsed_b = t1.elapsed().as_secs_f64();
    let (verdict_b, sha_b) = match &result_b {
        Ok((_output, blob)) => {
            let path = m0d::blob_store::mastered_path(&blob.core.id);
            let sha = sha256_of(&path);
            ("OK".to_string(), sha)
        }
        Err(e) => (format!("FAILED: {e:?}"), "N/A".to_string()),
    };
    eprintln!("  verdict={verdict_b}  sha256={sha_b}  elapsed={elapsed_b:.2}s");
    let _ = std::io::stderr().flush();

    eprintln!();
    eprintln!("=== ΣΥΓΚΡΙΣΗ ===");
    eprintln!("Διαδρομή Α (/master)           sha256={sha_a}");
    eprintln!("Διαδρομή Β (/master/streaming) sha256={sha_b}");
    if sha_a != "N/A" && sha_b != "N/A" {
        if sha_a == sha_b {
            eprintln!("ΙΔΙΟ — απροσδόκητο, χρειάζεται επανεξέταση της υπόθεσης.");
        } else {
            eprintln!("ΔΙΑΦΟΡΕΤΙΚΟ — το μονοπάτι μετράει, επιβεβαιωμένο εμπειρικά.");
        }
    } else {
        eprintln!("Τουλάχιστον μία διαδρομή απέτυχε — δες verdicts παραπάνω.");
    }
}
