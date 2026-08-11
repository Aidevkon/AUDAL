//! ACX delivery check — end-to-end plumbing test.
//!
//! Drives the FULL Episode pipeline via `run_dsp` with preset "acx" and
//! verifies the four `StoredLoudness.acx_*` fields arrive in the
//! `StoredBlob`. Does NOT test numerical calibration (covered by the
//! oracle tests in sp314-dsp); tests ONLY that the conditional wiring
//! (`wants_acx`, `run_trunk_metrics_with_acx`, `StreamingCertData.acx`,
//! `assemble_blob`) delivers the data end to end.

/// Synthesize a speech-like stereo WAV: tone bursts at ~-20 dBFS with
/// genuine quiet gaps at ~-70 dBFS. This gives a noise floor well
/// below the RMS, exercising a realistic `Some(floor)`.
fn generate_speech_like_fixture(sr: u32, dur_secs: f32) -> Vec<f32> {
    let n = (sr as f32 * dur_secs) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        // 1 second speech burst, 1 second quiet gap, repeating
        let in_burst = (t % 2.0) < 1.0;
        let v = if in_burst {
            // ~-20 dBFS speech-like tone
            0.1 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
                + 0.03 * (2.0 * std::f32::consts::PI * 600.0 * t).sin()
        } else {
            // ~-70 dBFS quiet gap (room tone)
            0.000_3 * (2.0 * std::f32::consts::PI * 120.0 * t).sin()
        };
        out.push(v); // L
        out.push(v); // R (mono-ish voice)
    }
    out
}

fn write_wav(samples: &[f32], sr: u32, path: &str) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

/// Run the full pipeline (run_dsp) and return the StoredBlob.
fn run_pipeline(wav_path: &str, preset: &str, track_id: &str) -> m0d::blob_store::StoredBlobV2 {
    let req = m0d::handlers::master::MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: preset.to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("acx_e2e".to_string()),
        track_id: Some(track_id.to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };
    let state = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        xaak::repo::DspState::default(),
    ));
    let state_tmp = tempfile::TempDir::new().unwrap();
    let (blob, _, _, _, _, _artifacts) = m0d::domain::dsp_pipeline::run_dsp(
        &req,
        std::time::Instant::now(),
        state,
        None,
        None,
        format!("acx-e2e-{}", track_id),
        state_tmp.path().to_str().unwrap(),
        "/tmp",
    )
    .expect("run_dsp failed");
    blob
}

// ── A1 + A2: ACX preset populates all four fields with sane values ──
#[test]
fn acx_preset_populates_delivery_check_fields() {
    let sr = 48_000;
    // Unique path to avoid collision with the 45 hardcoded /tmp/ paths
    let wav_path = "/tmp/e2e_acx_certificate_acx_preset.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path);

    let blob = run_pipeline(wav_path, "acx", "acx-check");

    // A1: all three measurement fields are Some with sane dBFS ranges
    let peak = blob
        .loudness().expect("test expects Certified")
        .acx_sample_peak_db
        .expect("acx_sample_peak_db must be Some for acx preset");
    let rms = blob
        .loudness().expect("test expects Certified")
        .acx_rms_db
        .expect("acx_rms_db must be Some for acx preset");
    let floor = blob
        .loudness().expect("test expects Certified")
        .acx_noise_floor_db
        .expect("acx_noise_floor_db must be Some for acx preset");

    assert!(
        peak >= -60.0 && peak <= 0.0,
        "acx_sample_peak_db {peak} outside sane range [-60, 0]"
    );
    assert!(
        rms >= -80.0 && rms <= 0.0,
        "acx_rms_db {rms} outside sane range [-80, 0]"
    );
    assert!(
        floor >= -144.0 && floor <= 0.0,
        "acx_noise_floor_db {floor} outside sane range [-144, 0]"
    );

    // A2: acx_compliant is Some, and its value agrees with recomputing
    // passes_acx() from the three stored numbers — the flag can never
    // silently diverge from the numbers it summarizes.
    let compliant = blob
        .loudness().expect("test expects Certified")
        .acx_compliant
        .expect("acx_compliant must be Some for acx preset");

    // Recompute passes_acx from the stored numbers using the SOURCE
    // constants — copies here would go stale silently if the limits
    // ever moved.
    use sp314_dsp::analysis::acx_check::{
        ACX_MAX_NOISE_FLOOR_DB, ACX_MAX_PEAK_DB, ACX_MAX_RMS_DB, ACX_MIN_RMS_DB,
    };
    let recomputed = peak <= ACX_MAX_PEAK_DB
        && rms <= ACX_MAX_RMS_DB
        && rms >= ACX_MIN_RMS_DB
        && floor <= ACX_MAX_NOISE_FLOOR_DB;

    assert_eq!(
        compliant, recomputed,
        "acx_compliant ({compliant}) disagrees with recomputed \
         passes_acx ({recomputed}) from peak={peak}, rms={rms}, floor={floor}"
    );

    eprintln!(
        "[ACX e2e] peak={peak:.2} rms={rms:.2} floor={floor:.2} \
         compliant={compliant}"
    );

    // Cleanup
    let _ = std::fs::remove_file(wav_path);
}

// ── A3: podcast preset yields all four fields None (conditional) ──
#[test]
fn podcast_preset_omits_acx_fields() {
    let sr = 48_000;
    let wav_path = "/tmp/e2e_acx_certificate_podcast_preset.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path);

    let blob = run_pipeline(wav_path, "podcast", "podcast-ctrl");

    assert!(
        blob.loudness().expect("test expects Certified").acx_sample_peak_db.is_none(),
        "acx_sample_peak_db must be None for podcast preset, got {:?}",
        blob.loudness().expect("test expects Certified").acx_sample_peak_db
    );
    assert!(
        blob.loudness().expect("test expects Certified").acx_rms_db.is_none(),
        "acx_rms_db must be None for podcast preset, got {:?}",
        blob.loudness().expect("test expects Certified").acx_rms_db
    );
    assert!(
        blob.loudness().expect("test expects Certified").acx_noise_floor_db.is_none(),
        "acx_noise_floor_db must be None for podcast preset, got {:?}",
        blob.loudness().expect("test expects Certified").acx_noise_floor_db
    );
    assert!(
        blob.loudness().expect("test expects Certified").acx_compliant.is_none(),
        "acx_compliant must be None for podcast preset, got {:?}",
        blob.loudness().expect("test expects Certified").acx_compliant
    );

    // Cleanup
    let _ = std::fs::remove_file(wav_path);
}

// ── A4: cert_json (ExecutionCertificate) has no "acx" keys ──
#[test]
fn cert_json_has_no_acx_keys() {
    let sr = 48_000;
    let wav_path = "/tmp/e2e_acx_certificate_cert_clean.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path);

    let blob = run_pipeline(wav_path, "acx", "cert-clean");

    let cert_str = blob.aether_cert().expect("aether_cert must be Some");
    let cert: serde_json::Value =
        serde_json::from_str(cert_str).expect("cert_json must be valid JSON");

    // The provenance certificate must stay clean of delivery QA fields.
    let cert_obj = cert.as_object().expect("cert must be a JSON object");
    for key in cert_obj.keys() {
        assert!(
            !key.contains("acx"),
            "ExecutionCertificate must NOT contain ACX keys, \
             but found key: {key}"
        );
    }

    // Cleanup
    let _ = std::fs::remove_file(wav_path);
}
