//! ORACLE-CERT: το όργανο μέτρησης του v5 baseline (και,
//! αμετάβλητο, κάθε μελλοντικού v6 σύγκρισης).
//!
//! ΕΝΑ πλήρες render, ΙΔΙΟ fixture/preset με τον φρουρό
//! INV-DET-1 (bodleasons_mid.wav, "spotify") ώστε τα
//! νούμερα εδώ και το ντετερμινιστικό hash εκεί να
//! περιγράφουν το ΙΔΙΟ σύμπαν.
//!
//! ΔΕΝ είναι gate — δεν κρίνει τιμές. Τα κατώφλια ζουν
//! στους φρουρούς (four_stem_contract, spatial_folddown,
//! inv_qa_*). Εδώ μόνο τυπώνονται, μηχανικά parseable,
//! για να τα δει ο άνθρωπος και το MEASURE βήμα (tee+sha).

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

/// Ίδιο ιδίωμα με inv_det_1_render_determinism: το fixture
/// ζει ΜΕΣΑ στο δέντρο, CARGO_MANIFEST_DIR ανεξάρτητο CWD.
fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")
}

#[test]
#[ignore = "oracle baseline instrument — full render (~Xs), run explicitly at freeze/compare points"]
fn oracle_certificate() {
    let input_path = fixture_path();
    if !input_path.exists() {
        panic!(
            "ORACLE-CERT fixture missing — the instrument must not run vacuously: {}",
            input_path.display()
        );
    }

    // Κανένα test στο home — identity σε tempdir (κανόνας).
    let masters_dir = tempfile::TempDir::new().unwrap();
    let identity_dir = masters_dir.path().join("identity_oracle");
    std::env::set_var("M0_IDENTITY_PATH", &identity_dir);

    let req = MasterRequest {
        audio_path: input_path.to_str().unwrap().to_string(),
        preset_id: "spotify".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("oracle_test".to_string()),
        track_id: Some("oracle1".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: Some(true),
        use_nmfd: Some(true),
    };

    let state_tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tempfile::TempDir::new().unwrap();

    let (blob, _, _pcm, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "oracle-spotify-1".to_string(),
        state_tmp.path().to_str().unwrap(),
        out_dir.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    let loudness = blob.loudness().expect("ORACLE-CERT expects Certified variant");
    let quality = blob.quality().expect("ORACLE-CERT expects Certified variant");

    println!(
        "[ORACLE-CERT] preset=spotify fixture=bodleasons_mid lufs={:.2} tp={:.2} lra={:.2} rms={:.2}",
        loudness.integrated_lufs,
        loudness.true_peak_dbtp,
        loudness.lra,
        quality.rms_db,
    );

    let master_path = artifacts.persisted_master.expect("Master not produced");
    let master_sha256 = get_sha256(&master_path);
    let pcm_blake3 = blob.core.pcm_blake3.clone().unwrap_or_default();

    println!(
        "[ORACLE-CERT-HASH] pcm_blake3={} master_sha256={}",
        pcm_blake3, master_sha256,
    );
}
