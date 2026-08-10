//! INV-DET-1: ίδιο input + ίδια πρόθεση + ίδιο
//! πλαίσιο → ίδιο output, byte για byte.
//!
//! Αυτό είναι το θεμέλιο του certificate. Χωρίς
//! αυτό, μια υπογραφή δεν σημαίνει τίποτα — δεν
//! μπορείς να αποδείξεις τι παρήγαγε τι.
//!
//! ΠΛΑΙΣΙΟ σημαίνει: θέση στο album και ό,τι
//! προηγήθηκε. Ο EarFatigue (conductor.rs:346)
//! διαβάζει το LUFS του προηγούμενου track και
//! γράφει στο ArcSwap, που το render διαβάζει
//! (dsp_pipeline.rs:909). Ίδιο αρχείο ως track #1
//! ή #2 δίνει διαφορετικό αποτέλεσμα — ΣΚΟΠΙΜΑ,
//! και καταγράφεται στο certificate ως
//! ear_fatigue_applied.
//!
//! Αυτό το test ελέγχει ΜΟΝΟ single-track render,
//! όπου το πλαίσιο είναι κενό.

use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use xaak::repo::DspState;

fn get_sha256(path: &Path) -> String {
    let output = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("Failed to execute sha256sum");
    let out_str = String::from_utf8_lossy(&output.stdout);
    out_str.split_whitespace().next().unwrap().to_string()
}

#[test]
#[ignore]
fn inv_det_1_render_determinism() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let req = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "Transparent".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("det_test".to_string()),
        track_id: Some("det1".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(true),
    };

    let state_tmp_1 = tempfile::TempDir::new().unwrap();
    let out_dir_1 = tempfile::TempDir::new().unwrap();

    let (blob1, _, _pcm1, _, _, artifacts1) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "det-podcast-1".to_string(),
        state_tmp_1.path().to_str().unwrap(),
        out_dir_1.path().to_str().unwrap(),
    )
    .expect("run_dsp 1 failed");

    let master_path_1 = artifacts1.persisted_master.expect("Master 1 not produced");
    let sha_1 = get_sha256(&master_path_1);
    println!("SHA A: {}", sha_1);

    let state_tmp_2 = tempfile::TempDir::new().unwrap();
    let out_dir_2 = tempfile::TempDir::new().unwrap();

    let (blob2, _, _pcm2, _, _, artifacts2) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "det-podcast-2".to_string(),
        state_tmp_2.path().to_str().unwrap(),
        out_dir_2.path().to_str().unwrap(),
    )
    .expect("run_dsp 2 failed");

    let master_path_2 = artifacts2.persisted_master.expect("Master 2 not produced");
    let sha_2 = get_sha256(&master_path_2);
    println!("SHA B: {}", sha_2);

    assert_eq!(sha_1, sha_2, "Files are not byte-for-byte identical");
    assert_eq!(blob1.loudness.integrated_lufs, blob2.loudness.integrated_lufs, "LUFS not identical");
    assert_eq!(blob1.loudness.true_peak_dbtp, blob2.loudness.true_peak_dbtp, "True Peak not identical");
}
