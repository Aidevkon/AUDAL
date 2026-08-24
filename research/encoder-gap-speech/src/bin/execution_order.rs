//! ΜΕΤΡΗΣΗ 2026-08-24 — η ΠΡΑΓΜΑΤΙΚΗ σειρά κόμβων του γράφου, από το
//! ίδιο το γράφημα. READ+RUN, standalone research binary. ΔΕΝ αγγίζει
//! production code — καλεί το ΠΡΑΓΜΑΤΙΚΟ `run_dsp` (dsp_pipeline.rs),
//! όχι re-implementation.
//!
//! ΑΠΑΙΤΕΙ debug build (`cargo build`, ΧΩΡΙΣ --release): το
//! `graph.execution_order` / `[DIAGNOSTIC] topology=` ίχνος ζει πίσω
//! από `#[cfg(debug_assertions)]`. Ο χρόνος καλύπτεται από το
//! ΠΡΟΣΩΡΙΝΟ `[profile.dev] opt-level = 3` του Cargo.toml — τα δύο
//! flags είναι ανεξάρτητα.
//!
//! ΥΛΙΚΟ: 1 δευτερόλεπτο συνθετικού σήματος. Δεν μας νοιάζει ο ήχος —
//! θέλουμε ΤΗ ΛΙΣΤΑ ΤΩΝ ΚΟΜΒΩΝ.
//!
//! ΟΡΑΤΟΤΗΤΑ: κάθε preset τυπώνει timestamp πριν ξεκινήσει, και το
//! αποτέλεσμά του γράφεται στο αρχείο ΑΜΕΣΩΣ (append), ΟΧΙ στο τέλος —
//! ώστε ένα timeout να μη χάνει ό,τι έχει ήδη μετρηθεί.
//!
//!   usage: execution_order <out_dir> <preset_id> [flavour_id]

use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::io::Write;
use std::sync::Arc;
use xaak::repo::DspState;

/// 1s συνθετικό stereo 48k. Δηλωμένο ρητά: ΑΝ το γράφημα δεν χτίζεται
/// με τόσο λίγο υλικό, ΕΙΝΑΙ ΕΥΡΗΜΑ (το γράφημα απαιτεί βαριά
/// pre-analysis) και αναφέρεται ως τέτοιο, δεν παρακάμπτεται.
fn fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(
        "/tmp/claude-1000/-home-aidevcon-Documents-creator-os/6467d876-8020-4252-8284-8582bac82a80/scratchpad/synth_1s.wav",
    )
}

fn stamp() -> String {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

fn main() {
    assert!(
        cfg!(debug_assertions),
        "ΠΡΕΠΕΙ debug build — αλλιώς το execution_order/topology δεν τυπώνεται καθόλου"
    );

    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(
        args.len() >= 2,
        "usage: execution_order <out_dir> <preset_id> [flavour_id]"
    );
    let out_dir = std::path::PathBuf::from(&args[0]);
    let preset = args[1].clone();
    let flavour = args.get(2).cloned();

    std::fs::create_dir_all(&out_dir).expect("out_dir");
    let label = match &flavour {
        Some(f) => format!("{preset}__flavour-{f}"),
        None => preset.clone(),
    };
    let out_file = out_dir.join(format!("{label}.txt"));

    let t0 = std::time::Instant::now();
    eprintln!("[{}] ΞΕΚΙΝΩ preset={preset:?} flavour={flavour:?}", stamp());
    let _ = std::io::stderr().flush();

    let input_path = fixture_path();
    assert!(input_path.exists(), "fixture missing: {}", input_path.display());

    let req = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: preset.clone(),
        flavour_id: flavour.clone(),
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("exec-order".to_string()),
        track_id: Some(format!("eo-{label}")),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(false),
        use_nmfd: Some(false),
    };

    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        format!("eo-{label}"),
        state_tmp.path().to_str().unwrap(),
        out_tmp.path().to_str().unwrap(),
    );

    let elapsed = t0.elapsed();
    let (verdict, master_sha256) = match &result {
        Ok((_, _, _, _, _, artifacts)) => {
            let sha = artifacts.persisted_master.as_ref().map(|p| {
                let out = std::process::Command::new("sha256sum")
                    .arg(p)
                    .output()
                    .expect("sha256sum");
                String::from_utf8_lossy(&out.stdout)
                    .split_whitespace()
                    .next()
                    .unwrap_or("?")
                    .to_string()
            });
            ("OK".to_string(), sha)
        }
        Err(e) => (format!("FAILED: {e}"), None),
    };

    // ΓΡΑΨΕ ΑΜΕΣΩΣ, ΑΝΑ PRESET — όχι στο τέλος.
    let mut f = std::fs::File::create(&out_file).expect("create out_file");
    writeln!(f, "preset_id={preset:?} flavour_id={flavour:?}").ok();
    writeln!(f, "elapsed_secs={:.1}", elapsed.as_secs_f64()).ok();
    writeln!(f, "run_dsp={verdict}").ok();
    writeln!(f, "persisted_master_sha256={master_sha256:?}").ok();
    f.flush().ok();

    eprintln!(
        "[{}] ΤΕΛΟΣ preset={preset:?} elapsed={:.1}s verdict={verdict} -> {}",
        stamp(),
        elapsed.as_secs_f64(),
        out_file.display()
    );
    let _ = std::io::stderr().flush();
}
