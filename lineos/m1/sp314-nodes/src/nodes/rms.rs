use crate::node::DspNode;
use libm::{expf, powf, sqrtf};

pub struct RmsDetectorNode {
    envelope_l: f32,
    envelope_r: f32,
    attack_coef: f32,
    release_coef: f32,
    threshold_linear: f32,

    // Parameters
    threshold_db: f32,
    attack_ms: f32,
    release_ms: f32,
    sample_rate: f32,
}

impl RmsDetectorNode {
    pub fn new(attack_ms: f32, release_ms: f32, threshold_db: f32, sample_rate: f32) -> Self {
        let mut node = Self {
            envelope_l: 0.0,
            envelope_r: 0.0,
            attack_coef: 0.0,
            release_coef: 0.0,
            threshold_linear: 0.0,
            threshold_db,
            attack_ms,
            release_ms,
            sample_rate,
        };
        node.recompute();
        node
    }

    fn recompute(&mut self) {
        self.attack_coef = 1.0 - expf(-1.0 / (self.attack_ms * 0.001 * self.sample_rate));
        self.release_coef = 1.0 - expf(-1.0 / (self.release_ms * 0.001 * self.sample_rate));
        self.threshold_linear = powf(10.0, self.threshold_db / 20.0);
    }
}

const PARAMS: &[&str] = &["threshold_db", "attack_ms", "release_ms"];

impl DspNode for RmsDetectorNode {
    fn param_names(&self) -> &'static [&'static str] {
        PARAMS
    }
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter().zip(right.iter()) {
            let sq_l = *l * *l;
            let sq_r = *r * *r;

            let coef_l = if sq_l > self.envelope_l {
                self.attack_coef
            } else {
                self.release_coef
            };
            let coef_r = if sq_r > self.envelope_r {
                self.attack_coef
            } else {
                self.release_coef
            };

            self.envelope_l += (sq_l - self.envelope_l) * coef_l;
            self.envelope_r += (sq_r - self.envelope_r) * coef_r;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) -> bool {
        if !self.has_parameter(name) {
            return false;
        }
        let mut changed = false;
        match name {
            "threshold_db" => {
                if self.threshold_db != value {
                    self.threshold_db = value;
                    changed = true;
                }
            }
            "attack_ms" => {
                if self.attack_ms != value {
                    self.attack_ms = value;
                    changed = true;
                }
            }
            "release_ms" => {
                if self.release_ms != value {
                    self.release_ms = value;
                    changed = true;
                }
            }
            _ => return false,
        }

        if changed {
            self.recompute();
        }
        true
    }

    fn get_output(&self, name: &str) -> Option<f32> {
        let env_l = sqrtf(self.envelope_l);
        let env_r = sqrtf(self.envelope_r);
        let envelope = if env_l > env_r { env_l } else { env_r }; // max of L/R

        if name == "envelope" {
            Some(envelope)
        } else if name == "gain_reduction" {
            if envelope > self.threshold_linear {
                Some(self.threshold_linear / envelope)
            } else {
                Some(1.0)
            }
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.envelope_l = 0.0;
        self.envelope_r = 0.0;
    }

    fn node_type(&self) -> &'static str {
        "RMS_Detector"
    }
}
