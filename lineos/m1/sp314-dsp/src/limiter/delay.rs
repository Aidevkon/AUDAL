// src/limiter/delay.rs
// Circular ring buffer for lookahead delay.
// Dynamic allocation — lives on heap.

pub struct RingBuffer {
    buffer: std::vec::Vec<f32>,
    write_pos: usize,
}

impl RingBuffer {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0_f32; size],
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
            if abs_val > max_val {
                max_val = abs_val;
            }
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

/// Ring of per-sample peak estimates, aligned with the audio delay line.
///
/// F-048: the limiter used to derive `delayed_peak` from `RingBuffer::max_abs()`,
/// i.e. the raw magnitude of the delayed samples. That discards the true-peak
/// estimate computed for each sample on the way in, so material whose
/// intersample peaks exceed its sample peaks passed unlimited. This ring stores
/// the estimate itself, so the sidechain sees the value belonging to the sample
/// it is about to scale, for the whole lookahead window rather than one sample.
///
/// Note: TruePeakDetector carries an 18-tap history, so its output lags the
/// input by roughly 9 samples. Against a 240-sample lookahead that is slack,
/// not exact alignment.
pub struct PeakRing {
    buffer: std::vec::Vec<f32>,
    write_pos: usize,
}

impl PeakRing {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0_f32; size],
            write_pos: 0,
        }
    }

    /// Stores `peak` and returns nothing; call `max()` before pushing if the
    /// window must exclude the incoming estimate.
    #[inline]
    pub fn push(&mut self, peak: f32) {
        if self.buffer.is_empty() {
            return;
        }
        self.buffer[self.write_pos] = peak;
        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }
    }

    #[inline]
    pub fn max(&self) -> f32 {
        let mut m = 0.0_f32;
        for &v in self.buffer.iter() {
            if v > m {
                m = v;
            }
        }
        m
    }

    pub fn reset(&mut self) {
        for v in self.buffer.iter_mut() {
            *v = 0.0;
        }
        self.write_pos = 0;
    }
}
