use m0d::blob_store::{
    find_sidecar, read_sidecar, write_sidecar, BlobStore, BlobVariant, SidecarError,
    StoredBlobCore, StoredBlobV2, StoredLoudness, StoredProvenance, StoredQuality, StoredSpatial,
};
use tempfile::tempdir;

fn stub_certified_blob(id: &str) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: id.into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-08-20T22:00:00Z".into(),
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
fn test_inv_pi_1_certificate_survival() {
    let masters_dir = tempdir().expect("failed to create tempdir");
    let identity_dir = masters_dir.path().join("identity_test");
    std::env::set_var("M0_IDENTITY_PATH", &identity_dir);

    let masters_root = masters_dir.path().to_str().expect("valid utf8 path");

    let project_id = "project_pi";
    let blob_id = "track_pi_001";

    let project_dir = masters_dir.path().join(project_id);
    std::fs::create_dir_all(&project_dir).expect("create project dir");
    let master_flac_path = project_dir.join(format!("{blob_id}.flac"));
    let original_bytes = b"ORIGINAL_STEREO_FLAC_DATA_BYTES_STUB_1234567890";
    std::fs::write(&master_flac_path, original_bytes).expect("write stub flac");

    let blob = stub_certified_blob(blob_id);

    // 2. write_sidecar -> Ok. ΤΩΡΑ: ΝΕΟ BlobStore::new() — άδειο = ο daemon «πέθανε».
    let sidecar_path = write_sidecar(masters_root, project_id, &blob, &master_flac_path)
        .expect("write_sidecar failed");
    assert!(sidecar_path.exists());

    // Check envelope fields directly from raw JSON
    let envelope_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&sidecar_path).expect("read sidecar file")).expect("parse sidecar json");
    let key_id = envelope_json.get("key_id").and_then(|v| v.as_str()).expect("key_id must exist");
    let signer_pk = envelope_json.get("signer_public_key").and_then(|v| v.as_str()).expect("signer_public_key must exist");

    assert!(key_id.starts_with("m0-"), "key_id must start with m0-");
    assert_ne!(key_id, "m0-", "key_id must not be empty");
    assert_eq!(signer_pk.len(), 64, "signer_public_key must be 64 hex chars");

    let _empty_store = BlobStore::new();

    // 3. Η ΑΝΑΣΤΑΣΗ: find_sidecar(masters_root, id) -> Some -> read_sidecar -> asserts
    let found_option = find_sidecar(masters_root, blob_id).expect("find_sidecar error");
    assert!(found_option.is_some(), "sidecar must be found");
    let found_path = found_option.unwrap();
    assert_eq!(found_path, sidecar_path);

    let rehydrated = read_sidecar(&found_path).expect("read_sidecar failed during resurrection");
    assert!(rehydrated.is_certified(), "rehydrated blob must be Certified");
    let loudness = rehydrated.loudness().expect("loudness must exist");
    assert!(
        (loudness.integrated_lufs - (-14.0)).abs() < 1e-5,
        "loudness must be -14.0"
    );
    assert_eq!(rehydrated.core.id, blob_id);
    assert_eq!(rehydrated.core.sample_rate, 48000);
    assert_eq!(rehydrated.core.channels, 2);
    assert_eq!(rehydrated.core.num_frames, 96000);
    // F-074: input_pcm_hash (the real content hash) must survive the
    // sidecar roundtrip as Some(non-empty) on a fresh cert.
    match &rehydrated.core.input_pcm_hash {
        Some(h) => assert!(!h.is_empty(), "input_pcm_hash must not be empty"),
        None => panic!("input_pcm_hash must be Some on a fresh cert"),
    }

    // 4. Η ΑΡΝΗΤΙΚΗ ΠΛΕΥΡΑ:
    // (α) πείραξε ΕΝΑ byte του master.flac -> MasterHashMismatch
    let corrupted_bytes = b"CORRUPTED_TEREO_FLAC_DATA_BYTES_STUB_1234567890";
    std::fs::write(&master_flac_path, corrupted_bytes).expect("write corrupted flac");
    let err_mismatch = read_sidecar(&found_path).expect_err("read_sidecar must fail on hash mismatch");
    assert!(
        matches!(err_mismatch, SidecarError::MasterHashMismatch { .. }),
        "expected MasterHashMismatch, got {err_mismatch:?}"
    );

    // (β) σβήσε το master -> MasterMissing
    std::fs::remove_file(&master_flac_path).expect("remove flac");
    let err_missing = read_sidecar(&found_path).expect_err("read_sidecar must fail on missing master");
    assert!(
        matches!(err_missing, SidecarError::MasterMissing(_)),
        "expected MasterMissing, got {err_missing:?}"
    );

    // (γ) find_sidecar σε ανύπαρκτο id -> Ok(None), ΟΧΙ Err
    let nonexistent_option = find_sidecar(masters_root, "nonexistent_track_id").expect("find_sidecar error");
    assert!(nonexistent_option.is_none(), "nonexistent id must return Ok(None)");
}
