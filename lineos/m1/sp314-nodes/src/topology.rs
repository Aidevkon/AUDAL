use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DspTopology {
    pub topology_id: String,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TopologyNode {
    pub node_id: String,
    pub node_type: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TopologyEdge {
    pub source: String,
    pub target: String,
    #[serde(default = "default_audio")]
    pub modulation_type: String, // "audio" | "parameter"
    pub source_output: Option<String>,
    pub target_parameter: Option<String>,
}

fn default_audio() -> String {
    "audio".to_string()
}

impl DspTopology {
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
