use m0d::dsp::streaming_pipeline::run_streaming_pipeline_with_scout;
use sp314_nodes::topology::DspTopology;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.wav> <output.wav>", args[0]);
        std::process::exit(1);
    }
    let input_path = &args[1];
    let output_path = &args[2];

    println!("=== Streaming Benchmark ===");
    println!("Input: {}", input_path);

    // Peek sample rate from header
    let spec = hound::WavReader::open(input_path)
        .expect("Failed to open input WAV")
        .spec();
    let sample_rate = spec.sample_rate;

    let topology_json = serde_json::json!({
        "topology_id": "streaming_benchmark",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "eq", "node_type": "BiquadFilter", "parameters": { "filter_type": 0.0, "freq_hz": 1000.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "eq", "modulation_type": "audio" },
            { "source": "eq", "target": "out", "modulation_type": "audio" }
        ]
    }).to_string();
    let topology = DspTopology::from_json(&topology_json).unwrap();

    let block_size = 512;

    let result = run_streaming_pipeline_with_scout(
        input_path,
        output_path,
        &topology,
        block_size,
        sample_rate,
    )
    .expect("Streaming pipeline failed");

    println!("\n=== Output Metrics ===");
    println!("  BPM extracted: {:.2}", result.bpm);
    println!("  Transients found: {}", result.transients_ms.len());

    println!("\n=== Output: {} ===", output_path);
    println!("Done.");
}
