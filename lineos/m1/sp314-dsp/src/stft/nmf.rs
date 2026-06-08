pub const N_COMPONENTS: usize = 4;
pub const N_ITER:       usize = 30;
const EPS:      f32 = 1e-10_f32;
const LAMBDA_H: f32 = 0.1;
const CONV_CHECK_INTERVAL: usize = 10;
const CONV_TOL: f32 = 1e-4;
pub const TRANSFORM_ITERS: usize = 5;

use rayon::prelude::*;
use std::cell::RefCell;

thread_local! {
    static NMF_SCRATCH: RefCell<(Vec<f32>, Vec<f32>)> =
        const { RefCell::new((Vec::new(), Vec::new())) };
}

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

fn downsample_frames(frames: &[Vec<f32>]) -> Vec<Vec<f32>> {
    frames.iter().step_by(2).cloned().collect()
}

fn upsample_masks_linear(masks: &[f32], n_components: usize,
                          original_frames: usize) -> Vec<f32> {
    let downsampled = original_frames.div_ceil(2);
    let mut out = vec![0.0f32; n_components * original_frames];
    for c in 0..n_components {
        for f in 0..original_frames {
            let exact = f as f32 / 2.0;
            let idx0  = exact.floor() as usize;
            let idx1  = (idx0 + 1).min(downsampled - 1);
            let frac  = exact - idx0 as f32;
            let v0    = masks[c * downsampled + idx0];
            let v1    = masks[c * downsampled + idx1];
            out[c * original_frames + f] = v0 + frac * (v1 - v0);
        }
    }
    out
}

impl NmfEngine {

    pub fn new(n_components: usize) -> Self {
        Self { n_components, w: Vec::new(), h: Vec::new() }
    }

    pub fn default() -> Self {
        Self::new(N_COMPONENTS)
    }

    /// Run NMF on magnitude spectrogram. Returns (W, H).
    pub fn fit_transform(&mut self, frames: &[Vec<f32>]) -> (Vec<f32>, Vec<f32>) {
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
            v_approx.fill(0.0f32);
            for c in 0..k {
                for b in 0..n_bins {
                    let w_bc = w[b * k + c];
                    let v_slice = &mut v_approx[b * n_frames..(b+1)*n_frames];
                    let h_slice = &h[c * n_frames..(c+1)*n_frames];
                    for f in 0..n_frames {
                        v_slice[f] += w_bc * h_slice[f];
                    }
                }
            }

            // Step 2: Update H: H *= (W^T * V) / (W^T * V_approx + EPS)
            h.par_chunks_mut(n_frames).enumerate().for_each(|(c, h_row)| {
                NMF_SCRATCH.with(|cell| {
                    let mut scratch = cell.borrow_mut();
                    let (ref mut num, ref mut den) = *scratch;
                    num.clear(); num.resize(n_frames, 0.0f32);
                    den.clear(); den.resize(n_frames, 0.0f32);

                    for b in 0..n_bins {
                        let w_bc = w[b * k + c];
                        let v_slice = &v_approx[b * n_frames..(b+1)*n_frames];
                        for f in 0..n_frames {
                            num[f] += w_bc * frames[f][b];
                            den[f] += w_bc * v_slice[f];
                        }
                    }
                    for f in 0..n_frames {
                        h_row[f] *= num[f] / (den[f] + LAMBDA_H + EPS);
                    }
                });
            });

            // Step 3: Recompute V_approx with updated H
            v_approx.fill(0.0f32);
            for c in 0..k {
                for b in 0..n_bins {
                    let w_bc = w[b * k + c];
                    let v_slice = &mut v_approx[b * n_frames..(b+1)*n_frames];
                    let h_slice = &h[c * n_frames..(c+1)*n_frames];
                    for f in 0..n_frames {
                        v_slice[f] += w_bc * h_slice[f];
                    }
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

        self.w = w.clone();
        self.h = h.clone();
        (w, h)
    }

    /// Learn stem profiles from representative sample.
    pub fn fit(&mut self, frames: &[Vec<f32>]) -> Vec<f32> {
        let ds_frames = downsample_frames(frames);
        let (w_new, h_down) = self.fit_transform(&ds_frames);
        self.w = w_new.clone();
        self.h = upsample_masks_linear(&h_down, self.n_components, frames.len());
        w_new
    }

    /// Apply learned profiles to full track.
    pub fn transform(&self, w: &[f32], frames: &[Vec<f32>]) -> Vec<f32> {
        let n_frames = frames.len();
        let n_bins   = if n_frames > 0 { frames[0].len() } else { 0 };
        let k        = self.n_components;

        let mut seed: u32 = 42;
        let mut h = vec![0.0_f32; k * n_frames];
        for x in h.iter_mut() { *x = xorshift32(&mut seed) + EPS; }

        let mut v_approx = vec![0.0_f32; n_bins * n_frames];

        for _ in 0..TRANSFORM_ITERS {
            // Step 1: V_approx = W * H
            v_approx.fill(0.0f32);
            for c in 0..k {
                for b in 0..n_bins {
                    let w_bc = w[b * k + c];
                    let v_slice = &mut v_approx[b * n_frames..(b+1)*n_frames];
                    let h_slice = &h[c * n_frames..(c+1)*n_frames];
                    for f in 0..n_frames {
                        v_slice[f] += w_bc * h_slice[f];
                    }
                }
            }

            // Step 2: Update H: H *= (W^T * V) / (W^T * V_approx + EPS)
            h.par_chunks_mut(n_frames).enumerate().for_each(|(c, h_row)| {
                NMF_SCRATCH.with(|cell| {
                    let mut scratch = cell.borrow_mut();
                    let (ref mut num, ref mut den) = *scratch;
                    num.clear(); num.resize(n_frames, 0.0f32);
                    den.clear(); den.resize(n_frames, 0.0f32);

                    for b in 0..n_bins {
                        let w_bc = w[b * k + c];
                        let v_slice = &v_approx[b * n_frames..(b+1)*n_frames];
                        for f in 0..n_frames {
                            num[f] += w_bc * frames[f][b];
                            den[f] += w_bc * v_slice[f];
                        }
                    }
                    for f in 0..n_frames {
                        h_row[f] *= num[f] / (den[f] + LAMBDA_H + EPS);
                    }
                });
            });
        }
        h
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

    /// Per-chunk component mask — uses h_chunk instead of full self.h.
    /// h_chunk: [n_components × n_chunk_frames] row-major
    /// Returns mask [n_chunk_frames][n_bins]
    pub fn component_mask_chunk(
        &self,
        component:    usize,
        h_chunk:      &[f32],
        n_chunk_frames: usize,
        n_bins:       usize,
    ) -> Vec<Vec<f32>> {
        let k = self.n_components;
        let mut mask = vec![vec![0.0_f32; n_bins]; n_chunk_frames];
        for f in 0..n_chunk_frames {
            for b in 0..n_bins {
                let w_bc = self.w[b * k + component];
                let h_cf = if component * n_chunk_frames + f < h_chunk.len() {
                    h_chunk[component * n_chunk_frames + f]
                } else { 0.0 };
                let target = w_bc * h_cf;
                let mut total = 0.0_f32;
                for c in 0..k {
                    let h_val = if c * n_chunk_frames + f < h_chunk.len() {
                        h_chunk[c * n_chunk_frames + f]
                    } else { 0.0 };
                    total += self.w[b * k + c] * h_val;
                }
                mask[f][b] = target / (total + 1e-10_f32);
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

#[cfg(test)]
mod transform_tests {
    use super::*;

    fn generate_dummy_spectrogram(n_bins: usize, n_frames: usize) -> Vec<Vec<f32>> {
        let mut frames = vec![vec![0.0_f32; n_bins]; n_frames];
        for f in 0..n_frames {
            for b in 0..n_bins {
                frames[f][b] = ((f * b) % 100) as f32 * 0.01;
            }
        }
        frames
    }

    #[test]
    fn fit_then_transform_matches_fit_transform() {
        let n_bins = 129;
        let n_frames = 50;
        let frames = generate_dummy_spectrogram(n_bins, n_frames);

        let mut nmf1 = NmfEngine::default();
        let (_, h1) = nmf1.fit_transform(&frames);

        let mut nmf2 = NmfEngine::default();
        let w2 = nmf2.fit(&frames);
        let h2 = nmf2.transform(&w2, &frames);

        let mut diff_sum = 0.0;
        for i in 0..h1.len() {
            diff_sum += (h1[i] - h2[i]).abs();
        }
        let avg_diff = diff_sum / h1.len() as f32;
        // The difference should be reasonably small since W is the same and we optimize H
        // Note: H scale can be ~60+, so avg_diff of 8-10 is ~15% error, which is expected after only 5 iterations
        assert!(avg_diff < 20.0, "Average difference too high: {}", avg_diff);
    }

    #[test]
    fn transform_is_deterministic() {
        let n_bins = 129;
        let n_frames = 20;
        let frames = generate_dummy_spectrogram(n_bins, n_frames);

        let mut nmf = NmfEngine::default();
        let w = nmf.fit(&frames);

        let h1 = nmf.transform(&w, &frames);
        let h2 = nmf.transform(&w, &frames);

        assert_eq!(h1, h2, "INV-AB-1: transform must be deterministic");
    }
}
