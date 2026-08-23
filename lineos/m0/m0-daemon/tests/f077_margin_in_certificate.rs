//! §5.3 IMPLEMENT (2026-08-24): the margin-adjusted verdict now lands in the
//! certificate's `delivery_checks` (StoredLoudness), written by
//! `run_deliver_core` at the same site as `output_delivery_*`, from
//! `AcxCheckReport::margin_checks()` — the SAME per-metric rule
//! `passes_acx_with_margin()` uses (no second implementation, §Ξ).
//!
//! Fixture math (validated against acx_export_rms_window.rs's measured
//! output — burst_amp=0.2 -> -20.00 dB, burst_amp=0.35 -> -15.14 dB,
//! burst_amp=0.02 -> -40.00 dB, all exact matches to 20*log10(burst_amp/2)):
//! a 50%-duty burst of a sine at `burst_amp` against a -70 dBFS quiet floor
//! has whole-file RMS ≈ burst_amp/2 (quiet floor's contribution is
//! negligible). Solving 20*log10(burst_amp/2) = target gives:
//!   target -22.9 dB -> burst_amp = 0.143244  (EDGE: inside nominal
//!     [-23,-18], outside margin-adjusted [-22.65,-18.10])
//!   target -20.5 dB -> burst_amp = 0.188812  (MID: inside both)
//! Peak for both (≈ burst_amp itself, ~-15..-17 dB) and noise floor (quiet
//! segment ≈ -73 dB) sit far inside every threshold, so RMS alone decides.
//!
//! PREDICTIONS, written before the test runs:
//! (1) EDGE fixture: delivery_checks has exactly 4 entries; the rms/min
//!     entry has verdict "fail", required_db -23.0, margin_applied_db 0.35;
//!     the other three ("rms"/max, "peak"/max, "noise_floor"/max) are
//!     "pass". The rewritten sidecar's signature verifies.
//! (2) MID fixture: all 4 entries are "pass", with the SAME required_db/
//!     margin_applied_db values stored (thresholds don't move, only the
//!     verdict does).

mod common;

use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{
    find_sidecar, write_sidecar, BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness,
    StoredProvenance, StoredQuality, StoredSpatial,
};
use m0d::dsp::signal_health::DeadAirSummary;
use m0d::handlers::deliver::{run_deliver_core, DeliverRequest, DeliveryPlan, PlanEntry};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;

/// Same shape as acx_export_rms_window.rs's generate_pcm_at_amplitude:
/// 440 Hz sine, 2 s cycle, 50% duty burst at `burst_amp`, -70 dBFS quiet
/// floor for the other 50% (needed so noise floor != RMS, see that file's
/// comment).
fn generate_pcm_at_amplitude(path: &Path, burst_amp: f32) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    for frame in 0..(48000 * 12) {
        let sec = frame as f32 / 48000.0;
        let is_burst = (sec % 2.0) < 1.0;
        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        sample *= if is_burst { burst_amp } else { 0.000316 };
        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap();
    }
}

fn make_certified_blob(id: &str, pcm_path: PathBuf) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-08-24T00:00:00Z".into(),
            input_path_hash: "f077margin".into(),
            input_pcm_sha256: Some("pcm-f077margin".into()),
            seed: 7,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: Some("blake3-stub".into()),
            cert_signature: None,
            audio_path: Arc::new(ManagedPcm::new(pcm_path)),
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000 * 12,
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

/// Runs the fixture through the full deliver path (masters_dir + project_id
/// present, so the sidecar gets rewritten — same wiring as
/// output_acx_delivered_certificate.rs) and returns the rewritten sidecar's
/// raw JSON text plus the `delivery_checks` array.
fn deliver_and_read_checks(label: &str, burst_amp: f32) -> (String, serde_json::Value) {
    let masters_tmp = tempdir().expect("masters tempdir");
    let identity_dir = masters_tmp.path().join(format!("identity_{label}"));
    std::env::set_var("M0_IDENTITY_PATH", &identity_dir);

    let masters_root = masters_tmp.path().to_str().expect("utf8 path").to_string();
    let project_id = format!("proj_{label}");
    let blob_id = format!("track_{label}");

    let project_dir = masters_tmp.path().join(&project_id);
    std::fs::create_dir_all(&project_dir).expect("project dir");
    let master_flac_path = project_dir.join(format!("{blob_id}.flac"));
    std::fs::write(&master_flac_path, b"F077_MARGIN_TEST_FLAC_STUB").expect("stub flac");

    let pcm_path = PathBuf::from(format!("/tmp/f077_margin_{label}.pcm"));
    generate_pcm_at_amplitude(&pcm_path, burst_amp);

    let blob = make_certified_blob(&blob_id, pcm_path.clone());
    write_sidecar(&masters_root, &project_id, &blob, &master_flac_path)
        .expect("initial sign (pre-deliver state)");

    let out_tmp = tempdir().expect("out tempdir");
    let req = DeliverRequest {
        output_dir: Some(out_tmp.path().to_string_lossy().into_owned()),
        book_title: format!("F077 Margin {label}"),
        entries: vec![],
    };
    let plan = DeliveryPlan {
        book_title_sanitized: format!("F077_Margin_{label}"),
        entries: vec![PlanEntry {
            index: 1,
            title: "Chapter 1".into(),
            role: "chapter".into(),
            filename: "01_Chapter_1.mp3".into(),
            duration_ms: 12000,
            exists: true,
            track_id: blob_id.clone(),
            audio_path: pcm_path.to_string_lossy().into_owned(),
            resolved_blob: Some(blob),
        }],
    };

    let resp = run_deliver_core(&req, plan, Some(&masters_root), Some(&project_id))
        .expect("run_deliver_core failed");

    let sidecar_path = find_sidecar(&masters_root, &blob_id)
        .unwrap()
        .expect("sidecar must exist after deliver");
    let after_text = std::fs::read_to_string(&sidecar_path).unwrap();
    let after_json: serde_json::Value = serde_json::from_str(&after_text).unwrap();
    let checks = after_json["payload"]["variant"]["Certified"]["loudness"]["delivery_checks"]
        .clone();

    let _ = std::fs::remove_file(&pcm_path);
    if let Some(book_dir) = resp.book_dir {
        let _ = std::fs::remove_dir_all(book_dir);
    }

    (after_text, checks)
}

fn find_check<'a>(checks: &'a serde_json::Value, metric: &str, bound: &str) -> &'a serde_json::Value {
    checks
        .as_array()
        .expect("delivery_checks must be an array")
        .iter()
        .find(|c| c["metric"] == metric && c["bound"] == bound)
        .unwrap_or_else(|| panic!("no {metric}/{bound} entry in delivery_checks: {checks:?}"))
}

#[test]
#[ignore = "writes mp3, runs LAME encoder, signs a certificate"]
fn edge_fixture_fails_rms_floor_margin_in_certificate() {
    let (after_text, checks) = deliver_and_read_checks("edge", 0.143244);

    let arr = checks.as_array().expect("delivery_checks array");
    assert_eq!(
        arr.len(),
        4,
        "expected 4 delivery_checks (rms-min, rms-max, peak, noise_floor), got {arr:?}"
    );

    let rms_min = find_check(&checks, "rms", "min");
    assert_eq!(rms_min["verdict"], "fail", "{rms_min:?}");
    assert_eq!(rms_min["required"], -23.0, "{rms_min:?}");
    assert_eq!(rms_min["margin_applied"], 0.35, "{rms_min:?}");
    assert_eq!(rms_min["unit"], "db", "{rms_min:?}");

    for (metric, bound) in [("rms", "max"), ("peak", "max"), ("noise_floor", "max")] {
        let c = find_check(&checks, metric, bound);
        assert_eq!(c["verdict"], "pass", "{metric}/{bound} unexpectedly failed: {c:?}");
    }

    common::verify_sidecar_bytes(&after_text).expect("rewritten sidecar must verify");
}

#[test]
#[ignore = "writes mp3, runs LAME encoder, signs a certificate"]
fn mid_window_fixture_passes_all_margin_checks_in_certificate() {
    let (after_text, checks) = deliver_and_read_checks("mid", 0.188812);

    let arr = checks.as_array().expect("delivery_checks array");
    assert_eq!(arr.len(), 4, "expected 4 delivery_checks, got {arr:?}");

    for (metric, bound, required, margin) in [
        ("rms", "min", -23.0, 0.35),
        ("rms", "max", -18.0, 0.10),
        ("peak", "max", -3.0, 0.20),
        ("noise_floor", "max", -60.0, 0.0),
    ] {
        let c = find_check(&checks, metric, bound);
        assert_eq!(c["verdict"], "pass", "{metric}/{bound} unexpectedly failed: {c:?}");
        assert_eq!(c["required"], required, "{metric}/{bound} required: {c:?}");
        assert_eq!(c["margin_applied"], margin, "{metric}/{bound} margin: {c:?}");
        assert_eq!(c["unit"], "db", "{metric}/{bound} unit: {c:?}");
    }

    common::verify_sidecar_bytes(&after_text).expect("rewritten sidecar must verify");
}
