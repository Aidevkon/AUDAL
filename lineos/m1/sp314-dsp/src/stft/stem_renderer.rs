use crate::stft::{StftEngine, N_BINS};
use crate::stft::hpss::HpssProcessor;
use crate::stft::nmf::{NmfEngine, N_COMPONENTS};
use rustfft::num_complex::Complex;

pub struct FourStemRenderer {
    engine:    StftEngine,
    processor: HpssProcessor,
}

/// Output of the 4-stem separation.
pub struct FourStems {
    pub bass:   Vec<f32>,
    pub vocals: Vec<f32>,
    pub drums:  Vec<f32>,
    pub other:  Vec<f32>,
}

impl FourStemRenderer {
    pub fn new() -> Self {
        Self {
            engine:    StftEngine::new(),
            processor: HpssProcessor::new(),
        }
    }

    pub fn render(&mut self, signal: &[f32]) -> FourStems {
        let n = signal.len();
        if n == 0 {
            return FourStems {
                bass:   Vec::new(),
                vocals: Vec::new(),
                drums:  Vec::new(),
                other:  Vec::new(),
            };
        }

        // Step 1: STFT
        let (frames, n_frames) = self.engine.forward(signal);

        // Step 2: Magnitude spectrogram
        let mut magnitudes = vec![vec![0.0_f32; N_BINS]; n_frames];
        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                magnitudes[t][b] = libm::sqrtf(re * re + im * im);
            }
        }

        // Step 3: HPSS — separate harmonic and percussive
        let (mask_h, mask_p) = self.processor.process(&magnitudes);

        // Step 4: Apply harmonic mask to get harmonic spectrogram
        let harmonic_mag: Vec<Vec<f32>> = (0..n_frames)
            .map(|t| (0..N_BINS)
                .map(|b| magnitudes[t][b] * mask_h[t][b])
                .collect())
            .collect();

        // Step 5: NMF on harmonic spectrogram → Bass, Vocals, Other
        let nmf = NmfEngine::fit(&harmonic_mag);
        let centroids = nmf.centroids(N_BINS);

        // Sort by centroid: lowest=Bass, highest=Other, middle=Vocals
        let mut sorted: Vec<usize> = (0..N_COMPONENTS).collect();
        sorted.sort_by(|&a, &b|
            centroids[a].total_cmp(&centroids[b]));
        let bass_comp   = sorted[0];
        let vocals_comp = sorted[1];
        let other_comp  = sorted[2];

        // Step 6: Build NMF Wiener masks
        let mask_bass   = nmf.component_mask(bass_comp,   N_BINS, n_frames);
        let mask_vocals = nmf.component_mask(vocals_comp, N_BINS, n_frames);
        let mask_other  = nmf.component_mask(other_comp,  N_BINS, n_frames);

        // Step 7: Combine HPSS + NMF masks and apply in Cartesian domain
        let mut frames_bass   = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_vocals = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_drums  = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_other  = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];

        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                let mh = mask_h[t][b];
                let mp = mask_p[t][b];

                // Harmonic stems: HPSS harmonic × NMF component mask
                let mb = mask_bass[t][b]   * mh;
                let mv = mask_vocals[t][b] * mh;
                let mo = mask_other[t][b]  * mh;

                frames_bass[t][b]   = Complex::new(re * mb, im * mb);
                frames_vocals[t][b] = Complex::new(re * mv, im * mv);
                frames_drums[t][b]  = Complex::new(re * mp, im * mp);
                frames_other[t][b]  = Complex::new(re * mo, im * mo);
            }
        }

        // Step 8: iSTFT for all 4 stems
        FourStems {
            bass:   self.engine.inverse(&frames_bass,   n),
            vocals: self.engine.inverse(&frames_vocals, n),
            drums:  self.engine.inverse(&frames_drums,  n),
            other:  self.engine.inverse(&frames_other,  n),
        }
    }
}

// Keep StemRenderer for backward compatibility
pub struct StemRenderer {
    inner: FourStemRenderer,
}

impl StemRenderer {
    pub fn new() -> Self {
        Self { inner: FourStemRenderer::new() }
    }
    pub fn render(&mut self, signal: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let stems = self.inner.render(signal);
        let mut harmonic = vec![0.0_f32; stems.bass.len()];
        for i in 0..stems.bass.len() {
            harmonic[i] = stems.bass[i] + stems.vocals[i] + stems.other[i];
        }
        (harmonic, stems.drums)
    }
}
