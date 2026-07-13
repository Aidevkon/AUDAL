use serde_json::json;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

#[test]
fn test_dual_graph_vca_proof() {
    let sample_rate = 48000;
    let block_size = 1024;
    let total_seconds = 4;
    let total_frames = sample_rate * total_seconds;

    // 1. Build distinct synthetic signals
    // vocal_signal: 1kHz sine
    // music_signal: 200Hz sine
    let mut vocal_signal = vec![0.0f32; total_frames as usize];
    let mut music_signal = vec![0.0f32; total_frames as usize];

    for i in 0..total_frames as usize {
        let t = i as f32 / sample_rate as f32;
        vocal_signal[i] = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        music_signal[i] = (2.0 * std::f32::consts::PI * 200.0 * t).sin() * 0.5;
    }

    // 2. Build two independent DspGraphs from the same JSON
    let topology_json = json!({
        "topology_id": "vca_bus_topology",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "vca_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 10.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "vca_gain", "modulation_type": "audio" },
            { "source": "vca_gain", "target": "out", "modulation_type": "audio" }
        ]
    });

    let topology = serde_json::from_value::<DspTopology>(topology_json).unwrap();
    let mut vocal_graph =
        DspGraph::from_topology(&topology, block_size, sample_rate as u32).unwrap();
    let mut music_graph =
        DspGraph::from_topology(&topology, block_size, sample_rate as u32).unwrap();

    let mut mixed_l = vec![0.0f32; total_frames as usize];
    let mut mixed_r = vec![0.0f32; total_frames as usize];

    // Segment tracking
    let mut current_segment = 0; // 0 for Speech, 1 for Music

    // Initial State (Speech Segment: 0-2s)
    vocal_graph
        .set_node_parameter("vca_gain", "gain", 1.0)
        .unwrap();
    music_graph
        .set_node_parameter("vca_gain", "gain", 0.0)
        .unwrap();

    let mut frame_idx = 0;
    while frame_idx < total_frames as usize {
        let end = (frame_idx + block_size).min(total_frames as usize);
        let mut v_bl = vocal_signal[frame_idx..end].to_vec();
        let mut v_br = vocal_signal[frame_idx..end].to_vec();

        let mut m_bl = music_signal[frame_idx..end].to_vec();
        let mut m_br = music_signal[frame_idx..end].to_vec();

        let t_sec = frame_idx as f32 / sample_rate as f32;

        // Simulate segment transition at 2.0s
        if t_sec >= 2.0 && current_segment == 0 {
            current_segment = 1;
            // Music Segment (2-4s)
            vocal_graph
                .set_node_parameter("vca_gain", "gain", 0.0)
                .unwrap();
            music_graph
                .set_node_parameter("vca_gain", "gain", 1.0)
                .unwrap();
            println!("Transition to Music Segment at {}s", t_sec);
        }

        if end == total_frames as usize {
            #[cfg(debug_assertions)]
            {
                vocal_graph.debug_sq_l.clear();
                music_graph.debug_sq_l.clear();
                vocal_graph.debug_frames = 0;
                music_graph.debug_frames = 0;
            }
        }

        vocal_graph.process_block(&mut v_bl, &mut v_br);
        music_graph.process_block(&mut m_bl, &mut m_br);

        // Sum outputs
        for i in 0..(end - frame_idx) {
            mixed_l[frame_idx + i] = v_bl[i] + m_bl[i];
            mixed_r[frame_idx + i] = v_br[i] + m_br[i];
        }

        frame_idx += block_size;
    }

    // 5. Assert the proof

    // Region 1: 0.5 - 1.5s (Speech Segment)
    let start_1 = (0.5 * sample_rate as f32) as usize;
    let end_1 = (1.5 * sample_rate as f32) as usize;
    let mut mse_vocal_1 = 0.0;
    let mut mse_music_1 = 0.0;

    for i in start_1..end_1 {
        let diff_vocal = mixed_l[i] - vocal_signal[i];
        mse_vocal_1 += diff_vocal * diff_vocal;
        let diff_music = mixed_l[i] - music_signal[i];
        mse_music_1 += diff_music * diff_music;
    }
    mse_vocal_1 /= (end_1 - start_1) as f32;
    mse_music_1 /= (end_1 - start_1) as f32;

    println!("Region 0.5-1.5s (Vocal Dominant):");
    println!("  MSE vs Vocal Signal: {:.6}", mse_vocal_1);
    println!("  MSE vs Music Signal: {:.6}", mse_music_1);

    // Assert vocal is dominant
    assert!(
        mse_vocal_1 < 1e-4,
        "Expected mixed output to match vocal signal"
    );
    assert!(
        mse_music_1 > 0.1,
        "Expected mixed output to differ significantly from music signal"
    );

    // Region 2: 2.5 - 3.5s (Music Segment)
    let start_2 = (2.5 * sample_rate as f32) as usize;
    let end_2 = (3.5 * sample_rate as f32) as usize;
    let mut mse_vocal_2 = 0.0;
    let mut mse_music_2 = 0.0;

    for i in start_2..end_2 {
        let diff_vocal = mixed_l[i] - vocal_signal[i];
        mse_vocal_2 += diff_vocal * diff_vocal;
        let diff_music = mixed_l[i] - music_signal[i];
        mse_music_2 += diff_music * diff_music;
    }
    mse_vocal_2 /= (end_2 - start_2) as f32;
    mse_music_2 /= (end_2 - start_2) as f32;

    println!("Region 2.5-3.5s (Music Dominant):");
    println!("  MSE vs Vocal Signal: {:.6}", mse_vocal_2);
    println!("  MSE vs Music Signal: {:.6}", mse_music_2);

    // Assert music is dominant
    assert!(
        mse_music_2 < 1e-4,
        "Expected mixed output to match music signal"
    );
    assert!(
        mse_vocal_2 > 0.1,
        "Expected mixed output to differ significantly from vocal signal"
    );

    // Assert internal state isolation
    #[cfg(debug_assertions)]
    {
        // Prove that the final debug_sq_l values reflect orthogonal states
        let v_sum_sq = vocal_graph
            .debug_sq_l
            .get("vca_gain")
            .copied()
            .unwrap_or(0.0);
        let m_sum_sq = music_graph
            .debug_sq_l
            .get("vca_gain")
            .copied()
            .unwrap_or(0.0);
        println!("Final Graph State (Node Output Sum-of-Squares):");
        println!("  vocal_graph vca_gain sum-of-squares: {:.6}", v_sum_sq);
        println!("  music_graph vca_gain sum-of-squares: {:.6}", m_sum_sq);

        // vocal_graph should be muted at the end (output ~ 0)
        assert!(v_sum_sq < 1e-6, "vocal_graph gain should be 0.0 at the end");
        // music_graph should be active at the end (output > 0)
        assert!(m_sum_sq > 0.01, "music_graph gain should be 1.0 at the end");
    }
}
