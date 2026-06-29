use crate::node::DspNode;
use sp314_dsp::masking_eq::{MaskingAwareEQ, MaskingEQConfig};

/// Masking-aware dynamic EQ node.
/// Wraps MaskingAwareEQ and feeds it NMF
/// stem energy ratios for mud correction.
pub struct MaskingEqNode {
    eq: MaskingAwareEQ,
    stem_ratios: [f32; 5],
}

impl MaskingEqNode {
    pub fn new(config: MaskingEQConfig, sample_rate: u32) -> Self {
        let eq = MaskingAwareEQ::new(config, sample_rate).expect("Invalid MaskingEQ Config");
        Self {
            eq,
            stem_ratios: [0.0; 5],
        }
    }
}

impl DspNode for MaskingEqNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        self.eq.process_block(left, right, &self.stem_ratios);
    }

    fn update_features(&mut self, stem_ratios: &[f32; 5]) {
        self.stem_ratios = *stem_ratios;
    }

    fn set_parameter(&mut self, _name: &str, _value: f32) {}

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {}

    fn node_type(&self) -> &'static str {
        "MaskingEQ"
    }
}
