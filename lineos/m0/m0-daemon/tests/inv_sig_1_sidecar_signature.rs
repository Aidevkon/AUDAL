//! INV-SIG-1: v0 §6 Σ1α — payload signature is byte-detached.
//!
//! The signature covers the full serialized sidecar bytes with
//! `payload_signature` zeroed to "". It must be recovered by zeroing
//! the bytes on disk back out — never by re-serializing the struct —
//! so ANY byte-level change (a tampered value, or even a harmless
//! reformat) invalidates it.

use m0d::blob_store::{
    write_sidecar, BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness, StoredProvenance,
    StoredQuality, StoredSpatial,
};
use tempfile::tempdir;

mod common;
use common::verify_sidecar_bytes;

fn stub_signed_blob(id: &str) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-08-21T00:00:00Z".into(),
            input_path_hash: "aabb11223344".into(),
            input_pcm_hash: Some("pcm-aabb11223344".into()),
            seed: 42,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: Some("1234567890abcdef".into()),
            cert_signature: Some("sig_mock_12345".into()),
            audio_path: std::sync::Arc::new(lineos_types::audio::ManagedPcm::default()),
            sample_rate: 48000,
            channels: 2,
            num_frames: 96000,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness {
                integrated_lufs: -14.0,
                ..Default::default()
            },
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: Default::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
        },
    }
}

#[test]
fn test_inv_sig_1_sidecar_signature() {
    let masters_dir = tempdir().expect("failed to create tempdir");
    let identity_dir = masters_dir.path().join("identity_sig_test");
    std::env::set_var("M0_IDENTITY_PATH", &identity_dir);

    let masters_root = masters_dir.path().to_str().expect("valid utf8 path");
    let project_id = "project_sig";
    let blob_id = "track_sig_001";

    let project_dir = masters_dir.path().join(project_id);
    std::fs::create_dir_all(&project_dir).expect("create project dir");
    let master_flac_path = project_dir.join(format!("{blob_id}.flac"));
    std::fs::write(&master_flac_path, b"SIG_TEST_FLAC_BYTES_STUB_1234567890")
        .expect("write stub flac");

    let blob = stub_signed_blob(blob_id);
    let sidecar_path = write_sidecar(masters_root, project_id, &blob, &master_flac_path)
        .expect("write_sidecar failed");

    let raw = std::fs::read(&sidecar_path).expect("read sidecar");
    let text = String::from_utf8(raw).expect("sidecar must be UTF-8");

    // (α) fresh cert: read bytes, zero the sig back out, verify — OK.
    verify_sidecar_bytes(&text).expect("fresh certificate must verify");

    // Fixture assumption the (β) tamper below depends on: sample_rate
    // 48000 -> lookahead_samples 240 (sp314_dsp::limiter::core).
    assert!(
        text.contains("\"declared_latency_samples\": 240"),
        "fixture assumption broken: expected declared_latency_samples: 240"
    );

    // (β) TAMPER: flip 240 -> 241 directly in the bytes on disk, not
    // through the struct — exactly what corruption or an attacker
    // would do. Byte-detached signing must catch it.
    let tampered = text.replacen(
        "\"declared_latency_samples\": 240",
        "\"declared_latency_samples\": 241",
        1,
    );
    assert_ne!(tampered, text, "tamper replace found nothing to replace");
    let err = verify_sidecar_bytes(&tampered)
        .expect_err("tampered declared_latency_samples must fail verification");
    eprintln!("(β) tamper correctly rejected: {err}");

    // (γ) REFORMAT: add one space, zero semantic change — still FAILS.
    // This is CORRECT, not a bug: Σ1α signs bytes, not JSON meaning,
    // precisely so a "harmless" reformat can't smuggle a change past a
    // semantic-equality check that would otherwise wave it through.
    let reformatted = text.replacen("\"schema\":", "\"schema\" :", 1);
    assert_ne!(reformatted, text, "reformat replace found nothing to replace");
    let err = verify_sidecar_bytes(&reformatted)
        .expect_err("reformatted (semantically identical) bytes must fail verification");
    eprintln!("(γ) reformat correctly rejected: {err}");
}
