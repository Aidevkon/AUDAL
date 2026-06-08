use crate::stft::{StftEngine, N_BINS};

pub const FLUX_THRESHOLD:    f32 = 0.01_f32;
pub const FLUX_MIN_DISTANCE: usize = 10;  // frames (~100ms)

pub struct SpectralFluxDetector {
    engine:       StftEngine,
}

impl Default for SpectralFluxDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl SpectralFluxDetector {
    pub fn new() -> Self {
        Self {
            engine:          StftEngine::new(),
        }
    }

    /// Compute spectral flux and detect beats.
    /// Returns: (flux_normalized: Vec<f32>,
    ///           beats: Vec<usize>)
    /// beats contains frame indices of detected onsets.
    pub fn detect(&mut self, signal: &[f32])
        -> (Vec<f32>, Vec<usize>)
    {
        // Forward STFT
        let (frames, n_frames) = self.engine.forward(signal);

        // Compute flux per frame
        let mut flux = vec![0.0_f32; n_frames];
        let mut prev = vec![0.0_f32; N_BINS];

        for (t, frame) in frames.iter().enumerate() {
            let mut frame_flux = 0.0_f32;
            for b in 0..N_BINS {
                let mag = libm::sqrtf(
                    frame[b].re * frame[b].re
                    + frame[b].im * frame[b].im
                );
                let diff = mag - prev[b];
                if diff > 0.0_f32 {
                    frame_flux += diff;
                }
                prev[b] = mag;
            }
            flux[t] = frame_flux;
        }

        // Normalize flux to [0, 1]
        let flux_max = flux.iter()
            .cloned()
            .fold(0.0_f32, f32::max);

        let mut flux_norm = vec![0.0_f32; n_frames];
        if flux_max > 1e-8_f32 {
            for t in 0..n_frames {
                flux_norm[t] = flux[t] / flux_max;
            }
        }

        // Peak picking — exact same logic as Python fixture:
        // local max > threshold with min_distance
        let mut beats     = Vec::new();

        // Use signed arithmetic for min_distance check
        for t in 1..n_frames.saturating_sub(1) {
            if flux_norm[t] >= FLUX_THRESHOLD
                && flux_norm[t] > flux_norm[t - 1]
                && flux_norm[t] > flux_norm[t + 1]
            {
                let dist = if beats.is_empty() {
                    FLUX_MIN_DISTANCE + 1
                } else {
                    t - *beats.last().unwrap()
                };
                if dist >= FLUX_MIN_DISTANCE {
                    beats.push(t);
                }
            }
        }

        (flux_norm, beats)
    }
}
