use lineos_types::{AudioChunk, Bmr128Schema, StereoBuffer, LoudnessTarget, GoldenBlob, BlobType, GoldenInputProfile, LufsReport, MasteringIntent};

#[test]
fn stereo_buffer_duration_correct() {
    let buf = StereoBuffer::new(48000, 48000);
    assert_eq!(buf.duration_secs(), 1.0);
}

#[test]
fn loudness_target_spotify_correct() {
    let target = LoudnessTarget::spotify();
    assert_eq!(target.target_lufs, -14.0);
    assert_eq!(target.max_true_peak_db, -1.0);
}

#[test]
fn golden_blob_serializes_roundtrip() {
    let blob = GoldenBlob {
        blob_type: BlobType::Master,
        input_profile: GoldenInputProfile {
            integrated_lufs: -18.0,
            true_peak_dbfs: -2.0,
            crest_factor_db: 16.0,
            spectral_centroid: 2000.0,
            dynamic_range_lu: 10.0,
            stereo_correlation: 0.8,
        },
        output_lufs: LufsReport {
            integrated_lufs: -14.0,
            true_peak_dbfs: -1.0,
            loudness_range_lu: 8.0,
            short_term_lufs: Some(-13.0),
        },
        sha256: "dummyhash".to_string(),
        preset_name: "SpotifyV3".to_string(),
        engine_version: "v3.0.0".to_string(),
        schema_version: 1,
        aether_cert: None,
        aether_persona: None,
        aether_config: None,
    };

    let json = serde_json::to_string(&blob).unwrap();
    let deserialized: GoldenBlob = serde_json::from_str(&json).unwrap();

    assert_eq!(blob.blob_type, deserialized.blob_type);
    assert_eq!(blob.sha256, deserialized.sha256);
}

#[test]
fn mastering_intent_spotify() {
    let intent = MasteringIntent::spotify();
    assert_eq!(intent.preset_name, "SpotifyV3");
    assert_eq!(intent.target.platform, "spotify");
}

#[test]
fn v29_aliases_work() {
    let _: AudioChunk = StereoBuffer::new(48000, 512);
    let _: Bmr128Schema = LoudnessTarget::spotify();
}
