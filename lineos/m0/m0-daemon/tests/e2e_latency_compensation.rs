use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

#[tokio::test]
async fn test_e2e_latency_compensation() {
    let wav_path = "../../m1/sp314-dsp/tests/fixtures/sine_1khz_3s.wav";

    // We expect the fixture to exist
    assert!(std::path::Path::new(wav_path).exists(), "Fixture missing!");

    let req = MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: "warm".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: Some("warm_analog".to_string()),
        tone: None,
        dynamics: None,
        chaos_seed: Some(42),
        project_id: Some("proj_latency".to_string()),
        track_id: Some("track_latency".to_string()),
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

    let (_blob, exported_pcm_path, _, _) = dsp_result.unwrap();

    // Read the exported PCM file (raw f32 LE interleaved)
    let file_bytes = std::fs::read(&exported_pcm_path).expect("Failed to read exported PCM file");
    let mut data = Vec::with_capacity(file_bytes.len() / 4);
    for chunk in file_bytes.chunks_exact(4) {
        data.push(f32::from_le_bytes(chunk.try_into().unwrap()));
    }

    // --- 1. Frame count guard ---
    let frames = data.len() / 2; // Stereo interleaved
                                 // Regression guard: catches if STFT_FLUSH_TAIL trim (commit 9039dac) is ever reverted or an upstream node's tail size changes without updating the trim.
    assert_eq!(
        frames, 144000,
        "Exported frame count must be EXACTLY the input frame count (144000) with no STFT tail."
    );

    // --- 2. Leading silence guard ---
    let zero_threshold = 1e-6;
    let mut leading_zero_count = 0;
    for i in 0..1024.min(frames) {
        let l = data[i * 2];
        let r = data[i * 2 + 1];
        if l.abs() < zero_threshold && r.abs() < zero_threshold {
            leading_zero_count += 1;
        } else {
            break; // Stop at the first non-zero sample - we count consecutive leading zeros
        }
    }
    assert!(
        leading_zero_count < 1000,
        "Leading silence guard failed: {} consecutive near-zero samples at start (expected well under 1024, which was the old STFT_FLUSH_TAIL bug's signature — sound should start within the first ~700 samples based on today's empirical Hanning ramp-up measurement).",
        leading_zero_count
    );
}
