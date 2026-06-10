use sp314_nodes::topology::{DspTopology, TopologyEdge, TopologyNode};

pub struct Merger;

impl Merger {
    pub fn merge(flavors: Vec<DspTopology>) -> DspTopology {
        if flavors.is_empty() {
            return DspTopology {
                topology_id: "empty".to_string(),
                nodes: vec![],
                edges: vec![],
            };
        }

        let num_flavors = flavors.len();
        let mut all_nodes: Vec<TopologyNode> = Vec::new();
        let mut all_edges: Vec<TopologyEdge> = Vec::new();

        // 1. Prefix all nodes and edges
        for (i, mut topo) in flavors.into_iter().enumerate() {
            let prefix = format!("f{}_", i);

            for node in &mut topo.nodes {
                node.node_id = format!("{}{}", prefix, node.node_id);
            }

            for edge in &mut topo.edges {
                edge.source = format!("{}{}", prefix, edge.source);
                edge.target = format!("{}{}", prefix, edge.target);
            }

            all_nodes.extend(topo.nodes);
            all_edges.extend(topo.edges);
        }

        // 2. Rewire between flavors
        for i in 0..(num_flavors.saturating_sub(1)) {
            let f_n_output = format!("f{}_Output", i);
            let f_next_input = format!("f{}_Input", i + 1);

            // Step 1: Find all audio edges targeting fN_Output -> collect source node IDs
            let mut output_sources = Vec::new();
            for edge in &all_edges {
                if edge.modulation_type == "audio" && edge.target == f_n_output {
                    output_sources.push(edge.source.clone());
                }
            }

            // Step 2: Find all audio edges sourcing from f(N+1)_Input -> collect target node IDs
            let mut input_targets = Vec::new();
            for edge in &all_edges {
                if edge.modulation_type == "audio" && edge.source == f_next_input {
                    input_targets.push(edge.target.clone());
                }
            }

            // Step 3: Create new direct audio edges
            for src in &output_sources {
                for tgt in &input_targets {
                    all_edges.push(TopologyEdge {
                        source: src.clone(),
                        target: tgt.clone(),
                        modulation_type: "audio".to_string(),
                        source_output: None,
                        target_parameter: None,
                    });
                }
            }

            // Step 4: Remove the fN_Output and f(N+1)_Input nodes from topology
            all_nodes.retain(|n| n.node_id != f_n_output && n.node_id != f_next_input);

            // Step 5: Remove the original edges that connected to these intermediate nodes
            all_edges.retain(|e| {
                if e.modulation_type == "audio" {
                    e.target != f_n_output && e.source != f_next_input
                } else {
                    true // Parameter edges are never touched
                }
            });
        }

        // Only f0_Input and f{last}_Output survive as true entry/exit markers
        DspTopology {
            topology_id: "merged_dag".to_string(),
            nodes: all_nodes,
            edges: all_edges,
        }
    }
}
