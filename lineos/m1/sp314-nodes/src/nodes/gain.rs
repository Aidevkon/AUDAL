use crate::node::DspNode;
use crate::glider::ParameterGlider;

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

impl DspNode for GainNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            if self.gain_glider.is_gliding() {
                self.gain_linear = self.gain_glider.next();
            }
            *l *= self.gain_linear;
            *r *= self.gain_linear;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) {
        if name == "gain" {
            self.gain_glider.set_target(value);
        } else if name == "glide_ms" {
            self.glide_ms = value;
            self.gain_glider.set_glide_ms(value);
        }
    }

    fn set_parameter_no_glide(&mut self, name: &str, value: f32) {
        if name == "gain" {
            self.gain_glider.set_target_instant(value);
            self.gain_linear = value;
        } else if name == "glide_ms" {
            self.glide_ms = value;
            self.gain_glider.set_glide_ms(value);
        }
    }

    fn get_output(&self, _name: &str) -> Option<f32> { None }

    fn reset(&mut self) {
        self.gain_glider.reset();
        self.gain_linear = self.gain_glider.value();
    }

    fn node_type(&self) -> &'static str { "Gain" }
}
