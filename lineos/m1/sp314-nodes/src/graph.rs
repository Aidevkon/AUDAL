use crate::node::DspNode;
use crate::nodes::autolevel::AutoLevelNode;
use crate::nodes::biquad::BiquadFilterNode;
use crate::nodes::compressor::CompressorNode;
use crate::nodes::deesser::DeEsserNode;
use crate::nodes::dehum::DeHumNode;
use crate::nodes::gain::GainNode;
use crate::nodes::input::InputNode;
use crate::nodes::limiter::LimiterNode;
use crate::nodes::ms::{InverseMsMatrixNode, MsMatrixNode};
use crate::nodes::noisegate::NoiseGateNode;
use crate::nodes::output::OutputNode;
use crate::nodes::reverb::ReverbNode;
use crate::nodes::rms::RmsDetectorNode;
use crate::nodes::width::WidthNode;
use crate::topology::DspTopology;
use std::collections::{HashMap, VecDeque};

#[derive(Debug)]
pub enum GraphError {
    UnknownNodeType(String),
    CycleDetected,
    MissingNode(String),
    InvalidTopology(String),
}

struct ParamEdge {
    source_node: String,
    source_output: String,
    target_node: String,
    target_parameter: String,
}

pub struct DspGraph {
    pub execution_order: Vec<String>,
    nodes: HashMap<String, Box<dyn DspNode>>,
    param_edges: Vec<ParamEdge>,

    // Audio routing
    audio_deps: HashMap<String, Vec<String>>,
    buffers: HashMap<String, (Vec<f32>, Vec<f32>)>,
    acc_left: Vec<f32>,
    acc_right: Vec<f32>,
    pub debug_sq_l: HashMap<String, f64>,
    pub debug_sq_r: HashMap<String, f64>,
    pub debug_frames: usize,
    block_size: usize,
    sample_rate: u32,
    topology: DspTopology,
}

impl Clone for DspGraph {
    fn clone(&self) -> Self {
        DspGraph::from_topology(&self.topology, self.block_size, self.sample_rate).unwrap()
    }
}

impl DspGraph {
    pub fn from_topology(
        topology: &DspTopology,
        block_size: usize,
        sample_rate: u32,
    ) -> Result<Self, GraphError> {
        let mut nodes: HashMap<String, Box<dyn DspNode>> = HashMap::new();
        let mut buffers = HashMap::new();

        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut audio_deps: HashMap<String, Vec<String>> = HashMap::new();
        let mut param_edges = Vec::new();

        // 1. Create nodes
        for t_node in &topology.nodes {
            let node: Box<dyn DspNode> = match t_node.node_type.as_str() {
                "Input" => Box::new(InputNode),
                "Output" => Box::new(OutputNode),
                "Gain" => {
                    let gain = t_node
                        .parameters
                        .get("gain")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1.0) as f32;
                    Box::new(GainNode::new(gain, sample_rate as f32))
                }
                "BiquadFilter" => {
                    let mut b = BiquadFilterNode::new(sample_rate as f32);
                    if let Some(ft) = t_node
                        .parameters
                        .get("filter_type")
                        .and_then(|v| v.as_f64())
                    {
                        b.set_parameter("filter_type", ft as f32);
                    }
                    if let Some(f) = t_node.parameters.get("freq_hz").and_then(|v| v.as_f64()) {
                        b.set_parameter("freq_hz", f as f32);
                    }
                    if let Some(q) = t_node.parameters.get("q").and_then(|v| v.as_f64()) {
                        b.set_parameter("q", q as f32);
                    }
                    if let Some(g) = t_node.parameters.get("gain_db").and_then(|v| v.as_f64()) {
                        b.set_parameter("gain_db", g as f32);
                    }
                    Box::new(b)
                }
                "RMS_Detector" => {
                    let attack = t_node
                        .parameters
                        .get("attack_ms")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(10.0) as f32;
                    let release = t_node
                        .parameters
                        .get("release_ms")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(100.0) as f32;
                    let threshold = t_node
                        .parameters
                        .get("threshold_db")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(-20.0) as f32;
                    Box::new(RmsDetectorNode::new(
                        attack,
                        release,
                        threshold,
                        sample_rate as f32,
                    ))
                }
                "MS_Matrix" => Box::new(MsMatrixNode),
                "Inverse_MS_Matrix" => Box::new(InverseMsMatrixNode),
                "Compressor" => {
                    let mut c = CompressorNode::new(sample_rate as f32);
                    if let Some(t) = t_node
                        .parameters
                        .get("threshold_db")
                        .and_then(|v| v.as_f64())
                    {
                        c.set_parameter("threshold_db", t as f32);
                    }
                    if let Some(r) = t_node.parameters.get("ratio").and_then(|v| v.as_f64()) {
                        c.set_parameter("ratio", r as f32);
                    }
                    if let Some(k) = t_node.parameters.get("knee_db").and_then(|v| v.as_f64()) {
                        c.set_parameter("knee_db", k as f32);
                    }
                    if let Some(a) = t_node.parameters.get("attack_ms").and_then(|v| v.as_f64()) {
                        c.set_parameter("attack_ms", a as f32);
                    }
                    if let Some(rel) = t_node.parameters.get("release_ms").and_then(|v| v.as_f64())
                    {
                        c.set_parameter("release_ms", rel as f32);
                    }
                    if let Some(m) = t_node.parameters.get("makeup_db").and_then(|v| v.as_f64()) {
                        c.set_parameter("makeup_db", m as f32);
                    }
                    Box::new(c)
                }
                "MultibandCompressor" => {
                    let f_low = t_node
                        .parameters
                        .get("f_low")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(200.0) as f32;
                    let f_high = t_node
                        .parameters
                        .get("f_high")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(3000.0) as f32;
                    Box::new(crate::nodes::multiband::MultibandCompressorNode::new(
                        sample_rate as f32,
                        f_low,
                        f_high,
                    ))
                }
                "Harmonic" => {
                    let drive = t_node
                        .parameters
                        .get("drive")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(2.0) as f32;
                    let mix = t_node
                        .parameters
                        .get("mix")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.3) as f32;
                    let even_amount = t_node
                        .parameters
                        .get("even_amount")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.6) as f32;
                    let odd_amount = t_node
                        .parameters
                        .get("odd_amount")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.2) as f32;
                    Box::new(crate::nodes::harmonic::HarmonicNode::new(
                        drive,
                        mix,
                        even_amount,
                        odd_amount,
                    ))
                }
                "Limiter" => {
                    let mut l = LimiterNode::new(sample_rate as f32);
                    if let Some(c) = t_node.parameters.get("ceiling_db").and_then(|v| v.as_f64()) {
                        l.set_parameter("ceiling_db", c as f32);
                    }
                    Box::new(l)
                }
                "Reverb" => Box::new(ReverbNode::new(sample_rate)),
                "Width" => Box::new(WidthNode::new(sample_rate)),
                "NoiseGate" => Box::new(NoiseGateNode::new(sample_rate)),
                "DeEsser" => Box::new(DeEsserNode::new(sample_rate)),
                "DeHum" => Box::new(DeHumNode::new(sample_rate)),
                "AutoLevel" => Box::new(AutoLevelNode::new(sample_rate)),
                _ => return Err(GraphError::UnknownNodeType(t_node.node_type.clone())),
            };

            nodes.insert(t_node.node_id.clone(), node);
            buffers.insert(
                t_node.node_id.clone(),
                (vec![0.0; block_size], vec![0.0; block_size]),
            );
            in_degree.insert(t_node.node_id.clone(), 0);
            adj.insert(t_node.node_id.clone(), Vec::new());
        }

        // 2. Build edges
        for edge in &topology.edges {
            if !nodes.contains_key(&edge.source) {
                return Err(GraphError::MissingNode(edge.source.clone()));
            }
            if !nodes.contains_key(&edge.target) {
                return Err(GraphError::MissingNode(edge.target.clone()));
            }

            // Every edge dictates execution order: source must run before target
            adj.get_mut(&edge.source).unwrap().push(edge.target.clone());
            *in_degree.get_mut(&edge.target).unwrap() += 1;

            if edge.modulation_type == "audio" {
                audio_deps
                    .entry(edge.target.clone())
                    .or_default()
                    .push(edge.source.clone());
            } else if edge.modulation_type == "parameter" {
                param_edges.push(ParamEdge {
                    source_node: edge.source.clone(),
                    source_output: edge
                        .source_output
                        .clone()
                        .unwrap_or_else(|| "envelope".to_string()),
                    target_node: edge.target.clone(),
                    target_parameter: edge.target_parameter.clone().unwrap_or_default(),
                });
            }
        }

        // 3. Topological Sort (Kahn's algorithm)
        let mut execution_order = Vec::new();
        let mut queue = VecDeque::new();
        for (node_id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(node_id.clone());
            }
        }

        while let Some(u) = queue.pop_front() {
            execution_order.push(u.clone());
            for v in &adj[&u] {
                let deg = in_degree.get_mut(v).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(v.clone());
                }
            }
        }

        if execution_order.len() != nodes.len() {
            return Err(GraphError::CycleDetected);
        }

        // Snap all parameters to their targets initially
        for node in nodes.values_mut() {
            node.reset();
        }

        Ok(Self {
            execution_order,
            nodes,
            param_edges,
            audio_deps,
            buffers,
            acc_left: vec![0.0; block_size],
            acc_right: vec![0.0; block_size],
            debug_sq_l: HashMap::new(),
            debug_sq_r: HashMap::new(),
            debug_frames: 0,
            block_size,
            sample_rate,
            topology: topology.clone(), // IDE refresh: topology does implement Clone
        })
    }

    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        debug_assert!(left.len() <= self.block_size);
        debug_assert!(right.len() <= self.block_size);

        // 1. Resolve parameter modulation edges
        for edge in &self.param_edges {
            if let Some(val) = self
                .nodes
                .get(&edge.source_node)
                .unwrap()
                .get_output(&edge.source_output)
            {
                self.nodes
                    .get_mut(&edge.target_node)
                    .unwrap()
                    .set_parameter_no_glide(&edge.target_parameter, val);
            }
        }

        // 2. Execute nodes in topological order
        for node_id in &self.execution_order {
            let node_type = self.nodes[node_id].node_type();

            if node_type == "Input" {
                let (buf_l, buf_r) = self.buffers.get_mut(node_id).unwrap();
                let len = left.len().min(self.block_size);
                buf_l[..len].copy_from_slice(&left[..len]);
                buf_r[..len].copy_from_slice(&right[..len]);
                // Zero-pad remainder if chunk shorter than block_size
                for i in len..self.block_size {
                    buf_l[i] = 0.0_f32;
                    buf_r[i] = 0.0_f32;
                }
                self.nodes
                    .get_mut(node_id)
                    .unwrap()
                    .process_stereo(buf_l, buf_r);
                continue;
            }

            self.acc_left.fill(0.0);
            self.acc_right.fill(0.0);

            if let Some(deps) = self.audio_deps.get(node_id) {
                for src_id in deps {
                    let (src_l, src_r) = self.buffers.get(src_id).unwrap();
                    for i in 0..self.block_size {
                        self.acc_left[i] += src_l[i];
                        self.acc_right[i] += src_r[i];
                    }
                }
            }

            let (buf_l, buf_r) = self.buffers.get_mut(node_id).unwrap();
            buf_l.copy_from_slice(&self.acc_left);
            buf_r.copy_from_slice(&self.acc_right);

            self.nodes
                .get_mut(node_id)
                .unwrap()
                .process_stereo(buf_l, buf_r);

            let sq_l: f64 = buf_l[..self.block_size]
                .iter()
                .map(|&x| (x as f64) * (x as f64))
                .sum();
            let sq_r: f64 = buf_r[..self.block_size]
                .iter()
                .map(|&x| (x as f64) * (x as f64))
                .sum();
            *self.debug_sq_l.entry(node_id.clone()).or_insert(0.0) += sq_l;
            *self.debug_sq_r.entry(node_id.clone()).or_insert(0.0) += sq_r;

            if node_type == "Output" {
                let len = left.len();
                left.copy_from_slice(&buf_l[..len]);
                right.copy_from_slice(&buf_r[..len]);
            }
        }
        self.debug_frames += self.block_size;
    }

    pub fn reset(&mut self) {
        for node in self.nodes.values_mut() {
            node.reset();
        }
        for (l, r) in self.buffers.values_mut() {
            l.fill(0.0);
            r.fill(0.0);
        }
    }

    pub fn set_node_parameter(&mut self, node_id: &str, param: &str, value: f32) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.set_parameter(param, value);
        }
    }

    pub fn set_node_glide_ms(&mut self, node_id: &str, glide_ms: f32) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.set_parameter("glide_ms", glide_ms);
        }
    }

    pub fn set_global_glide_ms(&mut self, glide_ms: f32) {
        for node in self.nodes.values_mut() {
            node.set_parameter("glide_ms", glide_ms);
        }
    }
}
