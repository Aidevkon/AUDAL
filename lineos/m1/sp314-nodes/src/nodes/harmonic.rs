use crate::node::DspNode;
use sp314_dsp::harmonic::{HarmonicConfig, HarmonicEngine};

pub struct HarmonicNode {
    engine: HarmonicEngine,
}

impl HarmonicNode {
    pub fn new(drive: f32, mix: f32, even_amount: f32, odd_amount: f32) -> Self {
        Self {
            engine: HarmonicEngine::new(HarmonicConfig {
                drive,
                drive_compensation: 1.0_f32,
                even_amount,
                odd_amount,
                mix,
            }),
        }
    }
}

const PARAMS: &[&str] = &["drive", "mix", "even_amount", "odd_amount"];

impl DspNode for HarmonicNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let mid = (*l + *r) * 0.5_f32;
            let side = (*l - *r) * 0.5_f32;
            let (m_out, s_out) = self.engine.process_frame(mid, side);
            *l = m_out + s_out;
            *r = m_out - s_out;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        match name {
            "drive" => self.engine.set_drive_compensation(value),
            "mix" => {}
            "even_amount" => {}
            "odd_amount" => {}
            _ => return false,
        }
        true
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.engine.reset();
    }

    fn node_type(&self) -> &'static str {
        "Harmonic"
    }
}
