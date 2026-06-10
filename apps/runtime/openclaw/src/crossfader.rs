use sp314_nodes::graph::DspGraph;

pub struct Crossfader {
    graph_a: Option<DspGraph>,
    graph_b: Option<DspGraph>,
    fade_samples_total: usize,
    fade_samples_remaining: usize,
    buf_a_left: Vec<f32>,
    buf_a_right: Vec<f32>,
    buf_b_left: Vec<f32>,
    buf_b_right: Vec<f32>,
    active: bool,
    block_size: usize,
}

impl Crossfader {
    pub fn new(block_size: usize) -> Self {
        Self {
            graph_a: None,
            graph_b: None,
            fade_samples_total: 0,
            fade_samples_remaining: 0,
            buf_a_left: vec![0.0; block_size],
            buf_a_right: vec![0.0; block_size],
            buf_b_left: vec![0.0; block_size],
            buf_b_right: vec![0.0; block_size],
            active: false,
            block_size,
        }
    }

    pub fn begin(&mut self, current_graph: DspGraph, mut new_graph: DspGraph, fade_samples: usize) {
        new_graph.reset();

        self.graph_a = Some(current_graph);
        self.graph_b = Some(new_graph);
        self.fade_samples_total = fade_samples;
        self.fade_samples_remaining = fade_samples;
        self.active = true;
    }

    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) -> bool {
        if !self.active {
            return true;
        }

        // We process both graphs into our pre-allocated buffers.
        // We copy the input into both sets of buffers.
        self.buf_a_left.copy_from_slice(left);
        self.buf_a_right.copy_from_slice(right);
        self.buf_b_left.copy_from_slice(left);
        self.buf_b_right.copy_from_slice(right);

        if let Some(ga) = &mut self.graph_a {
            ga.process_block(&mut self.buf_a_left, &mut self.buf_a_right);
        }
        if let Some(gb) = &mut self.graph_b {
            gb.process_block(&mut self.buf_b_left, &mut self.buf_b_right);
        }

        // If fade is instantly 0 (no crossfade requested), jump to end.
        if self.fade_samples_total == 0 || self.fade_samples_remaining == 0 {
            left.copy_from_slice(&self.buf_b_left);
            right.copy_from_slice(&self.buf_b_right);
            self.active = false;
            return true;
        }

        let fade_this_block = std::cmp::min(self.block_size, self.fade_samples_remaining);

        for i in 0..fade_this_block {
            let progress =
                1.0 - ((self.fade_samples_remaining - i) as f32 / self.fade_samples_total as f32);
            let gain_a = 1.0 - progress;
            let gain_b = progress;
            left[i] = self.buf_a_left[i] * gain_a + self.buf_b_left[i] * gain_b;
            right[i] = self.buf_a_right[i] * gain_a + self.buf_b_right[i] * gain_b;
        }

        // For the remainder of the block (if crossfade ends mid-block), use graph B entirely
        if fade_this_block < self.block_size {
            for i in fade_this_block..self.block_size {
                left[i] = self.buf_b_left[i];
                right[i] = self.buf_b_right[i];
            }
            self.fade_samples_remaining = 0;
            self.active = false;
            return true;
        }

        self.fade_samples_remaining -= self.block_size;

        if self.fade_samples_remaining == 0 {
            self.active = false;
            true
        } else {
            false
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn take_completed_graph(&mut self) -> Option<DspGraph> {
        self.graph_a = None; // Drop old graph
        self.graph_b.take()
    }

    pub fn clear(&mut self) {
        self.active = false;
        self.graph_a = None;
        self.graph_b = None;
    }
}
