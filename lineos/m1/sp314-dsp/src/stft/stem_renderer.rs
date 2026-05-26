use crate::stft::{StftEngine, N_BINS};
use crate::stft::hpss::HpssProcessor;
use rustfft::num_complex::Complex;

pub struct StemRenderer {
    engine:    StftEngine,
    processor: HpssProcessor,
}

impl StemRenderer {
    pub fn new() -> Self {
        Self {
            engine:    StftEngine::new(),
            processor: HpssProcessor::new(),
        }
    }

    /// Render two stems from a mono signal.
    /// Returns: (harmonic_stem, drums_stem)
    /// Both are Vec<f32> of length signal.len()
    pub fn render(&mut self, signal: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let n = signal.len();
        if n == 0 {
            return (Vec::new(), Vec::new());
        }

        // Step 1: STFT
        let (frames, n_frames) = self.engine.forward(signal);

        // Step 2: Extract magnitude (Phase is implicitly preserved in Cartesian components)
        let mut magnitudes = vec![vec![0.0_f32; N_BINS]; n_frames];

        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                magnitudes[t][b] = libm::sqrtf(re * re + im * im);
            }
        }

        // Step 3: HPSS masks
        let (mask_h, mask_p) = self.processor.process(&magnitudes);

        // Step 4: Apply masks directly in the Cartesian domain (Ultra-fast)
        let mut frames_h = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_p = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];

        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                let mh = mask_h[t][b];
                let mp = mask_p[t][b];

                frames_h[t][b] = Complex::new(re * mh, im * mh);
                frames_p[t][b] = Complex::new(re * mp, im * mp);
            }
        }

        // Step 5: iSTFT
        let stem_h = self.engine.inverse(&frames_h, n);
        let stem_p = self.engine.inverse(&frames_p, n);

        (stem_h, stem_p)
    }
}
