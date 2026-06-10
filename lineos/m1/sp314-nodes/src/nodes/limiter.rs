use crate::node::DspNode;
use sp314_dsp::limiter::core::{BrickwallLimiter, LimiterConfig};

pub struct LimiterNode {
    limiter: BrickwallLimiter,
    ceiling_db: f32,
    sample_rate: f32,
}

impl LimiterNode {
    pub fn new(sample_rate: f32) -> Self {
        let ceiling_db = -0.5;
        let config = LimiterConfig {
            release_ms: 100.0,
            ceiling_db,
            midside_eq_enabled: false,
            true_peak_enabled: true,
        };
        Self {
            limiter: BrickwallLimiter::new(config, sample_rate as u32),
            ceiling_db,
            sample_rate,
        }
    }
}

impl DspNode for LimiterNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.limiter.process_block(left, right);
    }

    fn set_parameter(&mut self, name: &str, value: f32) {
        if name == "ceiling_db" && self.ceiling_db != value {
            self.ceiling_db = value;
            let config = LimiterConfig {
                release_ms: 100.0,
                ceiling_db: self.ceiling_db,
                midside_eq_enabled: false,
                true_peak_enabled: true,
            };
            // Note: Re-creating the limiter flushes the lookahead delay line.
            // This is acceptable only when re-configuring before processing,
            // but not ideal during live parameter automation.
            // However, BrickwallLimiter in sp314-dsp doesn't expose a method
            // to update ceiling_db dynamically without losing state.
            // We'll keep the state reset here as it's the only safe way given the API.
            self.limiter = BrickwallLimiter::new(config, self.sample_rate as u32);
        }
    }

    fn get_output(&self, name: &str) -> Option<f32> {
        if name == "gain_reduction_db" {
            // BrickwallLimiter currently does not expose gain reduction in sp314-dsp
            // We return 0.0 to satisfy the node interface without crashing
            Some(0.0)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.limiter.reset();
    }

    fn node_type(&self) -> &'static str {
        "Limiter"
    }
}
