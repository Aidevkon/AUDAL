use lineos_corpus::scout::{SegmentBoundary, SegmentType, TimelineRouter};
use serde_json::json;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

#[test]
fn test_pass2_dynamic_ducking() {
    // 1. Build a KNOWN synthetic Timeline Map
    let boundaries = vec![
        SegmentBoundary {
            start_sec: 0.0,
            end_sec: 2.0,
            segment_type: SegmentType::Speech,
            avg_leaning: 0.9,
            avg_confidence: 0.9,
        },
        SegmentBoundary {
            start_sec: 2.0,
            end_sec: 4.0,
            segment_type: SegmentType::Music,
            avg_leaning: 0.1,
            avg_confidence: 0.9,
        },
    ];
    let router = TimelineRouter::new(boundaries);

    // 2. Generate a constant-amplitude sine wave, 4 seconds, 48000 Hz
    let sample_rate = 48000;
    let duration_sec = 4.0;
    let total_frames = (duration_sec * sample_rate as f32) as usize;
    let mut input_left = vec![0.0f32; total_frames];
    let mut input_right = vec![0.0f32; total_frames];

    // 1 kHz sine
    for i in 0..total_frames {
        let t = i as f32 / sample_rate as f32;
        let sample = (t * 1000.0 * 2.0 * std::f32::consts::PI).sin() * 0.5; // -6dBFS peak approx
        input_left[i] = sample;
        input_right[i] = sample;
    }

    // 3. Build a minimal DspGraph containing ONE GainNode
    let topology_json = json!({
        "topology_id": "ducking_test",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "duck_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 300.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "duck_gain", "modulation_type": "audio" },
            { "source": "duck_gain", "target": "out", "modulation_type": "audio" }
        ]
    }).to_string();

    let topology = DspTopology::from_json(&topology_json).unwrap();
    let block_size = 512;
    let mut graph = DspGraph::from_topology(&topology, block_size, sample_rate).unwrap();

    // 4. Stream block-by-block
    // Using a minimal local loop instead of streaming_pipeline since we're generating memory-resident audio.
    let mut output_left = vec![0.0f32; total_frames];
    let mut output_right = vec![0.0f32; total_frames];

    let mut last_type: Option<SegmentType> = None;

    let mut f = 0;
    while f < total_frames {
        let e = (f + block_size).min(total_frames);
        let b_len = e - f;

        let mut bl = vec![0.0f32; block_size];
        let mut br = vec![0.0f32; block_size];
        bl[..b_len].copy_from_slice(&input_left[f..e]);
        br[..b_len].copy_from_slice(&input_right[f..e]);

        // COMPUTE BLOCK TIMESTAMP (time at start of block)
        let block_time_sec = f as f32 / sample_rate as f32;

        // TIMELINE ROUTING
        if let Some(seg_type) = router.get_segment_type_at(block_time_sec) {
            if Some(seg_type) != last_type {
                // only call set_node_parameter HERE, on actual change
                // This is the CORRECT pattern Pass 2's real orchestrator must use too.
                match seg_type {
                    SegmentType::Speech => {
                        graph.set_node_parameter("duck_gain", "gain", 1.0).unwrap();
                    }
                    SegmentType::Music => {
                        // ~-6dB ducking (linear amplitude 0.501)
                        graph
                            .set_node_parameter("duck_gain", "gain", 0.501)
                            .unwrap();
                    }
                }
                last_type = Some(seg_type);
            }
        }

        graph.process_block(&mut bl, &mut br);

        output_left[f..e].copy_from_slice(&bl[..b_len]);
        output_right[f..e].copy_from_slice(&br[..b_len]);

        f += b_len;
    }

    // 5. ASSERT the proof
    // Avoid the glide transitions.
    // Speech block: 0.5s to 1.5s (safe distance from 0s and 2s)
    // Music block: 2.5s to 3.5s (giving 500ms to settle after the 2.0s boundary + 300ms glide)

    let compute_rms = |start_sec: f32, end_sec: f32| -> f32 {
        let start_idx = (start_sec * sample_rate as f32) as usize;
        let end_idx = (end_sec * sample_rate as f32) as usize;
        let slice = &output_left[start_idx..end_idx];
        let sq_sum: f32 = slice.iter().map(|&x| x * x).sum();
        (sq_sum / slice.len() as f32).sqrt()
    };

    let speech_rms = compute_rms(0.5, 1.5);
    let music_rms = compute_rms(2.5, 3.5);

    let speech_db = 20.0 * speech_rms.log10();
    let music_db = 20.0 * music_rms.log10();

    println!("=== Pass 2 Ducking Proof ===");
    println!(
        "Speech RMS (unity): {:.4} ({:.2} dB)",
        speech_rms, speech_db
    );
    println!("Music RMS (ducked): {:.4} ({:.2} dB)", music_rms, music_db);

    let delta_db = music_db - speech_db;
    println!("Delta dB: {:.2}", delta_db);

    // Assert the difference is approximately -6dB
    assert!(
        (delta_db - (-6.0)).abs() < 1.0,
        "Ducking failed or didn't settle properly. Expected -6dB, got {:.2}dB",
        delta_db
    );
}
