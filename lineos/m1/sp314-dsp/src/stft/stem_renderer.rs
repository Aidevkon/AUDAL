use crate::stft::{StftEngine, N_BINS};
use crate::stft::hpss::HpssProcessor;
use crate::stft::nmf::{NmfEngine, N_COMPONENTS};
use rustfft::num_complex::Complex;

pub struct FiveStemRenderer {
    engine:    StftEngine,
    processor: HpssProcessor,
}

/// Output of the 5-stem separation.
pub struct FiveStems {
    pub drums:     Vec<f32>,
    pub bass:      Vec<f32>,
    pub voice:     Vec<f32>,
    pub harmonics: Vec<f32>,
    pub ambience:  Vec<f32>,
    // Transient density per stem — computed during NMF (not approximated)
    pub voice_transient_density:     f32,
    pub drums_transient_density:     f32,
    pub bass_transient_density:      f32,
    pub harmonics_transient_density: f32,
    pub ambience_transient_density:  f32,
}

/// Compute transient density of an NMF component's H row.
/// Uses relative threshold (mean + 1σ of first derivative).
/// INV-AB-1: deterministic — no randomness.
pub fn component_transient_density(h: &[f32], n_frames: usize) -> f32 {
    if n_frames < 2 { return 0.0; }
    let deltas: Vec<f32> = (1..n_frames)
        .map(|i| (h[i] - h[i - 1]).abs())
        .collect();
    let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
    let variance = deltas.iter()
        .map(|d| (d - mean).powi(2))
        .sum::<f32>() / deltas.len() as f32;
    let threshold = mean + libm::sqrtf(variance);
    let spikes = deltas.iter().filter(|&&d| d > threshold).count();
    spikes as f32 / n_frames as f32
}

impl FiveStemRenderer {
    pub fn new() -> Self {
        Self {
            engine:    StftEngine::new(),
            processor: HpssProcessor::new(),
        }
    }

    pub fn render(&mut self, signal: &[f32]) -> FiveStems {
        let n = signal.len();
        if n == 0 {
            return FiveStems {
                drums:     Vec::new(),
                bass:      Vec::new(),
                voice:     Vec::new(),
                harmonics: Vec::new(),
                ambience:  Vec::new(),
                voice_transient_density:     0.0,
                drums_transient_density:     0.0,
                bass_transient_density:      0.0,
                harmonics_transient_density: 0.0,
                ambience_transient_density:  0.0,
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

        let remaining_voice: Vec<usize> = remaining.into_iter()
            .filter(|&c| c != bass_comp)
            .collect();

        // Voice vs Harmonics semantic assignment
        let mut voice_comp = remaining_voice[0];
        let mut harmonics_comp = remaining_voice[1];
        
        // Extract H rows for transient density
        let mut h_rows = vec![vec![0.0f32; n_frames]; N_COMPONENTS];
        for t in 0..n_frames {
            for c in 0..N_COMPONENTS {
                h_rows[c][t] = nmf.h[t * N_COMPONENTS + c];
            }
        }
        
        let td0 = component_transient_density(&h_rows[remaining_voice[0]], n_frames);
        let td1 = component_transient_density(&h_rows[remaining_voice[1]], n_frames);

        if (td0 - td1).abs() <= 0.01 {
            // Tie-break: highest centroid in 1-4kHz presence band
            let start_bin = (1024.0 * 1000.0 / 24000.0) as usize;
            let end_bin = (1024.0 * 4000.0 / 24000.0) as usize;
            
            let mut c0_sum = 0.0;
            let mut c0_mass = 0.0;
            let mut c1_sum = 0.0;
            let mut c1_mass = 0.0;
            
            for b in start_bin..=end_bin {
                let w0 = nmf.w[b * N_COMPONENTS + remaining_voice[0]];
                c0_sum += b as f32 * w0;
                c0_mass += w0;
                
                let w1 = nmf.w[b * N_COMPONENTS + remaining_voice[1]];
                c1_sum += b as f32 * w1;
                c1_mass += w1;
            }
            
            let c0 = if c0_mass > 0.0 { c0_sum / c0_mass } else { 0.0 };
            let c1 = if c1_mass > 0.0 { c1_sum / c1_mass } else { 0.0 };
            
            if c0 > c1 {
                voice_comp = remaining_voice[0];
                harmonics_comp = remaining_voice[1];
            } else {
                voice_comp = remaining_voice[1];
                harmonics_comp = remaining_voice[0];
            }
        } else if td0 > td1 {
            voice_comp = remaining_voice[0];
            harmonics_comp = remaining_voice[1];
        } else {
            voice_comp = remaining_voice[1];
            harmonics_comp = remaining_voice[0];
        }

        // Step 8: Build NMF Wiener masks
        let mask_bass      = nmf.component_mask(bass_comp,      N_BINS, n_frames);
        let mask_voice     = nmf.component_mask(voice_comp,     N_BINS, n_frames);
        let mask_harmonics = nmf.component_mask(harmonics_comp, N_BINS, n_frames);
        let mask_ambience  = nmf.component_mask(ambience_comp,  N_BINS, n_frames);

        // Step 9: Combine HPSS + NMF masks and apply in Cartesian domain
        let mut frames_bass      = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
        let mut frames_voice     = vec![vec![Complex::new(0.0_f32, 0.0_f32); N_BINS]; n_frames];
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
                let m_v = mask_voice[t][b]    * mh;
                let m_h = mask_harmonics[t][b] * mh;
                let mo = mask_ambience[t][b]  * mh;

                frames_bass[t][b]      = Complex::new(re * mb, im * mb);
                frames_voice[t][b]     = Complex::new(re * m_v, im * m_v);
                frames_harmonics[t][b] = Complex::new(re * m_h, im * m_h);
                frames_drums[t][b]     = Complex::new(re * mp, im * mp);
                frames_ambience[t][b]  = Complex::new(re * mo, im * mo);
            }
        }

        // Step 10: iSTFT for all 5 stems
        
        // Compute transient density for all stems
        let voice_td     = component_transient_density(&h_rows[voice_comp], n_frames);
        let harmonics_td = component_transient_density(&h_rows[harmonics_comp], n_frames);
        let bass_td      = component_transient_density(&h_rows[bass_comp], n_frames);
        let ambience_td  = component_transient_density(&h_rows[ambience_comp], n_frames);
        
        let mut h_drums = vec![0.0f32; n_frames];
        for t in 0..n_frames {
            let mut sum = 0.0;
            for b in 0..N_BINS {
                sum += mask_p[t][b];
            }
            h_drums[t] = sum;
        }
        let drums_td = component_transient_density(&h_drums, n_frames);

        FiveStems {
            bass:      self.engine.inverse(&frames_bass,      n),
            voice:     self.engine.inverse(&frames_voice,     n),
            harmonics: self.engine.inverse(&frames_harmonics, n),
            drums:     self.engine.inverse(&frames_drums,     n),
            ambience:  self.engine.inverse(&frames_ambience,  n),
            voice_transient_density:     voice_td,
            drums_transient_density:     drums_td,
            bass_transient_density:      bass_td,
            harmonics_transient_density: harmonics_td,
            ambience_transient_density:  ambience_td,
        }
    }
}

// Keep StemRenderer for backward compatibility
pub struct StemRenderer {
    inner: FiveStemRenderer,
}

impl StemRenderer {
    pub fn new() -> Self {
        Self { inner: FiveStemRenderer::new() }
    }
    pub fn render(&mut self, signal: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let stems = self.inner.render(signal);
        let mut harmonic = vec![0.0_f32; stems.bass.len()];
        for i in 0..stems.bass.len() {
            harmonic[i] = stems.bass[i] + stems.harmonics[i] + stems.voice[i] + stems.ambience[i];
        }
        (harmonic, stems.drums)
    }
}
