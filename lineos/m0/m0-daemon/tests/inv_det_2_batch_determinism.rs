//! INV-DET-2: ίδιο batch + ίδια σειρά → ίδια outputs, byte για byte.
//!
//! Το INV-DET-1 καλύπτει μεμονωμένο track, όπου ο conductor γράφει πριν
//! ξεκινήσει το render και δεν υπάρχει κούρσα. Εδώ ελέγχεται η περίπτωση
//! όπου το ΠΛΑΙΣΙΟ (θέση στο batch) έχει νόημα.
//!
//! ΠΡΟΒΛΕΨΗ: PASS — Η εκτέλεση του batch είναι αυστηρά σειριακή, το EarFatigue
//! μοντέλο υπολογίζει ντετερμινιστικά τα deltas στο 1ο νήμα και τα εφαρμόζει
//! στο `head_state_ptr` (ArcSwap) πριν από κάθε render. Το DSP rendering
//! έχει αποδειχθεί ντετερμινιστικό στο INV-DET-1.
//!
//! FAIL = Εύρημα πρώτης γραμμής (μη ντετερμινιστικό ΠΛΑΙΣΙΟ / race condition
//! στο ArcSwap / μη επαναλήψιμη κατάσταση). Σε περίπτωση αποτυχίας: STOP
//! και αναφορά.

use arc_swap::ArcSwap;
use m0d::agents::batch::run_batch;
use m0d::agents::operator::MasteringParams;
use m0d::blob_store::BlobStore;
use m0d::domain::nodes::album_certificate_node::AlbumCertificate;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;
use xaak::repo::DspState;

fn fixture_path_1() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")
}

fn fixture_path_2() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav")
}

#[tokio::test]
#[ignore = "full batch render x2 (~12s measured 2026-08-21, release) — run explicitly on any audio-touching wire"]
async fn inv_det_2_batch_determinism() {
    let input_path_1 = fixture_path_1();
    let input_path_2 = fixture_path_2();

    if !input_path_1.exists() {
        panic!(
            "INV-DET-2 fixture 1 missing — the gate must not pass vacuously: {}",
            input_path_1.display()
        );
    }
    if !input_path_2.exists() {
        panic!(
            "INV-DET-2 fixture 2 missing — the gate must not pass vacuously: {}",
            input_path_2.display()
        );
    }

    let item1 = MasteringParams::minimal(
        input_path_1.to_str().unwrap().to_string(),
        "spotify".to_string(),
        -14.0,
        -1.0,
        "batch_sess_1".to_string(),
    );

    let item2 = MasteringParams::minimal(
        input_path_2.to_str().unwrap().to_string(),
        "spotify".to_string(),
        -14.0,
        -1.0,
        "batch_sess_2".to_string(),
    );

    let items = vec![item1, item2];

    // ── RUN 1: Φρέσκος κόσμος ──────────────────────────────────
    let blob_store_1 = BlobStore::new();
    let certs_dir_1 = tempfile::TempDir::new().unwrap();
    let head_state_1 = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let (album_tx_1, _) = broadcast::channel(64);

    let outputs_1 = run_batch(
        "batch_det_run_1",
        items.clone(),
        head_state_1,
        &blob_store_1,
        certs_dir_1.path().to_str().unwrap(),
        &album_tx_1,
    )
    .await
    .expect("run_batch 1 failed");

    // ── RUN 2: Φρέσκος κόσμος ──────────────────────────────────
    let blob_store_2 = BlobStore::new();
    let certs_dir_2 = tempfile::TempDir::new().unwrap();
    let head_state_2 = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let (album_tx_2, _) = broadcast::channel(64);

    let outputs_2 = run_batch(
        "batch_det_run_2",
        items.clone(),
        head_state_2,
        &blob_store_2,
        certs_dir_2.path().to_str().unwrap(),
        &album_tx_2,
    )
    .await
    .expect("run_batch 2 failed");

    // ── ASSERTIONS: Per-Track Outputs ────────────────────────
    assert_eq!(outputs_1.len(), 2, "Run 1 must return 2 track outputs");
    assert_eq!(outputs_2.len(), 2, "Run 2 must return 2 track outputs");

    // Track #1 (Anchor Track):
    assert_eq!(outputs_1[0].status, "ok", "Track #1 run1 failed");
    assert_eq!(outputs_2[0].status, "ok", "Track #1 run2 failed");
    assert_eq!(
        outputs_1[0].pcm_blake3, outputs_2[0].pcm_blake3,
        "Track #1 PCM BLAKE3 hash mismatch run1 vs run2"
    );
    assert_eq!(
        outputs_1[0].output_lufs, outputs_2[0].output_lufs,
        "Track #1 Output LUFS mismatch run1 vs run2"
    );

    // Track #2 (EarFatigue Recovery Path — ΠΛΑΙΣΙΟ):
    assert_eq!(outputs_1[1].status, "ok", "Track #2 run1 failed");
    assert_eq!(outputs_2[1].status, "ok", "Track #2 run2 failed");
    assert_eq!(
        outputs_1[1].pcm_blake3, outputs_2[1].pcm_blake3,
        "Track #2 PCM BLAKE3 hash mismatch (EarFatigue context path non-deterministic)"
    );
    assert_eq!(
        outputs_1[1].output_lufs, outputs_2[1].output_lufs,
        "Track #2 Output LUFS mismatch run1 vs run2"
    );

    // ── ASSERTIONS: Album Certificates ───────────────────────
    // Το write_to_disk ορίζει ΤΟ ΙΔΙΟ το όνομα του αρχείου — δόγμα Ι:
    // δεν το μαντεύουμε (το πρώτο τρέξιμο 21/08 απέτυχε ΑΚΡΙΒΩΣ
    // εδώ, με όλα τα determinism asserts ήδη περασμένα). Ο tempdir
    // είναι φρέσκος και δικός μας: το ΜΟΝΑΔΙΚΟ .json μέσα του ΕΙΝΑΙ
    // το album certificate — το σαρώνουμε, δεν το ονομάζουμε.
    let find_cert = |dir: &Path| -> PathBuf {
        let mut jsons: Vec<PathBuf> = std::fs::read_dir(dir)
            .expect("read certs dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        assert_eq!(
            jsons.len(),
            1,
            "expected exactly one album certificate json in {:?}, found {}",
            dir,
            jsons.len()
        );
        jsons.remove(0)
    };
    let cert_path_1 = find_cert(certs_dir_1.path());
    let cert_path_2 = find_cert(certs_dir_2.path());

    let cert_json_1 = std::fs::read_to_string(&cert_path_1).expect("Failed to read cert 1");
    let cert_json_2 = std::fs::read_to_string(&cert_path_2).expect("Failed to read cert 2");

    let cert_1: AlbumCertificate =
        serde_json::from_str(&cert_json_1).expect("Failed to parse cert 1 JSON");
    let cert_2: AlbumCertificate =
        serde_json::from_str(&cert_json_2).expect("Failed to parse cert 2 JSON");

    // ΕΛΕΓΧΟΣ ΝΤΕΤΕΡΜΙΝΙΣΤΙΚΩΝ ΠΕΔΙΩΝ ALBUM CERTIFICATE:
    assert_eq!(cert_1.track_count, cert_2.track_count, "Album track count mismatch");
    assert_eq!(cert_1.anchor_track_idx, cert_2.anchor_track_idx, "Anchor track index mismatch");
    assert_eq!(cert_1.anchor_lufs, cert_2.anchor_lufs, "Anchor LUFS mismatch");
    assert_eq!(
        cert_1.album_hash, cert_2.album_hash,
        "Album hash (SHA-256 of track PCM BLAKE3 hashes) mismatch"
    );
    assert_eq!(cert_1.track_lufs, cert_2.track_lufs, "Per-track LUFS vector mismatch");
    assert_eq!(
        cert_1.ear_fatigue_applied, cert_2.ear_fatigue_applied,
        "EarFatigue applied boolean vector mismatch"
    );
    assert_eq!(cert_1.pipeline_version, cert_2.pipeline_version, "Pipeline version mismatch");

    // ΣΗΜΕΙΩΣΗ: Τα πεδία `created_at` (timestamp εκτέλεσης) και `track_blob_ids` (random UUIDs)
    // εξαιρούνται ρητά από τη σύγκριση καθώς είναι μη-ντετερμινιστικά metadata του runtime.
}
