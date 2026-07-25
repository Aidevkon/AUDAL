use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

#[tokio::test]
async fn test_e2e_alignment_oracle() {
    let wav_path = "/tmp/alignment_oracle.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
    let frames = 144000;

    // Generate a 3s/48k stereo wav: digital silence
    // with three single-sample impulses (1.0 on both channels) at
    // frames 0, 24000, and 143500.
    for i in 0..frames {
        let bg = (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48000.0).sin() * 0.1;
        if i == 0 || i == 24000 || i == 143500 {
            writer.write_sample(1.0f32).unwrap();
            writer.write_sample(1.0f32).unwrap();
        } else {
            writer.write_sample(bg).unwrap();
            writer.write_sample(bg).unwrap();
        }
    }
    writer.finalize().unwrap();

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
        restoration_enabled: None,
        macro_router_enabled: None,
    };

    let start = Instant::now();
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let dummy_head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
        let state_tmp = tempfile::TempDir::new().unwrap();

        run_dsp(
            &req,
            start,
            dummy_head_state,
            None,
            None,
            "".to_string(),
            state_tmp.path().to_str().unwrap(),
        )
    })
    .await;

    let dsp_result = result.unwrap();
    let (_blob, _, exported_pcm_path, _, _) = dsp_result.unwrap();

    let file_bytes =
        std::fs::read(exported_pcm_path.path()).expect("Failed to read exported PCM file");
    let mut data = Vec::with_capacity(file_bytes.len() / 4);
    for chunk in file_bytes.chunks_exact(4) {
        data.push(f32::from_le_bytes(chunk.try_into().unwrap()));
    }

    let out_frames = data.len() / 2;

    // One soft assert only: frames == 144000 (the count guard).
    assert_eq!(
        out_frames, 144000,
        "Exported frame count must be exactly 144000"
    );

    // F-052 alignment guard: impulse peaks must land within 64 frames
    // of their input position. DSP smear (Hanning overlap-add) shifts
    // peaks by ~31 frames empirically; 64 is the hard ceiling.
    let find_peak = |start: usize, end: usize, name: &str| -> usize {
        let mut max_val = 0.0_f32;
        let mut max_idx = 0;
        for i in start..end.min(out_frames) {
            let l = data[i * 2].abs();
            if l > max_val {
                max_val = l;
                max_idx = i;
            }
        }
        eprintln!(
            "IMPULSE {} region [{}..{}]: max at frame {}, value={}",
            name, start, end, max_idx, max_val
        );
        max_idx
    };

    let peak_0 = find_peak(0, 2048, "0");
    let peak_24k = find_peak(23000, 25000, "24000");
    let peak_143k = find_peak(143000, 144000, "143500");

    assert!(
        peak_0 < 64,
        "IMPULSE@0: peak landed at frame {} (expected 0..64)",
        peak_0
    );
    assert!(
        (24000..24064).contains(&peak_24k),
        "IMPULSE@24000: peak landed at frame {} (expected 24000..24064)",
        peak_24k
    );
    assert!(
        (143500..143564).contains(&peak_143k),
        "IMPULSE@143500: peak landed at frame {} (expected 143500..143564)",
        peak_143k
    );
}
