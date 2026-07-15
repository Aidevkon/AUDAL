use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DspTopology {
    pub topology_id: String,
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<TopologyEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(pub String);

pub struct DspTopologyBuilder {
    topology_id: String,
    nodes: Vec<TopologyNode>,
    edges: Vec<TopologyEdge>,
}

impl DspTopologyBuilder {
    pub fn new(topology_id: impl Into<String>) -> Self {
        Self {
            topology_id: topology_id.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(
        &mut self,
        node_id: impl Into<String>,
        node_type: impl Into<String>,
        parameters: serde_json::Value,
    ) -> NodeId {
        let node_id_str = node_id.into();
        self.nodes.push(TopologyNode {
            node_id: node_id_str.clone(),
            node_type: node_type.into(),
            parameters,
        });
        NodeId(node_id_str)
    }

    pub fn connect(&mut self, source: &NodeId, target: &NodeId) -> &mut Self {
        self.edges.push(TopologyEdge {
            source: source.0.clone(),
            target: target.0.clone(),
            modulation_type: "audio".to_string(),
            source_output: None,
            target_parameter: None,
        });
        self
    }

    pub fn build(self) -> DspTopology {
        DspTopology {
            topology_id: self.topology_id,
            nodes: self.nodes,
            edges: self.edges,
        }
    }
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
