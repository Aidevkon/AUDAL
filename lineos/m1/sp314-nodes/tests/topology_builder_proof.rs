use serde_json::json;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::{DspTopology, DspTopologyBuilder};

#[test]
fn builder_produces_identical_vocal_graph() {
    let json_topology = json!({
        "topology_id": "vocal_graph_topology",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "deesser", "node_type": "DeEsser", "parameters": { "threshold_db": 0.0, "frequency_hz": 6000.0 } },
            { "node_id": "ltass_band_0", "node_type": "BiquadFilter", "parameters": { "freq_hz": 50.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_1", "node_type": "BiquadFilter", "parameters": { "freq_hz": 150.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_2", "node_type": "BiquadFilter", "parameters": { "freq_hz": 350.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_3", "node_type": "BiquadFilter", "parameters": { "freq_hz": 750.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_4", "node_type": "BiquadFilter", "parameters": { "freq_hz": 1500.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_5", "node_type": "BiquadFilter", "parameters": { "freq_hz": 3000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_6", "node_type": "BiquadFilter", "parameters": { "freq_hz": 6000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "ltass_band_7", "node_type": "BiquadFilter", "parameters": { "freq_hz": 12000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 } },
            { "node_id": "vca_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 10.0 } },
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
            { "source": "ltass_band_7", "target": "vca_gain", "modulation_type": "audio" },
            { "source": "vca_gain", "target": "out", "modulation_type": "audio" }
        ]
    });
    let t_json = DspTopology::from_json(&json_topology.to_string()).unwrap();

    let mut b = DspTopologyBuilder::new("vocal_graph_topology");
    let n_in = b.add_node("in", "Input", json!({}));
    let n_deesser = b.add_node(
        "deesser",
        "DeEsser",
        json!({ "threshold_db": 0.0, "frequency_hz": 6000.0 }),
    );
    let n_eq0 = b.add_node(
        "ltass_band_0",
        "BiquadFilter",
        json!({ "freq_hz": 50.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq1 = b.add_node(
        "ltass_band_1",
        "BiquadFilter",
        json!({ "freq_hz": 150.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq2 = b.add_node(
        "ltass_band_2",
        "BiquadFilter",
        json!({ "freq_hz": 350.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq3 = b.add_node(
        "ltass_band_3",
        "BiquadFilter",
        json!({ "freq_hz": 750.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq4 = b.add_node(
        "ltass_band_4",
        "BiquadFilter",
        json!({ "freq_hz": 1500.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq5 = b.add_node(
        "ltass_band_5",
        "BiquadFilter",
        json!({ "freq_hz": 3000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq6 = b.add_node(
        "ltass_band_6",
        "BiquadFilter",
        json!({ "freq_hz": 6000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_eq7 = b.add_node(
        "ltass_band_7",
        "BiquadFilter",
        json!({ "freq_hz": 12000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let n_gain = b.add_node("vca_gain", "Gain", json!({ "gain": 1.0, "glide_ms": 10.0 }));
    let n_out = b.add_node("out", "Output", json!({}));

    b.connect(&n_in, &n_deesser);
    b.connect(&n_deesser, &n_eq0);
    b.connect(&n_eq0, &n_eq1);
    b.connect(&n_eq1, &n_eq2);
    b.connect(&n_eq2, &n_eq3);
    b.connect(&n_eq3, &n_eq4);
    b.connect(&n_eq4, &n_eq5);
    b.connect(&n_eq5, &n_eq6);
    b.connect(&n_eq6, &n_eq7);
    b.connect(&n_eq7, &n_gain);
    b.connect(&n_gain, &n_out);

    let t_builder = b.build();

    let mut graph_json = DspGraph::from_topology(&t_json, 512, 48000).unwrap();
    let mut graph_builder = DspGraph::from_topology(&t_builder, 512, 48000).unwrap();

    let input_l = vec![0.5; 512];
    let input_r = vec![-0.5; 512];

    let mut out_json_l = input_l.clone();
    let mut out_json_r = input_r.clone();
    graph_json.process_block(&mut out_json_l, &mut out_json_r);

    let mut out_builder_l = input_l.clone();
    let mut out_builder_r = input_r.clone();
    graph_builder.process_block(&mut out_builder_l, &mut out_builder_r);

    // Verify bit-exact equality
    for i in 0..512 {
        assert_eq!(out_json_l[i], out_builder_l[i]);
        assert_eq!(out_json_r[i], out_builder_r[i]);
    }

    println!("Bit-identical match confirmed for 512 frames.");
}
