use rustfft::num_complex::Complex;
use crate::stft::{StftEngine, N_BINS};

pub struct DrumSplit {
    pub percussive: Vec<f32>,
    pub harmonic: Vec<f32>,
}

pub fn split_percussive(
    spectrum: &[Vec<Complex<f32>>],
    mask_p: &[Vec<f32>],
    output_len: usize,
) -> DrumSplit {
    let n_frames = spectrum.len();
    if n_frames == 0 {
        return DrumSplit {
            percussive: Vec::new(),
            harmonic: Vec::new(),
        };
    }

    let mut spec_p = vec![vec![Complex::new(0.0, 0.0); N_BINS]; n_frames];
    let mut spec_h = vec![vec![Complex::new(0.0, 0.0); N_BINS]; n_frames];

    for t in 0..n_frames {
        // Handle cases where mask_p might have fewer bins or frames (shouldn't happen in valid usage, but safe indexing is good)
        let mask_bins = mask_p.get(t).map(|row| row.len()).unwrap_or(0);
        let spec_bins = spectrum[t].len().min(N_BINS);
        let bins = mask_bins.min(spec_bins);

        for b in 0..bins {
            let m_p = mask_p[t][b];
            let m_h = 1.0 - m_p;
            spec_p[t][b] = spectrum[t][b] * m_p;
            spec_h[t][b] = spectrum[t][b] * m_h;
        }
    }

    // Use the EXISTING istft path
    let mut stft = StftEngine::new();
    let percussive = stft.inverse(&spec_p, output_len);
    let harmonic = stft.inverse(&spec_h, output_len);

    DrumSplit {
        percussive,
        harmonic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::StftEngine;

    fn generate_test_signal(len: usize) -> Vec<f32> {
        (0..len).map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0)).collect()
    }

    #[test]
    fn test_reconstruction() {
        let input_len = 48000;
        let input = generate_test_signal(input_len);
        
        let mut stft = StftEngine::new();
        let (spectrum, _n_frames) = stft.forward(&input);
        
        // Generate a dummy mask (checkerboard pattern)
        let mut mask_p = vec![vec![0.0_f32; N_BINS]; spectrum.len()];
        for t in 0..spectrum.len() {
            for b in 0..N_BINS {
                mask_p[t][b] = if (t + b) % 2 == 0 { 0.8 } else { 0.2 };
            }
        }
        
        let split = split_percussive(&spectrum, &mask_p, input_len);
        let mut stft2 = StftEngine::new();
        let output = stft2.inverse(&spectrum, input_len); // Direct inverse of spectrum
        
        assert_eq!(split.percussive.len(), input_len);
        assert_eq!(split.harmonic.len(), input_len);
        assert_eq!(output.len(), input_len);
        
        for i in 0..input_len {
            let sum = split.percussive[i] + split.harmonic[i];
            let diff = (sum - output[i]).abs();
            assert!(diff < 1e-4, "Reconstruction failed at sample {}, diff: {}", i, diff);
        }
    }

    #[test]
    fn test_mask_all_zeros() {
        let input_len = 48000;
        let input = generate_test_signal(input_len);
        
        let mut stft = StftEngine::new();
        let (spectrum, _n_frames) = stft.forward(&input);
        
        let mask_p = vec![vec![0.0_f32; N_BINS]; spectrum.len()];
        let split = split_percussive(&spectrum, &mask_p, input_len);
        
        let mut stft2 = StftEngine::new();
        let output = stft2.inverse(&spectrum, input_len);
        
        for i in 0..input_len {
            assert!(split.percussive[i].abs() < 1e-6);
            assert!((split.harmonic[i] - output[i]).abs() < 1e-4);
        }
    }

    #[test]
    fn test_mask_all_ones() {
        let input_len = 48000;
        let input = generate_test_signal(input_len);
        
        let mut stft = StftEngine::new();
        let (spectrum, _n_frames) = stft.forward(&input);
        
        let mask_p = vec![vec![1.0_f32; N_BINS]; spectrum.len()];
        let split = split_percussive(&spectrum, &mask_p, input_len);
        
        let mut stft2 = StftEngine::new();
        let output = stft2.inverse(&spectrum, input_len);
        
        for i in 0..input_len {
            assert!(split.harmonic[i].abs() < 1e-6);
            assert!((split.percussive[i] - output[i]).abs() < 1e-4);
        }
    }

    #[test]
    fn test_lengths() {
        let input_len = 48000;
        let input = generate_test_signal(input_len);
        let mut stft = StftEngine::new();
        let (spectrum, _n_frames) = stft.forward(&input);
        
        let mask_p = vec![vec![0.5_f32; N_BINS]; spectrum.len()];
        let split = split_percussive(&spectrum, &mask_p, input_len);
        
        assert_eq!(split.percussive.len(), input_len);
        assert_eq!(split.harmonic.len(), input_len);
    }

    #[test]
    fn test_no_nan_inf() {
        let input_len = 48000;
        // Full scale input
        let input: Vec<f32> = (0..input_len).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let mut stft = StftEngine::new();
        let (spectrum, _n_frames) = stft.forward(&input);
        
        let mut mask_p = vec![vec![0.0_f32; N_BINS]; spectrum.len()];
        for t in 0..spectrum.len() {
            for b in 0..N_BINS {
                mask_p[t][b] = (t as f32 / spectrum.len() as f32).clamp(0.0, 1.0);
            }
        }
        
        let split = split_percussive(&spectrum, &mask_p, input_len);
        
        for i in 0..input_len {
            assert!(split.percussive[i].is_finite());
            assert!(split.harmonic[i].is_finite());
        }
    }
}
