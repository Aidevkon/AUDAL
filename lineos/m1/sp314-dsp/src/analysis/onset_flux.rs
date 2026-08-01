//! Böck & Widmer 2013, clean-room, librosa oracle, NEVER derived from madmom source.
//! SuperFlux: Maximum Filter Vibrato Suppression for Onset Detection.

use super::superflux_weights::{MEL_BANDS, MEL_BASIS};

pub struct SuperFluxOnset {
    /// Past max-filtered spectrum at t - 1 (lag=1)
    prev_ref: [f32; MEL_BANDS],
    /// Gamma compression factor matching the oracle
    gamma: f32,
}

impl Default for SuperFluxOnset {
    fn default() -> Self {
        Self::new(10.0)
    }
}

impl SuperFluxOnset {
    pub fn new(gamma: f32) -> Self {
        Self {
            prev_ref: [0.0; MEL_BANDS],
            gamma,
        }
    }

    /// Process a single 1025-bin magnitude spectrum and return the onset strength.
    pub fn process(&mut self, stft_mag: &[f32]) -> f32 {
        let s_log = self.compute_log_bands(stft_mag);
        self.process_log_bands(&s_log)
    }

    /// Extracted for math-only oracle testing (Gate A).
    pub fn compute_log_bands(&self, stft_mag: &[f32]) -> [f32; MEL_BANDS] {
        debug_assert_eq!(stft_mag.len(), 1025);

        // 1. Apply mel filterbank
        let mut s_mel = [0.0_f32; MEL_BANDS];
        for i in 0..MEL_BANDS {
            let band = &MEL_BASIS[i];
            let mut sum = 0.0;
            for (j, &w) in band.weights.iter().enumerate() {
                sum += stft_mag[band.start_bin + j] * w;
            }
            s_mel[i] = sum;
        }

        // 2. Log compression: S_log = ln(1 + gamma * S_mel)
        let mut s_log = [0.0_f32; MEL_BANDS];
        for i in 0..MEL_BANDS {
            s_log[i] = (1.0 + self.gamma * s_mel[i]).ln();
        }

        s_log
    }

    /// Extracted for math-only oracle testing (Gate B).
    pub fn process_log_bands(&mut self, s_log: &[f32; MEL_BANDS]) -> f32 {
        // 3. Max-filter across frequency (size=3).
        let mut s_ref = [0.0_f32; MEL_BANDS];
        for i in 0..MEL_BANDS {
            let left = if i == 0 { s_log[1] } else { s_log[i - 1] };
            let center = s_log[i];
            let right = if i == MEL_BANDS - 1 {
                s_log[MEL_BANDS - 2]
            } else {
                s_log[i + 1]
            };

            let mut m = left;
            if center > m {
                m = center;
            }
            if right > m {
                m = right;
            }
            s_ref[i] = m;
        }

        // 4. Diff with lag=1, half-wave rectify, sum
        let mut onset_strength = 0.0;
        for i in 0..MEL_BANDS {
            let diff = s_log[i] - self.prev_ref[i];
            if diff > 0.0 {
                onset_strength += diff;
            }
        }

        // Librosa averages across frequency bands by default.
        onset_strength /= MEL_BANDS as f32;

        // Update state
        self.prev_ref = s_ref;

        onset_strength
    }
}
