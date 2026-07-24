// src/restoration/cleaner.rs
// De-Hum + Split-Band De-Ess chain.
// Constitutional: libm only. Attack/release coefficients computed once in new().

use super::biquad::{Biquad, FilterType};
use super::gate::NoiseGate;
use super::RestorationConfig;
use crate::compressor::crossover::CrossoverLR4;
use libm::{expf, fabsf, powf};

pub struct RestorationChain {
    // De-Hum: 50Hz, 100Hz, 150Hz notches
    notch_50: Biquad,
    notch_100: Biquad,
    notch_150: Biquad,
    hum_enabled: bool,

    // Split-Band De-Ess (LR4 crossover at 6kHz)
    // Two crossovers (one per channel) split the signal into low + high bands.
    // Only the high band is ducked, preserving low/mid content (no music-bed pumping).
    // A single linked envelope prevents stereo image wander.
    deess_xover_l: CrossoverLR4,
    deess_xover_r: CrossoverLR4,
    env_deess: f32,
    deess_threshold: f32,
    deess_ratio: f32,
    deess_enabled: bool,

    // Low-Cut
    lowcut_hp: Biquad,
    lowcut_enabled: bool,

    // Noise Gate
    gate: NoiseGate,
    gate_enabled: bool,

    // Precomputed coefficients — computed ONCE in new(), never in process()
    attack_coef: f32,
    release_coef: f32,
}

impl RestorationChain {
    // gate_threshold_db: passed by orchestrator (which resolves noise_floor_dbfs.unwrap_or(-45.0)).
    // The DSP core never knows the -45 default. pad_db_shift remains for headroom compensation.
    pub fn new(
        sample_rate: f32,
        config: RestorationConfig,
        pad_db_shift: f32,
        gate_threshold_db: f32,
    ) -> Self {
        Self {
            // High Q (20.0) — strictly targets hum without affecting bass
            notch_50: Biquad::new(FilterType::Notch, 50.0, 20.0, sample_rate),
            notch_100: Biquad::new(FilterType::Notch, 100.0, 20.0, sample_rate),
            notch_150: Biquad::new(FilterType::Notch, 150.0, 20.0, sample_rate),
            hum_enabled: config.hum_enabled,

            // Split-Band De-Esser: LR4 crossover at 6kHz (one per channel).
            // CrossoverLR4::new precomputes all biquad coefficients — zero
            // per-sample allocation or trig calls.
            deess_xover_l: CrossoverLR4::new(6000.0, sample_rate as u32),
            deess_xover_r: CrossoverLR4::new(6000.0, sample_rate as u32),
            env_deess: 0.0,
            deess_threshold: powf(10.0, -24.0 / 20.0), // -24 dBFS linear
            deess_ratio: 4.0,
            deess_enabled: config.deess_enabled,

            // Low-Cut
            lowcut_hp: Biquad::new(FilterType::HighPass, 80.0, 0.707, sample_rate),
            lowcut_enabled: config.lowcut_enabled,

            // Gate
            gate: NoiseGate::new(sample_rate, pad_db_shift, gate_threshold_db),
            gate_enabled: config.gate_enabled,

            // Precomputed once — 1ms attack, 50ms release
            attack_coef: expf(-1.0 / (sample_rate * 0.001)),
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

            // 4. Split-Band De-Ess (LR4 crossover at 6kHz)
            // SPLIT-BAND: only the HF band is ducked — low/mid content passes
            //   through untouched, so music beds don't pump.
            // LINKED STEREO: one envelope from max(|high_l|, |high_r|) drives
            //   identical reduction on both channels — no image wander.
            if self.deess_enabled {
                let (low_l, high_l) = self.deess_xover_l.process(l);
                let (low_r, high_r) = self.deess_xover_r.process(r);

                // Linked stereo detection: max of both HF bands
                let rect = fabsf(high_l).max(fabsf(high_r));
                self.env_deess = if rect > self.env_deess {
                    self.env_deess + (rect - self.env_deess) * (1.0 - self.attack_coef)
                } else {
                    self.env_deess + (rect - self.env_deess) * (1.0 - self.release_coef)
                };

                // Single reduction applied identically to both HF bands
                let reduction = if self.env_deess > self.deess_threshold {
                    let overshoot = self.env_deess - self.deess_threshold;
                    1.0 - (overshoot / (overshoot + self.deess_threshold * self.deess_ratio))
                } else {
                    1.0
                };

                // Reconstruct: low band untouched + ducked high band
                l = low_l + high_l * reduction;
                r = low_r + high_r * reduction;
            }

            left[i] = l;
            right[i] = r;
        }
    }

    pub fn reset(&mut self) {
        self.notch_50.reset();
        self.notch_100.reset();
        self.notch_150.reset();
        self.deess_xover_l.reset();
        self.deess_xover_r.reset();
        self.lowcut_hp.reset();
        self.gate.reset();
        self.env_deess = 0.0;
    }
}
