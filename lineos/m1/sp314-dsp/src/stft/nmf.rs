pub const N_COMPONENTS: usize = 3;
pub const N_ITER:       usize = 30;
const EPS:      f32 = 1e-10_f32;
const LAMBDA_H: f32 = 0.1;
const CONV_CHECK_INTERVAL: usize = 10;
const CONV_TOL: f32 = 1e-4;

/// Deterministic xorshift32 PRNG (seed=42)
fn xorshift32(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state as f32 / u32::MAX as f32
}

pub struct NmfEngine {
    pub n_components: usize,
    /// W: basis matrix [n_bins × n_components] row-major
    pub w: Vec<f32>,
    /// H: activation matrix [n_components × n_frames] row-major
    pub h: Vec<f32>,
}

impl NmfEngine {

    pub fn new(n_components: usize) -> Self {
        Self { n_components, w: Vec::new(), h: Vec::new() }
    }

    pub fn default() -> Self {
        Self::new(N_COMPONENTS)
    }

    /// Run NMF on magnitude spectrogram.
    /// frames: &[Vec<f32>] — outer=time (n_frames), inner=freq (n_bins)
    pub fn fit(&mut self, frames: &[Vec<f32>]) {
        let n_frames = frames.len();
        let n_bins   = if n_frames > 0 { frames[0].len() } else { 0 };
        let k        = self.n_components;

        // Initialize W and H with fixed seed=42
        let mut seed: u32 = 42;
        let mut w = vec![0.0_f32; n_bins * k];
        let mut h = vec![0.0_f32; k * n_frames];
        for x in w.iter_mut() { *x = xorshift32(&mut seed) + EPS; }
        for x in h.iter_mut() { *x = xorshift32(&mut seed) + EPS; }

        // Pre-allocate V_approx buffer (reused each iteration)
        let mut v_approx = vec![0.0_f32; n_bins * n_frames];

        let mut prev_error = f32::MAX;
        for iter in 0..N_ITER {
            // Step 1: V_approx = W * H
            for b in 0..n_bins {
                for f in 0..n_frames {
                    let mut sum = 0.0_f32;
                    for c in 0..k {
                        sum += w[b * k + c] * h[c * n_frames + f];
                    }
                    v_approx[b * n_frames + f] = sum;
                }
            }

            // Step 2: Update H: H *= (W^T * V) / (W^T * V_approx + EPS)
            for c in 0..k {
                for f in 0..n_frames {
                    let mut num = 0.0_f32;
                    let mut den = 0.0_f32;
                    for b in 0..n_bins {
                        let w_bc = w[b * k + c];
                        num += w_bc * frames[f][b];
                        den += w_bc * v_approx[b * n_frames + f];
                    }
                    h[c * n_frames + f] *= num / (den + LAMBDA_H + EPS);
                }
            }

            // Step 3: Recompute V_approx with updated H
            for b in 0..n_bins {
                for f in 0..n_frames {
                    let mut sum = 0.0_f32;
                    for c in 0..k {
                        sum += w[b * k + c] * h[c * n_frames + f];
                    }
                    v_approx[b * n_frames + f] = sum;
                }
            }

            // Step 4: Update W: W *= (V * H^T) / (V_approx * H^T + EPS)
            for b in 0..n_bins {
                for c in 0..k {
                    let mut num = 0.0_f32;
                    let mut den = 0.0_f32;
                    for f in 0..n_frames {
                        let h_cf = h[c * n_frames + f];
                        num += frames[f][b] * h_cf;
                        den += v_approx[b * n_frames + f] * h_cf;
                    }
                    w[b * k + c] *= num / (den + EPS);
                }
            }

            // Normalize W columns (L1), absorb scale into H rows
            // Prevents scale ambiguity accumulating over iterations (NMF upgrade 1)
            for c in 0..k {
                let col_sum: f32 = (0..n_bins)
                    .map(|b| w[b * k + c])
                    .sum::<f32>()
                    .max(EPS);
                for b in 0..n_bins {
                    w[b * k + c] /= col_sum;
                }
                for f in 0..n_frames {
                    h[c * n_frames + f] *= col_sum;
                }
            }

            // Convergence check every CONV_CHECK_INTERVAL iterations
            if iter % CONV_CHECK_INTERVAL == 0 && iter > 0 {
                let mut sum_sq = 0.0_f32;
                for b in 0..n_bins {
                    for f in 0..n_frames {
                        let diff = frames[f][b] - v_approx[b * n_frames + f];
                        sum_sq += diff * diff;
                    }
                }
                let error = libm::sqrtf(sum_sq);
                let rel_improvement = (prev_error - error) / prev_error.max(EPS);
                if rel_improvement < CONV_TOL {
                    break;
                }
                prev_error = error;
            }
        }

        self.w = w;
        self.h = h;
    }

    /// Spectral centroid per component.
    pub fn centroids(&self, n_bins: usize) -> Vec<f32> {
        let k = self.n_components;
        let mut result = vec![0.0_f32; k];
        for c in 0..k {
            let mut num = 0.0_f32;
            let mut den = 0.0_f32;
            for b in 0..n_bins {
                let weight = self.w[b * k + c];
                num += b as f32 * weight;
                den += weight;
            }
            result[c] = if den > EPS { num / den } else { 0.0 };
        }
        result
    }

    /// Wiener-style soft mask for one component.
    /// Returns Vec<Vec<f32>> [n_frames][n_bins]
    pub fn component_mask(
        &self,
        component: usize,
        n_bins: usize,
        n_frames: usize,
    ) -> Vec<Vec<f32>> {
        let k = self.n_components;
        let mut mask = vec![vec![0.0_f32; n_bins]; n_frames];
        for f in 0..n_frames {
            for b in 0..n_bins {
                let target = self.w[b * k + component]
                           * self.h[component * n_frames + f];
                let mut total = 0.0_f32;
                for c in 0..k {
                    total += self.w[b * k + c]
                           * self.h[c * n_frames + f];
                }
                mask[f][b] = target / (total + EPS);
            }
        }
        mask
    }
}

/// Find the most spectrally diverse window in the signal.
/// Uses spectral flux to identify the region with maximum
/// sonic variation — best training data for NMF stem learning.
/// 
/// Returns (start_sample, end_sample).
/// Same input → same output always. INV-AB-1 preserved.
/// Determinism: max_by returns FIRST maximum on tie — no randomness.
pub fn find_most_diverse_window(
    signal:      &[f32],
    sample_rate: u32,
    window_sec:  f32,
) -> (usize, usize) {
    use crate::stft::spectral_flux::SpectralFluxDetector;
    use crate::stft::HOP_SIZE;

    // Edge case: track shorter than window
    let window_samples = (window_sec * sample_rate as f32) as usize;
    if signal.len() <= window_samples {
        return (0, signal.len());
    }

    // Compute spectral flux
    let mut detector = SpectralFluxDetector::new();
    let (flux, _) = detector.detect(signal);

    // Edge case: flat signal (silence)
    if flux.iter().all(|&f| f < 1e-6) {
        return find_highest_energy_chunk_window(signal, sample_rate, window_sec);
    }

    // Find window with maximum cumulative flux
    let window_frames = (window_sec * sample_rate as f32 / HOP_SIZE as f32) as usize;
    
    let best_start_frame = flux
        .windows(window_frames.max(1))
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            let sum_a: f32 = a.iter().sum();
            let sum_b: f32 = b.iter().sum();
            sum_a.partial_cmp(&sum_b).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0);

    let start = best_start_frame * HOP_SIZE;
    let end = (start + window_samples).min(signal.len());
    (start, end)
}

/// Fallback: energy-based selection (for flat/silent signals).
fn find_highest_energy_chunk_window(
    signal:      &[f32],
    sample_rate: u32,
    window_sec:  f32,
) -> (usize, usize) {
    let window_samples = (window_sec * sample_rate as f32) as usize;
    if signal.len() <= window_samples {
        return (0, signal.len());
    }
    let start = crate::pipeline::autotune::find_highest_energy_chunk(
        signal, signal, // mono — pass same slice
    );
    let end = (start + window_samples).min(signal.len());
    (start, end)
}

#[cfg(test)]
mod diverse_window_tests {
    use super::*;

    #[test]
    fn same_input_same_output() {
        let signal: Vec<f32> = (0..96000).map(|i| (i as f32 * 0.01).sin()).collect();
        let r1 = find_most_diverse_window(&signal, 48000, 10.0);
        let r2 = find_most_diverse_window(&signal, 48000, 10.0);
        assert_eq!(r1, r2, "INV-AB-1: must be deterministic");
    }

    #[test]
    fn short_track_returns_full() {
        let signal = vec![0.1f32; 48000]; // 1 second
        let (start, end) = find_most_diverse_window(&signal, 48000, 10.0);
        assert_eq!(start, 0);
        assert_eq!(end, signal.len());
    }

    #[test]
    fn result_within_bounds() {
        let signal: Vec<f32> = (0..480000).map(|i| (i as f32 * 0.001).sin()).collect();
        let (start, end) = find_most_diverse_window(&signal, 48000, 10.0);
        assert!(start < end);
        assert!(end <= signal.len());
        assert!(end - start <= 480001);
    }
}
