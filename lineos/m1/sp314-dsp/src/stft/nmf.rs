pub const N_COMPONENTS: usize = 3;
pub const N_ITER:       usize = 100;
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
    /// W: basis matrix [n_bins × N_COMPONENTS] row-major
    pub w: Vec<f32>,
    /// H: activation matrix [N_COMPONENTS × n_frames] row-major
    pub h: Vec<f32>,
}

impl NmfEngine {

    /// Run NMF on magnitude spectrogram.
    /// frames: &[Vec<f32>] — outer=time (n_frames), inner=freq (n_bins)
    pub fn fit(frames: &[Vec<f32>]) -> Self {
        let n_frames = frames.len();
        let n_bins   = if n_frames > 0 { frames[0].len() } else { 0 };
        let k        = N_COMPONENTS;

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

        Self { w, h }
    }

    /// Spectral centroid per component.
    pub fn centroids(&self, n_bins: usize) -> Vec<f32> {
        let k = N_COMPONENTS;
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
        let k = N_COMPONENTS;
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
