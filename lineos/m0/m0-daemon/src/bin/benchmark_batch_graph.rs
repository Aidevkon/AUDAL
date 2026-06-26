use m0d::handlers::decode::decode_raw_interleaved;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.wav> <output.wav>", args[0]);
        std::process::exit(1);
    }
    let input_path = &args[1];
    let output_path = &args[2];

    println!("=== Batch Graph Benchmark ===");
    println!("Input: {}", input_path);

    let topology_json = serde_json::json!({
        "topology_id": "batch_benchmark",
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

    // 1. Decode entire file
    let (batch_samples, sample_rate, _) = decode_raw_interleaved(input_path).unwrap();
    let mut batch_left: Vec<f32> = batch_samples.iter().step_by(2).copied().collect();
    let mut batch_right: Vec<f32> = batch_samples.iter().skip(1).step_by(2).copied().collect();

    // 2. Process
    let mut batch_graph = DspGraph::from_topology(&topology, block_size, sample_rate).unwrap();
    let num_frames = batch_left.len();
    let mut f = 0;
    while f < num_frames {
        let e = (f + block_size).min(num_frames);
        let b_len = e - f;
        if b_len < block_size {
            let mut pad_l = vec![0.0_f32; block_size];
            let mut pad_r = vec![0.0_f32; block_size];
            pad_l[..b_len].copy_from_slice(&batch_left[f..e]);
            pad_r[..b_len].copy_from_slice(&batch_right[f..e]);
            batch_graph.process_block(&mut pad_l, &mut pad_r);
            batch_left[f..e].copy_from_slice(&pad_l[..b_len]);
            batch_right[f..e].copy_from_slice(&pad_r[..b_len]);
        } else {
            batch_graph.process_block(&mut batch_left[f..e], &mut batch_right[f..e]);
        }
        f += b_len;
    }

    // Include BeatDetector so the workload is truly identical to `benchmark_streaming`
    use m0d::dsp::beat_detector::BeatDetector;
    let mut mono: Vec<f32> = Vec::with_capacity(batch_left.len());
    for i in 0..batch_left.len() {
        mono.push((batch_left[i] + batch_right[i]) * 0.5);
    }
    let beat_detector = BeatDetector::new(sample_rate);
    let (bpm, _, _, transients) = beat_detector.analyze(&mono);
    println!("  BPM extracted: {:.2}", bpm);
    println!("  Transients found: {}", transients.len());

    // 3. Write output
    let mut writer =
        sp314_dsp::io::wav_writer::StreamingWavWriter::new(output_path, sample_rate).unwrap();
    // Writing chunk by chunk so writer overhead is similar (it processes internally anyway, but doing large chunk might blow up memory. StreamingWavWriter expects small chunks, but it can handle large ones by just buffering if we pass a huge slice. Wait, let's write chunk by chunk just to avoid artificially inflating RAM via the writer. Actually, `StreamingWavWriter` writes to `hound::WavWriter` sample by sample natively.)
    writer.write_chunk(&batch_left, &batch_right).unwrap();
    writer.finalize().unwrap();

    println!("Done.");
}
