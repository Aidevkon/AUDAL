// src/restoration/cleaner.rs
// De-Hum + De-Ess chain.
// Constitutional: libm only. Attack/release coefficients computed once in new().

use super::biquad::{Biquad, FilterType};
use super::gate::NoiseGate;
use super::RestorationConfig;
use libm::{expf, fabsf, powf};

pub struct RestorationChain {
    // De-Hum: 50Hz, 100Hz, 150Hz notches
    notch_50:  Biquad,
    notch_100: Biquad,
    notch_150: Biquad,
    hum_enabled: bool,

    // De-Ess
    deess_hp:         Biquad,
    env_l:            f32,
    env_r:            f32,
    deess_threshold:  f32,
    deess_ratio:      f32,
    deess_enabled:    bool,

    // Low-Cut
    lowcut_hp: Biquad,
    lowcut_enabled: bool,

    // Noise Gate
    gate: NoiseGate,
    gate_enabled: bool,

    // Precomputed coefficients — computed ONCE in new(), never in process()
    attack_coef:  f32,
    release_coef: f32,
}

impl RestorationChain {
    pub fn new(sample_rate: f32, config: RestorationConfig, pad_db_shift: f32) -> Self {
        Self {
            // High Q (20.0) — strictly targets hum without affecting bass
            notch_50:  Biquad::new(FilterType::Notch, 50.0,  20.0, sample_rate),
            notch_100: Biquad::new(FilterType::Notch, 100.0, 20.0, sample_rate),
            notch_150: Biquad::new(FilterType::Notch, 150.0, 20.0, sample_rate),
            hum_enabled: config.hum_enabled,

            // De-Esser: detects harsh frequencies above 6kHz
            deess_hp:        Biquad::new(FilterType::HighPass, 6000.0, 0.707, sample_rate),
            env_l:           0.0,
            env_r:           0.0,
            deess_threshold: powf(10.0, -24.0 / 20.0), // -24 dBFS linear
            deess_ratio:     4.0,
            deess_enabled:   config.deess_enabled,

            // Low-Cut
            lowcut_hp: Biquad::new(FilterType::HighPass, 80.0, 0.707, sample_rate),
            lowcut_enabled: config.lowcut_enabled,

            // Gate
            gate: NoiseGate::new(sample_rate, pad_db_shift),
            gate_enabled: config.gate_enabled,

            // Precomputed once — 1ms attack, 50ms release
            attack_coef:  expf(-1.0 / (sample_rate * 0.001)),
            release_coef: expf(-1.0 / (sample_rate * 0.050)),
        }
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32]) {
        for i in 0..left.len() {
            let mut l = left[i];
            let mut r = right[i];

            // 1. First, remove low-frequency rumble so it doesn't trigger the gate
            if self.lowcut_enabled {
                let (l1, r1) = self.lowcut_hp.process_stereo(l, r);
                l = l1;
                r = r1;
            }

            // 2. Now apply the Noise Gate on the cleaned signal
            //    The gate detector sees only voice-range frequencies — not 30Hz truck rumble
            if self.gate_enabled {
                let (gl, gr) = self.gate.process_stereo(l, r);
                l = gl;
                r = gr;
            }

            // 3. De-Hum (static notch cascade)
            if self.hum_enabled {
                let (l1, r1) = self.notch_50.process_stereo(l, r);
                let (l2, r2) = self.notch_100.process_stereo(l1, r1);
                let (l3, r3) = self.notch_150.process_stereo(l2, r2);
                l = l3;
                r = r3;
            }

            // 2. De-Ess (dynamic HF sidechain)
            if self.deess_enabled {
                let (hf_l, hf_r) = self.deess_hp.process_stereo(l, r);

                // Left envelope follower
                let rect_l = fabsf(hf_l);
                self.env_l = if rect_l > self.env_l {
                    self.env_l + (rect_l - self.env_l) * (1.0 - self.attack_coef)
                } else {
                    self.env_l + (rect_l - self.env_l) * (1.0 - self.release_coef)
                };

                if self.env_l > self.deess_threshold {
                    let overshoot = self.env_l - self.deess_threshold;
                    let reduction = 1.0 - (overshoot / (overshoot + self.deess_threshold * self.deess_ratio));
                    l *= reduction;
                }

                // Right envelope follower
                let rect_r = fabsf(hf_r);
                self.env_r = if rect_r > self.env_r {
                    self.env_r + (rect_r - self.env_r) * (1.0 - self.attack_coef)
                } else {
                    self.env_r + (rect_r - self.env_r) * (1.0 - self.release_coef)
                };

                if self.env_r > self.deess_threshold {
                    let overshoot = self.env_r - self.deess_threshold;
                    let reduction = 1.0 - (overshoot / (overshoot + self.deess_threshold * self.deess_ratio));
                    r *= reduction;
                }
            }

            left[i] = l;
            right[i] = r;
        }
    }

    pub fn reset(&mut self) {
        self.notch_50.reset();
        self.notch_100.reset();
        self.notch_150.reset();
        self.deess_hp.reset();
        self.lowcut_hp.reset();
        self.gate.reset();
        self.env_l = 0.0;
        self.env_r = 0.0;
    }
}
