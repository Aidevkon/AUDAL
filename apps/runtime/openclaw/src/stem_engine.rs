use sp314_nodes::graph::DspGraph;
use crate::stem::StemBuffer;

pub struct StemEngine {
    pub id: String,
    buffer: StemBuffer,
    graph: DspGraph,
    left_buf: Vec<f32>,
    right_buf: Vec<f32>,
}

impl StemEngine {
    pub fn new(buffer: StemBuffer, graph: DspGraph, block_size: usize) -> Self {
        Self {
            id: buffer.id.clone(),
            buffer,
            graph,
            left_buf: vec![0.0; block_size],
            right_buf: vec![0.0; block_size],
        }
    }

    pub fn process_block(&mut self, frame_offset: usize, out_left: &mut [f32], out_right: &mut [f32]) {
        self.buffer.read_block(frame_offset, &mut self.left_buf, &mut self.right_buf);
        self.graph.process_block(&mut self.left_buf, &mut self.right_buf);
        out_left.copy_from_slice(&self.left_buf);
        out_right.copy_from_slice(&self.right_buf);
    }

    pub fn swap_graph(&mut self, mut new_graph: DspGraph) {
        new_graph.reset();
        self.graph = new_graph;
    }

    pub fn reset(&mut self) {
        self.graph.reset();
    }

    pub fn set_node_parameter(&mut self, node_id: &str, param: &str, value: f32) {
        self.graph.set_node_parameter(node_id, param, value);
    }

    pub fn set_global_glide_ms(&mut self, glide_ms: f32) {
        self.graph.set_global_glide_ms(glide_ms);
    }
}
