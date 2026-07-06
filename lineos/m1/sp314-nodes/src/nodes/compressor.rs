use crate::node::DspNode;
use libm::{fmaxf, fminf, powf};
use sp314_dsp::compressor::envelope::EnvelopeFollower;
use sp314_dsp::compressor::gain::compute_gain_reduction;

pub struct CompressorNode {
    env_l: EnvelopeFollower,
    env_r: EnvelopeFollower,
    threshold_db: f32,
    ratio: f32,
    knee_db: f32,
    attack_ms: f32,
    release_ms: f32,
    makeup_db: f32,
    sample_rate: f32,
    current_gr_db: f32,
}

impl CompressorNode {
    pub fn new(sample_rate: f32) -> Self {
        let attack_ms = 10.0;
        let release_ms = 100.0;
        Self {
            env_l: EnvelopeFollower::new(attack_ms, release_ms, sample_rate as u32),
            env_r: EnvelopeFollower::new(attack_ms, release_ms, sample_rate as u32),
            threshold_db: -20.0,
            ratio: 4.0,
            knee_db: 6.0,
            attack_ms,
            release_ms,
            makeup_db: 0.0,
            sample_rate,
            current_gr_db: 0.0,
        }
    }

    fn recompute_envelopes(&mut self) {
        // Re-create the envelope followers to update attack/release coefficients
        let sr = self.sample_rate as u32;
        // In order to not lose state, we could manually compute coeffs, but EnvelopeFollower
        // doesn't expose them. For now, replacing is fine since this happens between blocks.
        self.env_l = EnvelopeFollower::new(self.attack_ms, self.release_ms, sr);
        self.env_r = EnvelopeFollower::new(self.attack_ms, self.release_ms, sr);
    }
}

const PARAMS: &[&str] = &[
    "threshold_db",
    "ratio",
    "knee_db",
    "attack_ms",
    "release_ms",
    "makeup_db",
];

impl DspNode for CompressorNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let makeup_linear = powf(10.0, self.makeup_db / 20.0);

        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            // Unlinked stereo compressor
            let env_l_db = self.env_l.process(*l);
            let env_r_db = self.env_r.process(*r);

            let gr_l_db =
                compute_gain_reduction(env_l_db, self.threshold_db, self.ratio, self.knee_db);
            let gr_r_db =
                compute_gain_reduction(env_r_db, self.threshold_db, self.ratio, self.knee_db);

            let gr_db = fminf(gr_l_db, gr_r_db); // Stereo link (apply max reduction)
            self.current_gr_db = gr_db;

            let gr_linear = powf(10.0, gr_db / 20.0);

            *l = fmaxf(-2.0, fminf(2.0, *l * gr_linear * makeup_linear));
            *r = fmaxf(-2.0, fminf(2.0, *r * gr_linear * makeup_linear));
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        let mut env_changed = false;
        match name {
            "threshold_db" => self.threshold_db = value,
            "ratio" => self.ratio = value,
            "knee_db" => self.knee_db = value,
            "attack_ms" => {
                if self.attack_ms != value {
                    self.attack_ms = value;
                    env_changed = true;
                }
            }
            "release_ms" => {
                if self.release_ms != value {
                    self.release_ms = value;
                    env_changed = true;
                }
            }
            "makeup_db" => self.makeup_db = value,
            _ => return false,
        }

        if env_changed {
            self.recompute_envelopes();
        }
        true
    }

    fn get_output(&self, name: &str) -> Option<f32> {
        if name == "gain_reduction_db" {
            Some(self.current_gr_db)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.env_l.reset();
        self.env_r.reset();
        self.current_gr_db = 0.0;
    }

    fn node_type(&self) -> &'static str {
        "Compressor"
    }
}
