use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

/// End-to-end test: synthetic 5.1 WAV →
/// spatial_conformance_path →
/// StoredBlob (channels=6, blob_type=spatial_bed)
///
/// Validates the full pipeline path:
/// decode_smart → FiveDotOne arm in decode_node
/// → spatial_conformance_path in dsp_pipeline
/// → StoredBlob with correct metadata
#[test]
fn e2e_5dot1_wav_produces_spatial_blob() {
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_5dot1_input.wav"
    );

    if !std::path::Path::new(input_path).exists() {
        panic!(
            "Test fixture missing: {}\n\
             Generate with python script",
            input_path
        );
    }

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "apple_spatial_bed".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-session-spatial-001".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None, // progress_tx
        None, // progress_map
        "job-spatial-test".to_string(),
        state_tmp.path().to_str().unwrap(),
        "/tmp",
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (blob, _spatial, _path, _model, _, _artifacts) = result.unwrap();

    assert_eq!(blob.core.channels, 6, "Expected 6 channels in spatial blob");
    assert_eq!(
        blob.core.blob_type, "spatial_bed",
        "Expected blob_type = spatial_bed"
    );
    assert!(blob.core.sample_rate == 48000, "Expected 48000 Hz sample rate");

    let pcm_path = format!("/tmp/m0d-raw-{}.pcm", blob.core.id);
    let pcm_bytes = std::fs::read(&pcm_path).expect("PCM dump should exist");
    assert_eq!(pcm_bytes.len(), 48000 * 6 * 4, "PCM dump size mismatch");

    println!(
        "Spatial blob: id={} channels={} sample_rate={} blob_type={}",
        blob.core.id, blob.core.channels, blob.core.sample_rate, blob.core.blob_type
    );

    // (1) Run In-process MultichannelLufsMeter (Secondary Check)
    use sp314_dsp::metering::MultichannelLufsMeter;
    let mut meter = MultichannelLufsMeter::new();
    for chunk in pcm_bytes.chunks_exact(24) {
        let s: [f32; 6] = core::array::from_fn(|ch| {
            f32::from_le_bytes(chunk[ch * 4..ch * 4 + 4].try_into().unwrap())
        });
        meter.process_frame(&s);
    }
    let mc_lufs = meter.finish().expect("Meter failed to compute LUFS");
    let mc_delta = (mc_lufs - -18.0).abs();

    // (2) Run FFMPEG Ground Truth (Primary Check)
    let ffmpeg_status = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output();
    let mut ffmpeg_lufs: Option<f32> = None;

    if ffmpeg_status.is_ok() {
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-f", "f32le", "-ar", "48000", "-ac", "6", "-i", &pcm_path, "-af", "ebur128", "-f",
                "null", "-",
            ])
            .output()
            .expect("Failed to execute ffmpeg");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut in_summary = false;

        for line in stderr.lines() {
            if line.contains("Summary:") {
                in_summary = true;
                continue;
            }
            if in_summary && line.contains("I:") && line.contains("LUFS") {
                // e.g. "    I:         -18.0 LUFS"
                if let Some(val_str) = line.split("LUFS").next() {
                    if let Some(num_str) = val_str.split("I:").nth(1) {
                        if let Ok(val) = num_str.trim().parse::<f32>() {
                            ffmpeg_lufs = Some(val);
                            break;
                        }
                    }
                }
            }
        }
    }

    // (3) Print results and assert (primary first)
    if let Some(ffmpeg_val) = ffmpeg_lufs {
        let ffmpeg_delta = (ffmpeg_val - -18.0).abs();
        println!(
            "Oracle (ffmpeg ebur128): measured={:.2} LUFS, target=-18.0 (delta: {:.3})",
            ffmpeg_val, ffmpeg_delta
        );
        println!(
            "Oracle (in-process meter): measured={:.2} LUFS, target=-18.0 (delta: {:.3})",
            mc_lufs, mc_delta
        );

        assert!(
            ffmpeg_delta <= 0.5,
            "FFMPEG normalization failed: expected -18.0 ±0.5, got {:.2}",
            ffmpeg_val
        );
    } else {
        println!(
            "SKIP: ffmpeg not found in PATH or parsing failed, skipping primary oracle check."
        );
        println!(
            "Oracle (in-process meter): measured={:.2} LUFS, target=-18.0 (delta: {:.3})",
            mc_lufs, mc_delta
        );
    }

    assert!(
        mc_delta <= 0.5,
        "In-process meter normalization failed: expected -18.0 ±0.5, got {:.2}",
        mc_lufs
    );
}
