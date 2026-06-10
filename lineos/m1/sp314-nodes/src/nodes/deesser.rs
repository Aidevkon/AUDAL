use crate::node::DspNode;
use libm::{expf, fabsf, powf};
use sp314_dsp::restoration::biquad::{Biquad, FilterType};

pub struct DeEsserNode {
    sample_rate: u32,
    threshold_db: f32,
    frequency_hz: f32,
    ratio: f32,

    hp_filter: Biquad,
    env_l: f32,
    env_r: f32,
    attack_coef: f32,
    release_coef: f32,
}

impl DeEsserNode {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            threshold_db: -24.0,
            frequency_hz: 6000.0,
            ratio: 4.0,
            hp_filter: Biquad::new(FilterType::HighPass, 6000.0, 0.707, sample_rate as f32),
            env_l: 0.0,
            env_r: 0.0,
            attack_coef: expf(-1.0 / (sample_rate as f32 * 0.001)),
            release_coef: expf(-1.0 / (sample_rate as f32 * 0.050)),
        }
    }
}

impl DspNode for DeEsserNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let threshold_lin = powf(10.0, self.threshold_db / 20.0);

        for i in 0..left.len() {
            let mut l = left[i];
            let mut r = right[i];

            let (hf_l, hf_r) = self.hp_filter.process_stereo(l, r);

            let rect_l = fabsf(hf_l);
            self.env_l = if rect_l > self.env_l {
                self.env_l + (rect_l - self.env_l) * (1.0 - self.attack_coef)
            } else {
                self.env_l + (rect_l - self.env_l) * (1.0 - self.release_coef)
            };

            if self.env_l > threshold_lin {
                let overshoot = self.env_l - threshold_lin;
                let reduction = 1.0 - (overshoot / (overshoot + threshold_lin * self.ratio));
                l *= reduction;
            }

            let rect_r = fabsf(hf_r);
            self.env_r = if rect_r > self.env_r {
                self.env_r + (rect_r - self.env_r) * (1.0 - self.attack_coef)
            } else {
                self.env_r + (rect_r - self.env_r) * (1.0 - self.release_coef)
            };

            if self.env_r > threshold_lin {
                let overshoot = self.env_r - threshold_lin;
                let reduction = 1.0 - (overshoot / (overshoot + threshold_lin * self.ratio));
                r *= reduction;
            }

            left[i] = l;
            right[i] = r;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) {
        match name {
            "threshold_db" => self.threshold_db = value,
            "frequency_hz" => {
                self.frequency_hz = value;
                self.hp_filter =
                    Biquad::new(FilterType::HighPass, value, 0.707, self.sample_rate as f32);
            }
            "ratio" => self.ratio = value,
            _ => {}
        }
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.hp_filter.reset();
        self.env_l = 0.0;
        self.env_r = 0.0;
    }

    fn node_type(&self) -> &'static str {
        "DeEsser"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypasses_low_freq() {
        let mut node = DeEsserNode::new(48000);
        node.set_parameter("threshold_db", -40.0);
        node.set_parameter("frequency_hz", 6000.0);

        let mut left = vec![0.5; 480]; // DC is 0Hz, well below 6000Hz
        let mut right = vec![0.5; 480];
        node.process_stereo(&mut left, &mut right);

        assert!((left[479] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn attenuates_sibilance_above_threshold() {
        let mut node = DeEsserNode::new(48000);
        node.set_parameter("threshold_db", -40.0);
        node.set_parameter("frequency_hz", 1000.0); // lower to catch nyquist oscillation

        let mut left = vec![0.0; 480];
        let mut right = vec![0.0; 480];
        // Nyquist frequency oscillation (24kHz)
        for i in 0..480 {
            left[i] = if i % 2 == 0 { 0.5 } else { -0.5 };
            right[i] = if i % 2 == 0 { 0.5 } else { -0.5 };
        }

        node.process_stereo(&mut left, &mut right);

        // Should be attenuated
        assert!(left[479].abs() < 0.5);
    }

    #[test]
    fn no_nan() {
        let mut node = DeEsserNode::new(48000);
        let mut left = vec![0.0; 480];
        let mut right = vec![0.0; 480];
        node.process_stereo(&mut left, &mut right);
        assert!(!left[0].is_nan());
    }
}
