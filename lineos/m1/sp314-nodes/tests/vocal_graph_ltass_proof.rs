// allow: contract tests index synthetic buffers the same way the
// DSP code under test does; iterator rewrites add nothing here.
#![allow(clippy::needless_range_loop)]

use aether_bridge::reference_resolver::{ProfileId, ReferenceProfile, ReferenceResolver};
use serde_json::json;
use sp314_nodes::{graph::DspGraph, topology::DspTopology};

#[test]
fn test_vocal_graph_ltass_correction() {
    let ref_cfs = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];

    // 1. Build the topology JSON (using Rust builder for clarity and self-containment)
    let topology_json = json!({
        "topology_id": "vocal_graph_proof",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            // DeEsser with 0.0 threshold to act as passthrough for this EQ measurement proof
            { "node_id": "deesser", "node_type": "DeEsser", "parameters": { "threshold_db": 0.0, "frequency_hz": 6000.0 } },
            // 8 LTASS Biquad bands (all initialized to 0dB gain, filter_type=3.0 peaking, q=0.707)
            { "node_id": "ltass_band_0", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[0], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_1", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[1], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_2", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[2], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_3", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[3], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_4", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[4], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_5", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[5], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_6", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[6], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_7", "node_type": "BiquadFilter", "parameters": { "freq_hz": ref_cfs[7], "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "deesser", "modulation_type": "audio" },
            { "source": "deesser", "target": "ltass_band_0", "modulation_type": "audio" },
            { "source": "ltass_band_0", "target": "ltass_band_1", "modulation_type": "audio" },
            { "source": "ltass_band_1", "target": "ltass_band_2", "modulation_type": "audio" },
            { "source": "ltass_band_2", "target": "ltass_band_3", "modulation_type": "audio" },
            { "source": "ltass_band_3", "target": "ltass_band_4", "modulation_type": "audio" },
            { "source": "ltass_band_4", "target": "ltass_band_5", "modulation_type": "audio" },
            { "source": "ltass_band_5", "target": "ltass_band_6", "modulation_type": "audio" },
            { "source": "ltass_band_6", "target": "ltass_band_7", "modulation_type": "audio" },
            { "source": "ltass_band_7", "target": "out", "modulation_type": "audio" }
        ]
    }).to_string();

    let topology = DspTopology::from_json(&topology_json).unwrap();
    let sample_rate = 48000;
    let block_size = 1024;
    let mut graph = DspGraph::from_topology(&topology, block_size, sample_rate).unwrap();

    // 2. Compute REAL LTASS gains via ReferenceResolver
    let profile = ReferenceProfile::load(ProfileId::PodcastV1);

    // We construct a synthetic profile where most bands perfectly match the target,
    // but Band 3 (750 Hz) is deliberately 6 dB too quiet, which should produce a +6 dB boost request.
    let mut raw_profile = profile.spectral_target;
    // Move all bands to an absolute level (e.g. around -20 dBFS)
    for v in raw_profile.iter_mut() {
        *v += -20.0;
    }

    // Deliberately drop Band 3 by 6 dB to force a correction.
    // We don't raise another band, so normalization will shift the mean slightly,
    // resulting in a smaller correction distributed across all bands,
    // but Band 3 will still be the dominant positive correction.
    raw_profile[3] -= 6.0;

    // Apply the exact normalization logic used in build_dsp_config
    let n = profile.normalization_band_count;
    let speech_mean: f32 = raw_profile[..n].iter().sum::<f32>() / n as f32;
    let normalized_profile: [f32; 8] = std::array::from_fn(|k| raw_profile[k] - speech_mean);

    let ref_gains = ReferenceResolver::resolve(&normalized_profile, &profile);

    println!("Target LTASS Shape: {:?}", profile.spectral_target);
    println!("Raw Input Profile:  {:?}", raw_profile);
    println!("Normalized Profile: {:?}", normalized_profile);
    println!("Computed Gains:     {:?}", ref_gains);

    // Assert that Band 3 asks for a boost
    assert!(
        ref_gains[3] > 4.0,
        "Expected significant boost on Band 3, got {}",
        ref_gains[3]
    );

    // 3. Apply the 8 gains to the graph nodes
    for i in 0..8 {
        let node_id = format!("ltass_band_{}", i);
        graph
            .set_node_parameter_no_glide(&node_id, "gain_db", ref_gains[i])
            .unwrap_or_else(|_| panic!("Failed to set gain on {}", node_id));
    }

    // 4. Test signal verification
    // Generate a 750Hz sine wave (Band 3 center frequency)
    // Run it unprocessed to measure base RMS, then run through graph to see the boost.

    let mut base_l = vec![0.0; block_size];
    let mut base_r = vec![0.0; block_size];
    let freq = 750.0;
    let mut phase: f32 = 0.0;
    let phase_inc = 2.0 * std::f32::consts::PI * freq / (sample_rate as f32);

    for i in 0..block_size {
        base_l[i] = phase.sin() * 0.1; // -20 dBFS approx
        base_r[i] = base_l[i];
        phase += phase_inc;
        if phase >= 2.0 * std::f32::consts::PI {
            phase -= 2.0 * std::f32::consts::PI;
        }
    }

    // Measure base energy (RMS)
    let base_rms = (base_l.iter().map(|v| v * v).sum::<f32>() / (block_size as f32)).sqrt();

    // Process the exact same signal through the corrected graph
    let mut test_l = base_l.clone();
    let mut test_r = base_r.clone();

    // Process a few blocks to settle the biquad states and group delay
    for _ in 0..5 {
        for i in 0..block_size {
            test_l[i] = phase.sin() * 0.1;
            test_r[i] = test_l[i];
            phase += phase_inc;
            if phase >= 2.0 * std::f32::consts::PI {
                phase -= 2.0 * std::f32::consts::PI;
            }
        }
        graph.process_block(&mut test_l, &mut test_r);
    }

    // Now measure output RMS
    let out_rms = (test_l.iter().map(|v| v * v).sum::<f32>() / (block_size as f32)).sqrt();

    let db_change = 20.0 * (out_rms / base_rms).log10();

    println!("Base RMS: {:.6}", base_rms);
    println!("Output RMS: {:.6}", out_rms);
    println!("Measured DB Change at 750 Hz: {:.2} dB", db_change);
    println!(
        "Expected DB Change at 750 Hz (from gains): {:.2} dB",
        ref_gains[3]
    );

    // The measured dB change at exactly 750Hz should be very close to the peak gain of the filter,
    // though slightly offset by the broad Q=0.707 of adjacent bands which are also active.
    assert!(
        (db_change - ref_gains[3]).abs() < 1.5,
        "Measured gain {:.2} does not match expected correction {:.2}",
        db_change,
        ref_gains[3]
    );
}
