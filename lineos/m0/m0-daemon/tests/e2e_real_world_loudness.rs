use approx::assert_abs_diff_eq;
use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use sp314_dsp::metering::measure_integrated_lufs;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

/// REAL-WORLD LUFS REGRESSION GUARD
///
/// This test complements the manual EBU oracle verification (which relies on
/// uncommitted, strict sine-wave EBU test signals due to licensing restrictions)
/// by running a committed, licensing-clean, real-world music track ("simban")
/// automatically in every CI run.
///
/// It guarantees that the production `m0-daemon` pipeline correctly hits the
/// target LUFS for the "apple_music" preset (-16.0 LUFS) within a real-world
/// dynamic tolerance (±1.0 LU), ensuring no silent fallback bugs or gain-staging
/// regressions occur in the DSP engine.
#[tokio::test]
async fn test_e2e_real_world_loudness() {
    let wav_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";

    // We expect the real-world fixture to exist
    assert!(std::path::Path::new(wav_path).exists(), "Fixture missing!");

    let req = MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: "apple_music".to_string(), // Target: -16.0 LUFS
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: Some("warm_analog".to_string()),
        tone: None,
        dynamics: None,
        chaos_seed: Some(42),
        project_id: Some("proj_loudness".to_string()),
        track_id: Some("track_loudness".to_string()),
        mix_levels: None,
        preview_id: None,
    };

    let start = Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
        run_dsp(&req, start, dummy_head_state, None, None, "".to_string())
    })
    .await;

    // Must not timeout
    assert!(result.is_ok(), "Test timed out after 60s");

    let dsp_result = result.unwrap();
    // Must succeed
    assert!(dsp_result.is_ok(), "run_dsp failed: {:?}", dsp_result.err());

    let (_blob, _, exported_pcm_path, _) = dsp_result.unwrap();

    // Read the exported PCM file (raw f32 LE interleaved)
    let file_bytes = std::fs::read(&exported_pcm_path).expect("Failed to read exported PCM file");
    let mut left = Vec::new();
    let mut right = Vec::new();

    // De-interleave the f32 samples (each f32 is 4 bytes, so a stereo frame is 8 bytes)
    for chunk in file_bytes.chunks_exact(8) {
        let l_bytes: [u8; 4] = chunk[0..4].try_into().unwrap();
        let r_bytes: [u8; 4] = chunk[4..8].try_into().unwrap();
        left.push(f32::from_le_bytes(l_bytes));
        right.push(f32::from_le_bytes(r_bytes));
    }

    // Measure LUFS of the processed output
    let measured_lufs = measure_integrated_lufs(&left, &right);

    // Assert it hits the target -16.0 LUFS within a real-world tolerance of ±1.0 LU
    assert_abs_diff_eq!(measured_lufs, -16.0, epsilon = 1.0);
}
