// src/limiter/delay.rs
// Circular ring buffer for lookahead delay.
// Dynamic allocation — lives on heap.

pub struct RingBuffer {
    buffer:    std::vec::Vec<f32>,
    write_pos: usize,
}

impl RingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            buffer:    vec![0.0_f32; size],
            write_pos: 0,
        }
    }

    #[inline]
    pub fn push_and_pop(&mut self, x: f32) -> f32 {
        if self.buffer.is_empty() {
            return x;
        }
        let delayed = self.buffer[self.write_pos];
        self.buffer[self.write_pos] = x;
        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }
        delayed
    }

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
        for val in self.buffer.iter_mut() {
            *val = 0.0;
        }
        self.write_pos = 0;
    }
}
