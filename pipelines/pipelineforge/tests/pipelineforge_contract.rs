use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

fn default_conditions() -> ConditionSet {
    ConditionSet {
        conditions: vec![],
        sample_rate: 48000,
        target_lufs: -14.0,
    }
}

#[test]
fn muddy_mix_selects_lowmid_clarity() {
    let mut conditions = default_conditions();
    conditions.conditions.push(EngineerCondition::MuddyMix);
    conditions.conditions.push(EngineerCondition::PumpDrift);

    let flavors = Pipelineforge::selected_flavors(&conditions);
    assert!(flavors.contains(&"LowMidClarity"));
    
    let eq_idx = flavors.iter().position(|&f| f == "LowMidClarity").unwrap();
    let comp_idx = flavors.iter().position(|&f| f == "AntiPumpStabilization").unwrap();
    assert!(eq_idx < comp_idx, "EQ flavor must appear before Compression flavor");
}

#[test]
fn muddy_and_pump_produces_valid_dag() {
    let mut conditions = default_conditions();
    conditions.conditions.push(EngineerCondition::MuddyMix);
    conditions.conditions.push(EngineerCondition::PumpDrift);

    let json_str = Pipelineforge::forge(&conditions).expect("Should forge DAG");
    println!("=== EXAMPLE OUTPUT ===");
    println!("{}", json_str);
    println!("======================");
    let topology = DspTopology::from_json(&json_str).expect("Valid JSON topology");

    let graph = DspGraph::from_topology(&topology, 512, 48000);
    assert!(graph.is_ok(), "Graph should compile without cycles or missing edges");
}

#[test]
fn lufs_normalization_always_last() {
    let mut conditions = default_conditions();
    conditions.conditions.push(EngineerCondition::TranslationRisk);
    conditions.conditions.push(EngineerCondition::DcOffsetDetected);

    let flavors = Pipelineforge::selected_flavors(&conditions);
    assert_eq!(*flavors.last().unwrap(), "LufsNormalization");
}

#[test]
fn hum_removal_always_first() {
    let mut conditions = default_conditions();
    conditions.conditions.push(EngineerCondition::MainsHumDetected);
    conditions.conditions.push(EngineerCondition::MuddyMix);

    let flavors = Pipelineforge::selected_flavors(&conditions);
    assert_eq!(*flavors.first().unwrap(), "HumRemoval");
}

#[test]
fn empty_conditions_produces_lufs_only() {
    let conditions = default_conditions();
    let flavors = Pipelineforge::selected_flavors(&conditions);
    assert_eq!(flavors.len(), 1);
    assert_eq!(flavors[0], "LufsNormalization");

    let json_str = Pipelineforge::forge(&conditions).unwrap();
    let topology = DspTopology::from_json(&json_str).unwrap();

    // 1 pass-through + 1 Limiter
    let node_count = topology.nodes.len();
    assert!(node_count > 0, "Should have minimal nodes");
}

#[test]
fn forged_topology_loads_into_dsp_graph() {
    let mut conditions = default_conditions();
    conditions.conditions.push(EngineerCondition::DcOffsetDetected);
    conditions.conditions.push(EngineerCondition::HarshTopEnd);
    conditions.conditions.push(EngineerCondition::PumpDrift);
    conditions.conditions.push(EngineerCondition::LufsTooQuiet);

    let json_str = Pipelineforge::forge(&conditions).unwrap();
    let topology = DspTopology::from_json(&json_str).unwrap();

    let mut graph = DspGraph::from_topology(&topology, 512, 48000).expect("Topology should load into DspGraph");
    
    let mut left = vec![0.0; 512];
    let mut right = vec![0.0; 512];
    graph.process_block(&mut left, &mut right);
    // Should not panic
}
