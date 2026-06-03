use crate::node::DspNode;
use libm::{expf, powf, log10f};

pub struct AutoLevelNode {
    sample_rate: u32,
    target_rms_db: f32,
    lookahead_ms: f32,
    max_gain_db: f32,
    min_gain_db: f32,
    smoothing_ms: f32,

    // Ring buffer for sliding RMS
    window_buffer: Vec<f32>,
    window_idx: usize,
    sum_squares: f32,
    samples_seen: usize,
    
    current_gain_lin: f32,
}

impl AutoLevelNode {
    pub fn new(sample_rate: u32) -> Self {
        let lookahead_samples = (sample_rate as f32 * 500.0 / 1000.0).max(1.0) as usize;
        Self {
            sample_rate,
            target_rms_db: -18.0,
            lookahead_ms: 500.0,
            max_gain_db: 6.0,
            min_gain_db: -6.0,
            smoothing_ms: 50.0,
            window_buffer: vec![0.0; lookahead_samples],
            window_idx: 0,
            sum_squares: 0.0,
            samples_seen: 0,
            current_gain_lin: 1.0,
        }
    }

    pub fn set_params(&mut self, target: f32, lookahead: f32, max_gain: f32, min_gain: f32, smoothing: f32) {
        self.set_parameter("target_rms_db", target);
        self.set_parameter("lookahead_ms", lookahead);
        self.set_parameter("max_gain_db", max_gain);
        self.set_parameter("min_gain_db", min_gain);
        self.set_parameter("smoothing_ms", smoothing);
    }
}

impl DspNode for AutoLevelNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        if self.window_buffer.is_empty() {
            return;
        }

        let smoothing_coef = if self.smoothing_ms > 0.0 {
            expf(-1.0 / (self.sample_rate as f32 * self.smoothing_ms / 1000.0))
        } else {
            0.0
        };

        for i in 0..left.len() {
            let l = left[i];
            let r = right[i];
            
            // Mono mix for RMS calculation
            let mono = (l + r) * 0.5;
            let sq = mono * mono;
            
            // Sliding window RMS
            let old_sq = self.window_buffer[self.window_idx];
            self.sum_squares = self.sum_squares + sq - old_sq;
            if self.sum_squares < 0.0 { self.sum_squares = 0.0; }
            self.window_buffer[self.window_idx] = sq;
            
            self.window_idx += 1;
            if self.window_idx >= self.window_buffer.len() {
                self.window_idx = 0;
            }
            
            if self.samples_seen < self.window_buffer.len() {
                self.samples_seen += 1;
            }
            
            let divisor = self.samples_seen as f32;
            let mean_sq = if divisor > 0.0 { self.sum_squares / divisor } else { 0.0 };
            let rms_lin = libm::sqrtf(mean_sq);
            
            let rms_db = if rms_lin > 1e-6 {
                20.0 * log10f(rms_lin)
            } else {
                -120.0
            };
            
            let mut target_gain_db = self.target_rms_db - rms_db;
            if target_gain_db > self.max_gain_db { target_gain_db = self.max_gain_db; }
            if target_gain_db < self.min_gain_db { target_gain_db = self.min_gain_db; }
            
            let target_gain_lin = powf(10.0, target_gain_db / 20.0);
            
            self.current_gain_lin = self.current_gain_lin * smoothing_coef + target_gain_lin * (1.0 - smoothing_coef);
            
            left[i] = l * self.current_gain_lin;
            right[i] = r * self.current_gain_lin;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) {
        match name {
            "target_rms_db" => self.target_rms_db = value,
            "lookahead_ms" => {
                if (self.lookahead_ms - value).abs() > 0.1 {
                    self.lookahead_ms = value;
                    let samples = (self.sample_rate as f32 * value / 1000.0).max(1.0) as usize;
                    self.window_buffer = vec![0.0; samples];
                    self.window_idx = 0;
                    self.sum_squares = 0.0;
                    self.samples_seen = 0;
                }
            }
            "max_gain_db" => self.max_gain_db = value,
            "min_gain_db" => self.min_gain_db = value,
            "smoothing_ms" => self.smoothing_ms = value,
            _ => {}
        }
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.window_buffer.fill(0.0);
        self.window_idx = 0;
        self.sum_squares = 0.0;
        self.samples_seen = 0;
        self.current_gain_lin = 1.0;
    }

    fn node_type(&self) -> &'static str {
        "AutoLevel"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_when_at_target() {
        let mut node = AutoLevelNode::new(48000);
        node.set_params(-18.0, 500.0, 6.0, -6.0, 50.0);
        // Signal already at target RMS (-18dBFS ≈ 0.126 amplitude)
        let amp = 10.0_f32.powf(-18.0 / 20.0);
        let mut l = vec![amp; 512];
        let mut r = vec![amp; 512];
        let l_orig = l.clone();
        node.process_stereo(&mut l, &mut r);
        // Gain should be ~1.0 — signal unchanged
        let ratio = l[511] / l_orig[511];
        assert!((ratio - 1.0).abs() < 0.1, "Expected unity gain, got {}", ratio);
    }

    #[test]
    fn boosts_quiet_signal() {
        let mut node = AutoLevelNode::new(48000);
        node.set_params(-18.0, 500.0, 12.0, -12.0, 50.0);
        // Very quiet signal — well below target
        let mut l = vec![0.001f32; 4800]; // 100ms of quiet
        let mut r = vec![0.001f32; 4800];
        node.process_stereo(&mut l, &mut r);
        // Output should be louder than input
        assert!(l[4799] > 0.001, "Signal not boosted");
        assert!(l[4799] < 0.001 * 10.0_f32.powf(12.0 / 20.0) + 0.0001, "Gain exceeded maximum");
    }

    #[test]
    fn attenuates_loud_signal() {
        let mut node = AutoLevelNode::new(48000);
        node.set_params(-18.0, 500.0, 6.0, -12.0, 50.0);
        // Hot signal — well above target
        let mut l = vec![0.9f32; 4800];
        let mut r = vec![0.9f32; 4800];
        node.process_stereo(&mut l, &mut r);
        // Output should be quieter than input
        assert!(l[4799] < 0.9, "Signal not attenuated");
    }

    #[test]
    fn gain_change_is_smooth() {
        let mut node = AutoLevelNode::new(48000);
        node.set_params(-18.0, 500.0, 12.0, -12.0, 100.0); // 100ms smoothing
        let mut l = vec![0.001f32; 4800];
        let mut r = vec![0.001f32; 4800];
        node.process_stereo(&mut l, &mut r);
        // Check no abrupt jumps between adjacent samples
        for i in 1..4800 {
            let delta = (l[i] - l[i-1]).abs();
            assert!(delta < 0.01, "Abrupt gain change at sample {}: {}", i, delta);
        }
    }

    #[test]
    fn no_nan_or_inf() {
        let mut node = AutoLevelNode::new(48000);
        node.set_params(-18.0, 500.0, 12.0, -12.0, 50.0);
        let mut l: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let mut r = l.clone();
        node.process_stereo(&mut l, &mut r);
        assert!(l.iter().all(|x| x.is_finite()), "NaN/Inf in output");
    }
}
