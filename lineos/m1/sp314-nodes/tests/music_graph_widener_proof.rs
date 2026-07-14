use sp314_nodes::graph::DspGraph;

#[test]
fn test_music_graph_widener_proof() {
    let sample_rate = 48000;

    // Create topology: Input -> GainNode -> WidthNode -> Output
    let json = r#"
    {
        "topology_id": "widener_proof",
        "nodes": [
            { "node_id": "input", "node_type": "Input", "parameters": {} },
            { "node_id": "gain", "node_type": "Gain", "parameters": { "gain": 1.0 } },
            { "node_id": "widener", "node_type": "Width", "parameters": { 
                "side_gain_db": 0.0, 
                "decorrelation": 0.0, 
                "mono_comp_shelf_db": 0.0 
            } },
            { "node_id": "output", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "input", "target": "gain" },
            { "source": "gain", "target": "widener" },
            { "source": "widener", "target": "output" }
        ]
    }
    "#;

    use sp314_nodes::topology::DspTopology;
    let topology: DspTopology = serde_json::from_str(json).expect("Invalid JSON");
    let mut graph =
        DspGraph::from_topology(&topology, 1024, sample_rate).expect("Failed to build graph");

    // Generate test signal: A mix of 60Hz and 1000Hz sine waves.
    // We'll generate it as fully mono (correlated) first, but to prove widening,
    // the widener actually requires SOME side content to multiply (if side_gain_db is used)
    // OR decorrelation to create side content from mono.
    // The instructions said "fully mono/correlated to start" and "applies a widening side_gain_db".
    // Wait: if it's fully mono, L-R is exactly 0.
    // Multiplying 0 by +6dB is still 0.
    // So to see side_gain_db work, we need a signal that is mostly mono but has *some* side energy,
    // OR we just use decorrelation to generate it.
    // Let's create a signal with 1.0 at L and 0.8 at R (slightly panned) for both 60Hz and 1000Hz.
    // This gives it a small natural side component.
    let num_samples = 48000; // 1 second
    let mut input_l = vec![0.0f32; num_samples];
    let mut input_r = vec![0.0f32; num_samples];

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sine_60 = (2.0 * std::f32::consts::PI * 60.0 * t).sin();
        let sine_1000 = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();

        let signal = sine_60 + sine_1000;

        // Panned slightly to give a non-zero side signal
        input_l[i] = signal * 1.0;
        input_r[i] = signal * 0.8;
    }

    // 1. Measure baseline RMS
    let mut bl_l = input_l.clone();
    let mut bl_r = input_r.clone();
    for i in (0..num_samples).step_by(1024) {
        let end = (i + 1024).min(num_samples);
        graph.process_block(&mut bl_l[i..end], &mut bl_r[i..end]);
    }

    // Calculate side energy (L-R)
    let baseline_side_rms = (bl_l
        .iter()
        .zip(bl_r.iter())
        .map(|(l, r)| (l - r).powi(2))
        .sum::<f32>()
        / num_samples as f32)
        .sqrt();

    println!("BASELINE:");
    println!("  Side RMS (L-R): {:.6}", baseline_side_rms);

    // 2. Widen and protect bass
    graph
        .set_node_parameter_no_glide("widener", "side_gain_db", 6.02)
        .unwrap();
    // mono_comp_shelf_db at -144dB means cut the side channel below ~200Hz
    graph
        .set_node_parameter_no_glide("widener", "mono_comp_shelf_db", -144.0)
        .unwrap();

    let mut test_l = input_l.clone();
    let mut test_r = input_r.clone();
    for i in (0..num_samples).step_by(1024) {
        let end = (i + 1024).min(num_samples);
        graph.process_block(&mut test_l[i..end], &mut test_r[i..end]);
    }

    let widened_side_rms = (test_l
        .iter()
        .zip(test_r.iter())
        .map(|(l, r)| (l - r).powi(2))
        .sum::<f32>()
        / num_samples as f32)
        .sqrt();

    println!("\nWIDENED (+6.02dB Side Gain, -144dB Mono Comp Shelf):");
    println!("  Side RMS (L-R): {:.6}", widened_side_rms);

    println!("\nIsolating Frequency Responses...");

    // 60 Hz ONLY
    let mut bass_l = vec![0.0f32; num_samples];
    let mut bass_r = vec![0.0f32; num_samples];
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sine_60 = (2.0 * std::f32::consts::PI * 60.0 * t).sin();
        bass_l[i] = sine_60 * 1.0;
        bass_r[i] = sine_60 * 0.8;
    }
    for i in (0..num_samples).step_by(1024) {
        let end = (i + 1024).min(num_samples);
        graph.process_block(&mut bass_l[i..end], &mut bass_r[i..end]);
    }
    let bass_side_rms = (bass_l
        .iter()
        .zip(bass_r.iter())
        .map(|(l, r)| (l - r).powi(2))
        .sum::<f32>()
        / num_samples as f32)
        .sqrt();

    // 1000 Hz ONLY
    let mut treble_l = vec![0.0f32; num_samples];
    let mut treble_r = vec![0.0f32; num_samples];
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sine_1000 = (2.0 * std::f32::consts::PI * 1000.0 * t).sin();
        treble_l[i] = sine_1000 * 1.0;
        treble_r[i] = sine_1000 * 0.8;
    }
    for i in (0..num_samples).step_by(1024) {
        let end = (i + 1024).min(num_samples);
        graph.process_block(&mut treble_l[i..end], &mut treble_r[i..end]);
    }
    let treble_side_rms = (treble_l
        .iter()
        .zip(treble_r.iter())
        .map(|(l, r)| (l - r).powi(2))
        .sum::<f32>()
        / num_samples as f32)
        .sqrt();

    // Baseline inputs for reference
    let raw_bass_side = (1.0f32 - 0.8f32) / 2.0_f32.sqrt();
    let raw_treble_side = (1.0f32 - 0.8f32) / 2.0_f32.sqrt();

    println!("  Raw 60Hz Input Side RMS: {:.6}", raw_bass_side);
    println!("  Processed 60Hz Side RMS: {:.6}", bass_side_rms);
    println!("  Raw 1000Hz Input Side RMS: {:.6}", raw_treble_side);
    println!("  Processed 1000Hz Side RMS: {:.6}", treble_side_rms);

    assert!(
        widened_side_rms > baseline_side_rms,
        "Overall side RMS did not increase"
    );
    assert!(
        treble_side_rms > raw_treble_side * 1.5,
        "Treble was not widened"
    );
    assert!(
        bass_side_rms < raw_bass_side * 0.7,
        "Bass was not narrowed/centered"
    );
}
