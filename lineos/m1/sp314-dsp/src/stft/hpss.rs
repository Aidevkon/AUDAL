pub const L_HARM: usize = 17;  // time frames window
pub const L_PERC: usize = 17;  // frequency bins window

/// Compute 1D sliding median with edge clamping (nearest mode).
/// input:  slice of f32 values
/// window: odd window size
/// output: Vec<f32> same length as input
fn sliding_median(input: &[f32], window: usize) -> Vec<f32> {
    let n    = input.len();
    let half = window / 2;
    let mut result = vec![0.0_f32; n];
    let mut buf    = vec![0.0_f32; window];

    for i in 0..n {
        // Fill window with edge clamping (nearest)
        for k in 0..window {
            let idx = if k < half {
                // Left side — clamp to 0
                if i + k < half { 0 }
                else { i + k - half }
            } else {
                let j = i + k - half;
                j.min(n - 1)  // Right side — clamp to n-1
            };
            buf[k] = input[idx];
        }
        // Sort and take middle element
        // Insertion sort (small window — efficient)
        for j in 1..window {
            let key = buf[j];
            let mut m = j;
            while m > 0 && buf[m - 1] > key {
                buf[m] = buf[m - 1];
                m -= 1;
            }
            buf[m] = key;
        }
        result[i] = buf[half];
    }
    result
}

pub struct HpssProcessor;

impl HpssProcessor {
    pub fn new() -> Self { Self }

    /// Apply HPSS to a magnitude spectrogram.
    /// Input:  magnitudes[n_frames][n_bins] (flat: row-major)
    /// Output: (harmonic_mask, percussive_mask)
    ///         each is Vec<f32> of length n_frames * n_bins
    pub fn process(
        &self,
        magnitudes: &[Vec<f32>],
    ) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
        let n_frames = magnitudes.len();
        if n_frames == 0 {
            return (Vec::new(), Vec::new());
        }
        let n_bins = magnitudes[0].len();

        // Harmonic median: filter each bin across time
        let mut mag_harm = vec![vec![0.0_f32; n_bins]; n_frames];
        for b in 0..n_bins {
            // Extract column (time series for this bin)
            let col: Vec<f32> = (0..n_frames)
                .map(|t| magnitudes[t][b])
                .collect();
            let filtered = sliding_median(&col, L_HARM);
            for t in 0..n_frames {
                mag_harm[t][b] = filtered[t];
            }
        }

        // Percussive median: filter each frame across bins
        let mut mag_perc = vec![vec![0.0_f32; n_bins]; n_frames];
        for t in 0..n_frames {
            let filtered = sliding_median(&magnitudes[t], L_PERC);
            mag_perc[t] = filtered;
        }

        // Wiener soft masks
        let eps = 1e-8_f32;
        let mut mask_h = vec![vec![0.0_f32; n_bins]; n_frames];
        let mut mask_p = vec![vec![0.0_f32; n_bins]; n_frames];

        for t in 0..n_frames {
            for b in 0..n_bins {
                let h = mag_harm[t][b];
                let p = mag_perc[t][b];
                let d = h + p + eps;
                mask_h[t][b] = h / d;
                mask_p[t][b] = p / d;
            }
        }

        (mask_h, mask_p)
    }
}
