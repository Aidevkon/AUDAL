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
    pub bass:      Vec<f32>,
    pub harmonics: Vec<f32>,
    pub drums:     Vec<f32>,
    pub ambience:  Vec<f32>,
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
                bass:      Vec::new(),
                harmonics: Vec::new(),
                drums:     Vec::new(),
                ambience:  Vec::new(),
            };
        }

        // --- NEW V2 PIPELINE ---

        // Step 1: Find representative window (spectral flux)
        use crate::stft::nmf::find_most_diverse_window;
        let (start, end) = find_most_diverse_window(signal, 48000, 10.0);
        let sample = &signal[start..end];

        // Step 2: STFT + Magnitude on sample only
        let (sample_frames, sample_n_frames) = self.engine.forward(sample);
        let mut sample_magnitudes = vec![vec![0.0_f32; N_BINS]; sample_n_frames];
        for t in 0..sample_n_frames {
            for b in 0..N_BINS {
                let re = sample_frames[t][b].re;
                let im = sample_frames[t][b].im;
                sample_magnitudes[t][b] = libm::sqrtf(re * re + im * im);
            }
        }

        // Step 3: HPSS on sample
        let (sample_mask_h, _) = self.processor.process(&sample_magnitudes);
        let sample_harmonic_mag: Vec<Vec<f32>> = (0..sample_n_frames)
            .map(|t| (0..N_BINS)
                .map(|b| sample_magnitudes[t][b] * sample_mask_h[t][b])
                .collect())
            .collect();

        // Step 4: NMF fit on sample → learn W
        let mut nmf = NmfEngine::default();
        let w = nmf.fit(&sample_harmonic_mag);

        // Step 5: STFT + Magnitude on full track
        let (frames, n_frames) = self.engine.forward(signal);
        let mut magnitudes = vec![vec![0.0_f32; N_BINS]; n_frames];
        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                magnitudes[t][b] = libm::sqrtf(re * re + im * im);
            }
        }

        // Step 6: HPSS on full track
        let (mask_h, mask_p) = self.processor.process(&magnitudes);
        let harmonic_mag: Vec<Vec<f32>> = (0..n_frames)
            .map(|t| (0..N_BINS)
                .map(|b| magnitudes[t][b] * mask_h[t][b])
                .collect())
            .collect();

        // Step 7: NMF transform on full track (fast — no iteration)
        let h = nmf.transform(&w, &harmonic_mag);
        
        // Inject full-track H into NmfEngine so existing component_mask works
        nmf.h = h;
        
        let centroids = nmf.centroids(N_BINS);

        // Step 7: Semantic assignment
        // Ambience = highest spectral flatness (diffuse spectrum)
        // Bass = lowest centroid among remaining
        // Harmonics = the other one

        // Compute spectral flatness per NMF component
        let mut flatness = [0.0f32; N_COMPONENTS];
        for c in 0..N_COMPONENTS {
            let mut log_sum = 0.0f32;
            let mut arith   = 0.0f32;
            let eps = 1e-10f32;
            for b in 0..N_BINS {
                let w_val = nmf.w[b * N_COMPONENTS + c];
                log_sum += libm::logf(w_val + eps);
                arith   += w_val;
            }
            let geom = libm::expf(log_sum / N_BINS as f32);
            let mean = arith / N_BINS as f32;
            flatness[c] = if mean > eps { (geom / mean).clamp(0.0, 1.0) } else { 0.0 };
        }

        // Ambience = flattest component
        let ambience_comp = (0..N_COMPONENTS)
            .max_by(|&a, &b| flatness[a].total_cmp(&flatness[b]))
            .unwrap_or(2);

        // Bass and Harmonics from remaining two — by centroid
        let remaining: Vec<usize> = (0..N_COMPONENTS)
            .filter(|&c| c != ambience_comp)
            .collect();

        let bass_comp = *remaining.iter()
            .min_by(|&&a, &&b| centroids[a].total_cmp(&centroids[b]))
            .unwrap();
        let harmonics_comp = *remaining.iter()
            .find(|&&c| c != bass_comp)
            .unwrap();

        // Step 8: Build NMF Wiener masks
        let mask_bass      = nmf.component_mask(bass_comp,      N_BINS, n_frames);
        let mask_harmonics = nmf.component_mask(harmonics_comp, N_BINS, n_frames);
        let mask_ambience  = nmf.component_mask(ambience_comp,  N_BINS, n_frames);

        // Step 9: Combine HPSS + NMF masks and apply in Cartesian domain
        let mut frames_bass      = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_harmonics = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_drums     = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_ambience  = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];

        for t in 0..n_frames {
            for b in 0..N_BINS {
                let re = frames[t][b].re;
                let im = frames[t][b].im;
                let mh = mask_h[t][b];
                let mp = mask_p[t][b];

                // Harmonic stems: HPSS harmonic × NMF component mask
                let mb = mask_bass[t][b]      * mh;
                let mv = mask_harmonics[t][b] * mh;
                let mo = mask_ambience[t][b]  * mh;

                frames_bass[t][b]      = Complex::new(re * mb, im * mb);
                frames_harmonics[t][b] = Complex::new(re * mv, im * mv);
                frames_drums[t][b]     = Complex::new(re * mp, im * mp);
                frames_ambience[t][b]  = Complex::new(re * mo, im * mo);
            }
        }

        // Step 10: iSTFT for all 4 stems
        FourStems {
            bass:      self.engine.inverse(&frames_bass,      n),
            harmonics: self.engine.inverse(&frames_harmonics, n),
            drums:     self.engine.inverse(&frames_drums,     n),
            ambience:  self.engine.inverse(&frames_ambience,  n),
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
            harmonic[i] = stems.bass[i] + stems.harmonics[i] + stems.ambience[i];
        }
        (harmonic, stems.drums)
    }
}
