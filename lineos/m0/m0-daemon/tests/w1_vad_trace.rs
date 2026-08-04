//! Ένα όχημα παραγωγής του VAD trace CSV (W1).
//! Αντίγραφο της δομής του ab_render_full με ενεργοποιημένο το vad_observe_enabled.
//!
//! Αυτό το test είναι #[ignore] by default.

use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::fs;
use std::sync::Arc;
use xaak::repo::DspState;

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")
}

#[test]
#[ignore]
fn render_vad_trace_vehicle() {
    let path = fixture_path();
    assert!(path.exists(), "Missing fixture: {}", path.display());

    let req = MasterRequest {
        audio_path: path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("w1_trace".to_string()),
        track_id: Some("bodleasons".to_string()),
        mix_levels: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true), // ΕΝΕΡΓΟΠΟΙΗΜΕΝΟ VAD TRACE
    };

    let state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tempfile::TempDir::new().unwrap(); // Αντί για σταθερό /tmp path

    let blob_id = "w1-trace".to_string();
    // ΠΡΟΣΟΧΗ: το blob_id όρισμα του run_dsp γίνεται shadow από το
    // req.track_id (dsp_pipeline.rs:322-325) — το CSV παίρνει όνομα
    // από το track_id ("bodleasons"), όχι από το blob_id που περνάμε.
    let generated_csv = std::env::temp_dir().join("vad-trace-bodleasons.csv");
    let final_dest = "/tmp/w1_vad_trace.wav";
    let dest_csv = "/tmp/w1_vad_trace_out.csv";

    // Pre-run cleanup ώστε το exists() να αποδεικνύει φρέσκια παραγωγή
    let _ = fs::remove_file(&generated_csv);
    let _ = fs::remove_file(final_dest);
    let _ = fs::remove_file(dest_csv);

    let (_blob, _, pcm, _, _, _) = run_dsp(
        &req,
        std::time::Instant::now(),
        state,
        None,
        None,
        blob_id.clone(),
        state_tmp.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    // Output wav σε δικό του path για να μην πατάει το ab_render_full
    fs::copy(pcm.path(), final_dest).unwrap();
    println!("Written WAV: {}", final_dest);

    // Εντοπισμός και αντιγραφή του παραχθέντος vad-trace CSV
    assert!(generated_csv.exists(), "Generated CSV not found at: {}", generated_csv.display());
    
    fs::copy(&generated_csv, dest_csv).unwrap();
    
    let csv_content = fs::read_to_string(dest_csv).unwrap();
    let lines_count = csv_content.lines().count();
    
    println!("CSV Lines: {}", lines_count);
    println!("Written CSV: {}", dest_csv);
}
