use crate::node::DspNode;
use sp314_dsp::restoration::biquad::{Biquad, FilterType};

pub struct DeHumNode {
    sample_rate: u32,
    enabled: bool,
    fundamental_hz: f32,
    harmonics: f32, // using f32 for parameter matching, cast to usize internally

    notches: Vec<Biquad>,
}

impl DeHumNode {
    pub fn new(sample_rate: u32) -> Self {
        let mut node = Self {
            sample_rate,
            enabled: true,
            fundamental_hz: 50.0,
            harmonics: 3.0,
            notches: Vec::new(),
        };
        node.build_notches();
        node
    }

    fn build_notches(&mut self) {
        self.notches.clear();
        let num_harmonics = (self.harmonics as usize).clamp(1, 10);
        for i in 1..=num_harmonics {
            let freq = self.fundamental_hz * i as f32;
            if freq < self.sample_rate as f32 / 2.0 {
                // Q = 20.0 from cleaner.rs
                self.notches.push(Biquad::new(
                    FilterType::Notch,
                    freq,
                    20.0,
                    self.sample_rate as f32,
                ));
            }
        }
    }
}

const PARAMS: &[&str] = &["enabled", "fundamental_hz", "harmonics"];

impl DspNode for DeHumNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        if !self.enabled || self.notches.is_empty() {
            return;
        }

        for i in 0..left.len() {
            let mut l = left[i];
            let mut r = right[i];

            for notch in &mut self.notches {
                let (nl, nr) = notch.process_stereo(l, r);
                l = nl;
                r = nr;
            }

            left[i] = l;
            right[i] = r;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        match name {
            "enabled" => self.enabled = value > 0.5,
            "fundamental_hz" => {
                if (value - self.fundamental_hz).abs() > 0.1 {
                    self.fundamental_hz = value;
                    self.build_notches();
                }
            }
            "harmonics" => {
                if (value - self.harmonics).abs() > 0.1 {
                    self.harmonics = value;
                    self.build_notches();
                }
            }
            _ => return false,
        }
        true
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        for notch in &mut self.notches {
            notch.reset();
        }
    }

    fn node_type(&self) -> &'static str {
        "DeHum"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypasses_when_disabled() {
        let mut node = DeHumNode::new(48000);
        node.set_parameter("enabled", 0.0);

        // Use 50Hz signal
        let mut left = vec![0.5; 480];
        let mut right = vec![0.5; 480];
        // If bypassed, stays 0.5 (or whatever it is)
        node.process_stereo(&mut left, &mut right);

        assert_eq!(left[0], 0.5);
    }

    #[test]
    fn attenuates_hum_frequency() {
        let mut node = DeHumNode::new(48000);
        node.set_parameter("enabled", 1.0);
        node.set_parameter("fundamental_hz", 50.0);
        node.set_parameter("harmonics", 1.0);

        // Generate 50Hz sine
        let mut left = vec![0.0; 4800];
        let mut right = vec![0.0; 4800];
        for i in 0..4800 {
            let val = libm::sinf(2.0 * core::f32::consts::PI * 50.0 * i as f32 / 48000.0);
            left[i] = val;
            right[i] = val;
        }

        node.process_stereo(&mut left, &mut right);

        // 50Hz should be heavily attenuated
        // Check energy near end of buffer
        let end_energy = left[4000..4800].iter().map(|v| v.abs()).sum::<f32>() / 800.0;
        assert!(end_energy < 0.35, "energy={}", end_energy);
    }

    #[test]
    fn no_nan() {
        let mut node = DeHumNode::new(48000);
        let mut left = vec![0.0; 480];
        let mut right = vec![0.0; 480];
        node.process_stereo(&mut left, &mut right);
        assert!(!left[0].is_nan());
    }
}
