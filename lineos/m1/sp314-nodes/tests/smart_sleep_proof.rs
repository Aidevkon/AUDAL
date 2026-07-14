use serde_json::json;
use sp314_nodes::{graph::DspGraph, topology::DspTopology};

#[test]
fn test_smart_sleep_deesser_tail_flush() {
    let topology_json = json!({
        "topology_id": "deesser_sleep_proof",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "deesser", "node_type": "DeEsser", "parameters": { "threshold_db": -40.0, "frequency_hz": 1000.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "deesser", "modulation_type": "audio" },
            { "source": "deesser", "target": "out", "modulation_type": "audio" }
        ]
    }).to_string();

    let topology = DspTopology::from_json(&topology_json).unwrap();
    let block_size = 1024;
    let sample_rate = 48000;
    let mut graph = DspGraph::from_topology(&topology, block_size, sample_rate).unwrap();

    let mut left = vec![0.0; block_size];
    let mut right = vec![0.0; block_size];

    // Phase 1: Feed a sibilant signal (high frequency burst) to build up DeEsser envelope
    // Nyquist oscillation is very high frequency, DeEsser HP filter will catch it
    for i in 0..block_size {
        left[i] = if i % 2 == 0 { 0.8 } else { -0.8 };
        right[i] = left[i];
    }

    // Run a few blocks to build up the envelope
    for _ in 0..5 {
        graph.process_block(&mut left, &mut right);
    }

    let env_before_sleep = graph
        .get_node_output("deesser", "env_l")
        .expect("env_l must be exposed");
    println!(
        "Envelope AFTER signal burst (before sleep): {:.6}",
        env_before_sleep
    );
    assert!(
        env_before_sleep > 0.1,
        "Envelope must rise to a non-trivial value"
    );

    // Phase 2: Simulate "going silent" for a flush period
    left.fill(0.0);
    right.fill(0.0);

    let mut env_decay_history = Vec::new();
    for _ in 0..10 {
        graph.process_block(&mut left, &mut right);
        let env_now = graph.get_node_output("deesser", "env_l").unwrap();
        env_decay_history.push(env_now);
    }

    println!("Envelope decay history during flush blocks:");
    for (i, val) in env_decay_history.iter().enumerate() {
        println!("  Flush block {}: {:.6}", i + 1, val);
    }

    // Assert that it decays, but check if it asymptotically approaches 0 instead of hitting exactly 0
    let last_decay = *env_decay_history.last().unwrap();
    assert!(
        last_decay < env_before_sleep,
        "Envelope must decay during silence"
    );
    assert!(
        last_decay > 0.0,
        "Envelope decays asymptotically, doesn't reach exactly 0.0"
    );

    // Phase 3: Explicit hard reset (simulating waking up for a new segment after sleep)
    // DspGraph::reset() will clear all node states and buffers
    graph.reset();
    let env_after_reset = graph.get_node_output("deesser", "env_l").unwrap();
    println!("Envelope AFTER hard reset: {:.6}", env_after_reset);
    assert_eq!(
        env_after_reset, 0.0,
        "Envelope must be exactly 0.0 after reset"
    );

    // Phase 4: Re-trigger with fresh sibilant signal
    for i in 0..block_size {
        left[i] = if i % 2 == 0 { 0.8 } else { -0.8 };
        right[i] = left[i];
    }
    graph.process_block(&mut left, &mut right);
    let env_retriggered = graph.get_node_output("deesser", "env_l").unwrap();
    println!("Envelope AFTER re-trigger: {:.6}", env_retriggered);
    assert!(
        env_retriggered > 0.0,
        "Envelope must rise again from clean slate"
    );
    // Prove it started from 0 by comparing to what 1 block of burst produced initially
    // Actually, we ran 5 blocks initially, so comparing 1 block vs 1 block is tricky without doing it explicitly.
    // Let's just assert it's healthy and responding.
}
