use crate::glider::ParameterGlider;
use crate::node::DspNode;

pub struct GainNode {
    gain_linear: f32,
    gain_glider: ParameterGlider,
    glide_ms: f32,
}

impl GainNode {
    pub fn new(gain_linear: f32, sample_rate: f32) -> Self {
        Self {
            gain_linear,
            gain_glider: ParameterGlider::new(gain_linear, 300.0, sample_rate),
            glide_ms: 300.0,
        }
    }
}

const PARAMS: &[&str] = &["gain", "glide_ms"];

impl DspNode for GainNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }

    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.gain_glider.is_gliding() {
                self.gain_linear = self.gain_glider.next();
            }
            *l *= self.gain_linear;
            *r *= self.gain_linear;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        if name == "gain" {
            self.gain_glider.set_target(value);
        } else if name == "glide_ms" {
            self.glide_ms = value;
            self.gain_glider.set_glide_ms(value);
        }
        true
    }

    fn set_parameter_no_glide(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        if name == "gain" {
            self.gain_glider.set_target_instant(value);
            self.gain_linear = value;
        } else if name == "glide_ms" {
            self.glide_ms = value;
            self.gain_glider.set_glide_ms(value);
        }
        true
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.gain_glider.reset();
        self.gain_linear = self.gain_glider.value();
    }

    fn node_type(&self) -> &'static str {
        "Gain"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_parameter_unknown_name_returns_false() {
        // Test A — set_parameter_unknown_name_returns_false:
        let mut node = GainNode::new(1.0, 48000.0);
        let result = node.set_parameter("gian", 0.5);
        assert!(!result, "Typo parameter 'gian' should return false");
    }

    #[test]
    fn test_set_parameter_known_name_returns_true_and_applies() {
        // Test B — set_parameter_known_name_returns_true_and_applies:
        let mut node = GainNode::new(1.0, 48000.0);
        let result = node.set_parameter_no_glide("gain", 0.5);
        assert!(result, "Valid parameter 'gain' should return true");

        // Επιβεβαίωσε ότι η πραγματική τιμή άλλαξε
        let mut left = [1.0];
        let mut right = [1.0];
        node.process_stereo(&mut left, &mut right);
        assert_eq!(left[0], 0.5, "Gain linear should be applied to audio");
    }

    #[test]
    fn test_has_parameter_matches_param_names() {
        // Test C — has_parameter_matches_param_names:
        let node = GainNode::new(1.0, 48000.0);
        assert!(node.has_parameter("gain"));
        assert!(node.has_parameter("glide_ms"));
        assert!(!node.has_parameter("nonexistent"));
    }
}
