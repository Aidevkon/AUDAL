use crate::node::DspNode;
use sp314_dsp::compressor::core::{
    CompressorBandConfig, MultibandCompressor3, MultibandCompressor3Config,
};

pub struct MultibandCompressorNode {
    comp_l: MultibandCompressor3,
    comp_r: MultibandCompressor3,
}

fn default_band(
    threshold_db: f32,
    ratio: f32,
    attack_ms: f32,
    release_ms: f32,
    makeup_db: f32,
) -> CompressorBandConfig {
    CompressorBandConfig {
        threshold_db,
        ratio,
        knee_db: 2.0_f32,
        attack_ms,
        release_ms,
        makeup_db,
        crossover_hz: 0.0_f32, // unused in MultibandCompressor3
    }
}

fn default_config(f_low: f32, f_high: f32) -> MultibandCompressor3Config {
    MultibandCompressor3Config {
        f_low,
        f_high,
        low_config: default_band(-24.0, 3.0, 10.0, 150.0, 0.0),
        mid_config: default_band(-18.0, 2.5, 10.0, 100.0, 0.0),
        high_config: default_band(-20.0, 2.0, 5.0, 80.0, 0.0),
    }
}

impl MultibandCompressorNode {
    pub fn new(sample_rate: f32, f_low: f32, f_high: f32) -> Self {
        let sr = sample_rate as u32;
        Self {
            comp_l: MultibandCompressor3::new(default_config(f_low, f_high), sr),
            comp_r: MultibandCompressor3::new(default_config(f_low, f_high), sr),
        }
    }
}

const PARAMS: &[&str] = &[];

impl DspNode for MultibandCompressorNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        // Independent state per channel — no stereo crosstalk
        for l in left.iter_mut() {
            *l = self.comp_l.process(*l);
        }
        for r in right.iter_mut() {
            *r = self.comp_r.process(*r);
        }
    }

    fn set_parameter(&mut self, _name: &str, _value: f32) -> bool {
        // Future: f_low, f_high, per-band threshold/ratio
        false
    }

    fn get_output(&self, _name: &str) -> Option<f32> {
        None
    }

    fn reset(&mut self) {
        self.comp_l.reset();
        self.comp_r.reset();
    }

    fn node_type(&self) -> &'static str {
        "MultibandCompressor"
    }
}
