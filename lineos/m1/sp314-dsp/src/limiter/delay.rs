// src/limiter/delay.rs
// Circular ring buffer for lookahead delay.
// Const generic N = compile-time buffer size.
// Zero allocation — lives on stack.

pub struct RingBuffer<const N: usize> {
    buffer:    [f32; N],
    write_pos: usize,
}

impl<const N: usize> RingBuffer<N> {
    pub const fn new() -> Self {
        Self {
            buffer:    [0.0_f32; N],
            write_pos: 0,
        }
    }
}

impl<const N: usize> Default for RingBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> RingBuffer<N> {
    /// Write one sample, return the sample that was N frames ago.
    /// This is the core lookahead operation:
    /// push new sample in, get old sample out.
    #[inline]
    pub fn push_and_pop(&mut self, x: f32) -> f32 {
        let delayed = self.buffer[self.write_pos];
        self.buffer[self.write_pos] = x;
        self.write_pos += 1;
        if self.write_pos >= N {
            self.write_pos = 0;
        }
        delayed
    }

    /// Returns the maximum absolute value currently in the buffer.
    /// Vital for true lookahead: holds the envelope until peaks exit.
    /// Scans N stack-allocated f32s — negligible CPU cost.
    #[inline]
    pub fn max_abs(&self) -> f32 {
        let mut max_val = 0.0_f32;
        for &val in self.buffer.iter() {
            let abs_val = libm::fabsf(val);
            if abs_val > max_val { max_val = abs_val; }
        }
        max_val
    }

    pub fn reset(&mut self) {
        self.buffer    = [0.0_f32; N];
        self.write_pos = 0;
    }
}
