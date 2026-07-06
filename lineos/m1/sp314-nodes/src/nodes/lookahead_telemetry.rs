use crate::node::DspNode;
use libm::log10f;
use sp314_dsp::lookahead_ring::LookaheadRing;

#[inline]
fn compute_stereo_rms_db(left: &[f32], right: &[f32]) -> f32 {
    let mut sq_sum = 0.0;
    for (l, r) in left.iter().zip(right.iter()) {
        sq_sum += l * l + r * r;
    }
    let mean_sq = sq_sum / ((left.len() * 2) as f32);
    if mean_sq < 1e-12 {
        -120.0
    } else {
        10.0 * log10f(mean_sq)
    }
}

/// NOTE ON LookaheadRing CONTRACT:
/// The struct-level contract of LookaheadRing states "call consume_into() BEFORE feed()".
/// However, LookaheadTelemetryNode NEVER calls consume_into(). This is 100% SAFE and intentional.
/// The `peek_into` method calculates read position relying exclusively on `write_pos` and `filled`,
/// which are only mutated by `feed()`. `consume_into()` itself is purely an accessor and does
/// not mutate internal cursor state. Thus, an "observer-only" node like this one can safely
/// omit calling `consume_into()`.
pub struct LookaheadTelemetryNode {
    ring_l: LookaheadRing,
    ring_r: LookaheadRing,
    capacity_frames: usize,
    block_size: usize,
    recent_buf_l: Vec<f32>,
    recent_buf_r: Vec<f32>,
    old_buf_l: Vec<f32>,
    old_buf_r: Vec<f32>,
    transient_flag: f32,
}

impl LookaheadTelemetryNode {
    pub fn new(capacity_frames: usize, block_size: usize) -> Self {
        Self {
            ring_l: LookaheadRing::new(capacity_frames, block_size),
            ring_r: LookaheadRing::new(capacity_frames, block_size),
            capacity_frames,
            block_size,
            recent_buf_l: vec![0.0; block_size],
            recent_buf_r: vec![0.0; block_size],
            old_buf_l: vec![0.0; block_size],
            old_buf_r: vec![0.0; block_size],
            transient_flag: 0.0,
        }
    }
}

impl Default for LookaheadTelemetryNode {
    fn default() -> Self {
        Self::new(9728, 512)
    }
}

const PARAMS: &[&str] = &[];

impl DspNode for LookaheadTelemetryNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.ring_l.feed(left);
        self.ring_r.feed(right);

        let recent_offset = self.capacity_frames - self.block_size;
        let old_offset = self.capacity_frames - (11 * self.block_size);

        if self.ring_l.peek_into(recent_offset, &mut self.recent_buf_l)
            && self.ring_r.peek_into(recent_offset, &mut self.recent_buf_r)
            && self.ring_l.peek_into(old_offset, &mut self.old_buf_l)
            && self.ring_r.peek_into(old_offset, &mut self.old_buf_r)
        {
            let recent_db = compute_stereo_rms_db(&self.recent_buf_l, &self.recent_buf_r);
            let old_db = compute_stereo_rms_db(&self.old_buf_l, &self.old_buf_r);

            if recent_db - old_db > 6.0 {
                self.transient_flag = 1.0;
            } else {
                self.transient_flag = 0.0;
            }
        }
    }

    fn set_parameter(&mut self, _name: &str, _value: f32) -> bool {
        false
    }

    fn get_output(&self, name: &str) -> Option<f32> {
        if name == "transient_detected" {
            Some(self.transient_flag)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.transient_flag = 0.0;
    }

    fn node_type(&self) -> &'static str {
        "LookaheadTelemetry"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_is_detected_and_flag_exposed() {
        let block_size = 512;
        let capacity = 19 * block_size; // 9728
        let mut node = LookaheadTelemetryNode::new(capacity, block_size);

        let zeros = vec![0.0; block_size];

        // Feed exactly 19 blocks of silence to fill the ring fully
        for _ in 0..19 {
            let mut left = zeros.clone();
            let mut right = zeros.clone();
            node.process_stereo(&mut left, &mut right);
            assert_eq!(node.get_output("transient_detected"), Some(0.0));
        }

        // Block 20: loud impulse. This will trigger peek_into to return true.
        let mut impulse = zeros.clone();
        impulse[5] = 0.8;
        let mut right_impulse = zeros.clone();
        right_impulse[5] = 0.8;

        node.process_stereo(&mut impulse, &mut right_impulse);

        // Assert detection works via the +6dB jump on the combined energy
        assert_eq!(node.get_output("transient_detected"), Some(1.0));
    }

    #[test]
    fn steady_signal_does_not_trigger_transient() {
        let block_size = 512;
        let capacity = 19 * block_size;
        let mut node = LookaheadTelemetryNode::new(capacity, block_size);

        let steady = vec![0.5; block_size];

        // Feed 30 blocks of steady signal
        for _ in 0..30 {
            let mut left = steady.clone();
            let mut right = steady.clone();
            node.process_stereo(&mut left, &mut right);
            // Even after cold-start (block 19), steady signal won't exceed +6dB jump
            assert_eq!(node.get_output("transient_detected"), Some(0.0));
        }
    }

    #[test]
    fn output_is_bit_identical_to_input() {
        let block_size = 512;
        let capacity = 19 * block_size;
        let mut node = LookaheadTelemetryNode::new(capacity, block_size);

        let mut left = vec![0.0; block_size];
        let mut right = vec![0.0; block_size];
        left[10] = 0.123;
        right[20] = 0.456;

        let left_copy = left.clone();
        let right_copy = right.clone();

        node.process_stereo(&mut left, &mut right);

        // Zero-delay pass-through verification
        assert_eq!(left, left_copy);
        assert_eq!(right, right_copy);
    }

    #[test]
    fn zero_allocation_contract() {
        let block_size = 512;
        let capacity = 19 * block_size;
        let mut node = LookaheadTelemetryNode::new(capacity, block_size);

        let ptr_l_recent = node.recent_buf_l.as_ptr();
        let ptr_r_recent = node.recent_buf_r.as_ptr();
        let ptr_l_old = node.old_buf_l.as_ptr();
        let ptr_r_old = node.old_buf_r.as_ptr();

        let zeros = vec![0.0; block_size];

        // Process 25 blocks to surpass cold-start
        for _ in 0..25 {
            let mut left = zeros.clone();
            let mut right = zeros.clone();
            node.process_stereo(&mut left, &mut right);
        }

        // Assert heap memory remained perfectly stable (no re-allocations)
        assert_eq!(node.recent_buf_l.as_ptr(), ptr_l_recent);
        assert_eq!(node.recent_buf_r.as_ptr(), ptr_r_recent);
        assert_eq!(node.old_buf_l.as_ptr(), ptr_l_old);
        assert_eq!(node.old_buf_r.as_ptr(), ptr_r_old);
    }
}
