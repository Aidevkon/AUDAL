//! OUTPUT-ACX certificate wiring (§5.6 Δ2, 2026-08-22 IMPLEMENT).
//!
//! Model: acx_export_rms_window.rs (known-fixture blob, real
//! export_mp3_acx call) + inv_sig_1_sidecar_signature.rs (tempdir
//! masters_dir, fresh identity, anchor->zero->verify — imported via
//! tests/common, not re-copied here).
//!
//! PREDICTIONS, written before the test runs:
//! (a) the rewritten sidecar's output_acx_{sample_peak,rms,noise_floor}_db
//!     must equal the manifest.json entry for the same track — both come
//!     from the SAME `outcome.report` (one AcxCheckAnalyzer pass over the
//!     same final buffer, export.rs:713-721). Tolerance |Δ| < 0.01 dB, not
//!     bit-for-bit: JSON round-trips an f32 through f64, which can perturb
//!     the last representable bit.
//! (b) the rewritten sidecar's payload_signature must verify against its
//!     own signer_public_key, because run_deliver_core re-signs through
//!     the EXISTING blob_store::write_sidecar — no ad-hoc signing path.

mod common;

use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{
    find_sidecar, write_sidecar, BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness,
    StoredProvenance, StoredQuality, StoredSpatial,
};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::deliver::{run_deliver_core, DeliverRequest, DeliveryPlan, PlanEntry};
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

fn generate_pcm(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    for frame in 0..(48000 * 6) {
        let sec = frame as f32 / 48000.0;
        let is_burst = (sec % 2.0) < 1.0;
        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        sample *= if is_burst { 0.1 } else { 0.000316 };
        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap(); // left and right
    }
}

fn make_certified_blob(id: &str, pcm_path: std::path::PathBuf) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-08-22T00:00:00Z".into(),
            input_path_hash: "deadbeef".into(),
            input_pcm_sha256: Some("pcm-deadbeef".into()),
            seed: 7,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: Some("blake3-stub".into()),
            cert_signature: None,
            audio_path: Arc::new(ManagedPcm::new(pcm_path)),
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000 * 6,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness {
                integrated_lufs: -20.0,
                ..Default::default()
            },
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: DeadAirSummary::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
        },
    }
}

#[test]
#[ignore = "writes mp3, runs LAME encoder, signs a certificate"]
fn output_acx_writes_and_signs_the_delivered_certificate() {
    let masters_tmp = tempdir().expect("masters tempdir");
    let identity_dir = masters_tmp.path().join("identity_output_acx_test");
    std::env::set_var("M0_IDENTITY_PATH", &identity_dir);

    let masters_root = masters_tmp.path().to_str().expect("utf8 path").to_string();
    let project_id = "proj_output_acx";
    let blob_id = "track_output_acx_001";

    let project_dir = masters_tmp.path().join(project_id);
    std::fs::create_dir_all(&project_dir).expect("project dir");
    let master_flac_path = project_dir.join(format!("{blob_id}.flac"));
    std::fs::write(&master_flac_path, b"OUTPUT_ACX_TEST_FLAC_STUB").expect("stub flac");

    let pcm_path = std::path::PathBuf::from(format!("/tmp/output_acx_test_{blob_id}.pcm"));
    generate_pcm(&pcm_path);

    let blob = make_certified_blob(blob_id, pcm_path.clone());

    // Στιγμή "μετά την υπογραφή": sidecar ήδη υπάρχει, ΧΩΡΙΣ output_acx_*
    // — αυτό ΕΙΝΑΙ η αφετηρία που το ticket περιγράφει (§5.6 Δ2).
    let sidecar_path_before = write_sidecar(&masters_root, project_id, &blob, &master_flac_path)
        .expect("initial sign (pre-deliver state)");
    let before_text = std::fs::read_to_string(&sidecar_path_before).unwrap();
    assert!(
        !before_text.contains("output_delivery_rms_db"),
        "fixture assumption broken: output_delivery_* must be ABSENT before deliver"
    );
    // Ο φρουρός της απουσίας ΔΕΝ ξεχωρίζει «δεν γράφτηκε ΑΚΟΜΑ» από «δεν
    // γράφεται ΠΟΤΕ» — μια μετονομασία τον αφήνει αυτόματα πράσινο. Το
    // ζεύγος του (ΠΑΡΟΥΣΙΑ μετά το deliver) είναι στο assert μετά το
    // run_deliver_core παρακάτω, και τα δύο πάνω στο ΙΔΙΟ literal.
    // Ο παλιός τύπος ΔΕΝ πρέπει να επιβιώνει πουθενά στην εγγραφή: το
    // serde alias διαβάζει παλιά sidecars, δεν τα ΞΑΝΑΓΡΑΦΕΙ (§5.1α).
    assert!(
        !before_text.contains("output_acx_rms_db"),
        "παλιό κλειδί output_acx_rms_db γράφτηκε — το alias είναι read-only"
    );

    let out_tmp = tempdir().expect("out tempdir");
    let req = DeliverRequest {
        output_dir: Some(out_tmp.path().to_string_lossy().into_owned()),
        book_title: "Output ACX Test".into(),
        entries: vec![],
    };
    let plan = DeliveryPlan {
        book_title_sanitized: "Output_ACX_Test".into(),
        entries: vec![PlanEntry {
            index: 1,
            title: "Chapter 1".into(),
            role: "chapter".into(),
            filename: "01_Chapter_1.mp3".into(),
            duration_ms: 6000,
            exists: true,
            track_id: blob_id.into(),
            audio_path: pcm_path.to_string_lossy().into_owned(),
            resolved_blob: Some(blob),
        }],
    };

    let resp = run_deliver_core(&req, plan, Some(&masters_root), Some(project_id))
        .expect("run_deliver_core failed");

    let manifest_path = resp.manifest_path.expect("manifest path");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let entry = &manifest["entries"][0];
    let manifest_rms = entry["rms_db"].as_f64().expect("manifest rms_db");
    let manifest_peak = entry["sample_peak_db"]
        .as_f64()
        .expect("manifest sample_peak_db");
    let manifest_floor = entry["noise_floor_db"]
        .as_f64()
        .expect("manifest noise_floor_db");

    let sidecar_path_after = find_sidecar(&masters_root, blob_id)
        .unwrap()
        .expect("sidecar must still exist after deliver");
    let after_text = std::fs::read_to_string(&sidecar_path_after).unwrap();
    let after_json: serde_json::Value = serde_json::from_str(&after_text).unwrap();
    let loudness = &after_json["payload"]["variant"]["Certified"]["loudness"];

    // ΤΟ ΖΕΥΓΟΣ ΤΟΥ ΦΡΟΥΡΟΥ ΑΠΟΥΣΙΑΣ (γραμμή ~116): ΙΔΙΟ literal, ανάποδη
    // φορά. Αν μια μετονομασία αφήσει το πάνω assert να περνάει κενό, αυτό
    // εδώ πέφτει — η απουσία μόνη της δεν αποδεικνύει τίποτα.
    assert!(
        after_text.contains("output_delivery_rms_db"),
        "output_delivery_rms_db ΛΕΙΠΕΙ μετά το deliver — ο φρουρός απουσίας \
         της γραμμής ~116 θα περνούσε κενός"
    );

    let sidecar_rms = loudness["output_delivery_rms_db"]
        .as_f64()
        .expect("output_delivery_rms_db must be present after deliver");
    let sidecar_peak = loudness["output_delivery_peak_db"]
        .as_f64()
        .expect("output_delivery_peak_db must be present after deliver");
    let sidecar_floor = loudness["output_delivery_noise_floor_db"]
        .as_f64()
        .expect("output_delivery_noise_floor_db must be present after deliver");

    // (α) — see prediction above. STOP and report if this does not hold;
    // do not loosen the tolerance to make it pass.
    assert!(
        (sidecar_rms - manifest_rms).abs() < 0.01,
        "rms mismatch: sidecar {sidecar_rms} vs manifest {manifest_rms}"
    );
    assert!(
        (sidecar_peak - manifest_peak).abs() < 0.01,
        "peak mismatch: sidecar {sidecar_peak} vs manifest {manifest_peak}"
    );
    assert!(
        (sidecar_floor - manifest_floor).abs() < 0.01,
        "noise floor mismatch: sidecar {sidecar_floor} vs manifest {manifest_floor}"
    );

    // (β)
    common::verify_sidecar_bytes(&after_text).expect("rewritten sidecar must verify");

    let _ = std::fs::remove_file(&pcm_path);
    if let Some(book_dir) = resp.book_dir {
        let _ = std::fs::remove_dir_all(book_dir);
    }
}
