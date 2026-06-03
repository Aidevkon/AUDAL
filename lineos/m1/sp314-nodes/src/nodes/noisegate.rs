use crate::node::DspNode;

#[derive(Debug, Clone, Copy, PartialEq)]
enum GateState {
    Closed,
    Attack,
    Hold,
    Release,
}

pub struct NoiseGateNode {
    sample_rate: u32,
    threshold_db: f32,
    attack_ms: f32,
    hold_ms: f32,
    release_ms: f32,

    state: GateState,
    current_gain: f32,
    hold_counter: u32,
    env: f32,
}

impl NoiseGateNode {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            threshold_db: -60.0,
            attack_ms: 5.0,
            hold_ms: 50.0,
            release_ms: 100.0,
            state: GateState::Closed,
            current_gain: 0.0001,
            hold_counter: 0,
            env: 0.0,
        }
    }

    pub fn set_params(&mut self, threshold_db: f32, attack_ms: f32, hold_ms: f32, release_ms: f32) {
        self.threshold_db = threshold_db;
        self.attack_ms = attack_ms;
        self.hold_ms = hold_ms;
        self.release_ms = release_ms;
    }
}

impl DspNode for NoiseGateNode {
    fn process_stereo(&mut self, left: &mut [f32], right: &mut [f32]) {
        let threshold_lin = 10.0_f32.powf(self.threshold_db / 20.0);
        let sr = self.sample_rate as f32;
        let attack_coef = f32::exp(-1.0 / (self.attack_ms * 0.001 * sr).max(1.0));
        let release_coef = f32::exp(-1.0 / (self.release_ms * 0.001 * sr).max(1.0));
        let hold_samples = (self.hold_ms * 0.001 * sr) as u32;

        for i in 0..left.len() {
            let abs_sig = left[i].abs().max(right[i].abs());
            
            // Fast attack, slow release peak detector for envelope
            if abs_sig > self.env {
                self.env = abs_sig;
            } else {
                let env_release = f32::exp(-1.0 / (0.050 * sr)); // 50ms env release
                self.env = self.env * env_release + abs_sig * (1.0 - env_release);
            }

            match self.state {
                GateState::Closed => {
                    self.current_gain = 0.0001;
                    if self.env > threshold_lin {
                        self.state = GateState::Attack;
                    }
                }
                GateState::Attack => {
                    self.current_gain = self.current_gain * attack_coef + 1.0 * (1.0 - attack_coef);
                    if self.env < threshold_lin {
                        self.state = GateState::Hold;
                        self.hold_counter = hold_samples;
                    }
                }
                GateState::Hold => {
                    self.current_gain = 1.0;
                    if self.env > threshold_lin {
                        self.state = GateState::Attack; // Keep open
                    } else {
                        if self.hold_counter > 0 {
                            self.hold_counter -= 1;
                        } else {
                            self.state = GateState::Release;
                        }
                    }
                }
                GateState::Release => {
                    self.current_gain = self.current_gain * release_coef + 0.0001 * (1.0 - release_coef);
                    if self.env > threshold_lin {
                        self.state = GateState::Attack;
                    } else if self.current_gain <= 0.0002 {
                        self.current_gain = 0.0001;
                        self.state = GateState::Closed;
                    }
                }
            }

            left[i] *= self.current_gain;
            right[i] *= self.current_gain;
        }
    }

    fn set_parameter(&mut self, name: &str, value: f32) {
        match name {
            "threshold_db" => self.threshold_db = value,
            "attack_ms" => self.attack_ms = value,
            "hold_ms" => self.hold_ms = value,
            "release_ms" => self.release_ms = value,
            _ => {}
        }
    }

    fn get_output(&self, name: &str) -> Option<f32> {
        match name {
            "gain" => Some(self.current_gain),
            _ => None,
        }
    }

    fn reset(&mut self) {
        self.state = GateState::Closed;
        self.current_gain = 0.0001;
        self.hold_counter = 0;
        self.env = 0.0;
    }

    fn node_type(&self) -> &'static str {
        "NoiseGate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bypasses_loud_signal() {
        let mut gate = NoiseGateNode::new(48000);
        gate.set_params(-20.0, 1.0, 10.0, 10.0);
        let mut left = vec![0.5; 4800];
        let mut right = vec![0.5; 4800];
        gate.process_stereo(&mut left, &mut right);
        
        // Signal is above -20dB (0.1), so gain should go to 1.0 and signal should pass
        // Check end of buffer
        assert!((left[4799] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn attenuates_silence() {
        let mut gate = NoiseGateNode::new(48000);
        gate.set_params(-20.0, 1.0, 10.0, 10.0);
        let mut left = vec![0.001; 4800]; // well below -20dB
        let mut right = vec![0.001; 4800];
        gate.process_stereo(&mut left, &mut right);
        
        // Signal is quiet, should be attenuated
        assert!(left[4799] < 0.001 * 0.1);
    }

    #[test]
    fn respects_hold_time_before_closing() {
        let mut gate = NoiseGateNode::new(48000);
        // 50ms hold = 2400 samples
        gate.set_params(-20.0, 1.0, 50.0, 10.0);
        
        // 1. Loud signal to open gate
        let mut left = vec![0.5; 480];
        let mut right = vec![0.5; 480];
        gate.process_stereo(&mut left, &mut right);
        
        // 2. Sudden silence, length < hold_time (e.g. 1000 samples)
        let mut left2 = vec![0.0; 1000];
        let mut right2 = vec![0.0; 1000];
        left2[500] = 1.0; // dummy transient to check if gain is still 1.0
        gate.process_stereo(&mut left2, &mut right2);
        
        // Since we are in hold phase, gain should still be 1.0
        assert!((left2[500] - 1.0).abs() < 1e-4);
    }
}
