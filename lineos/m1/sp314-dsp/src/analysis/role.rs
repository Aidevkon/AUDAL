#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Voice,
    Drums,
    Music,
}

#[derive(Debug, Clone, Copy)]
pub struct RoleFeatures {
    pub speech_p: Option<f32>,
    pub low_ratio: f32,
    pub flux: f32,
}

pub fn classify(f: &RoleFeatures) -> Role {
    if let Some(p) = f.speech_p {
        if p > 0.15 {
            return Role::Voice;
        }
    }

    if f.low_ratio > 0.3 && f.flux > 0.06 {
        return Role::Drums;
    }

    Role::Music
}

/// Extracts the ratio of energy below 150Hz.
/// Assumes `mag` is a half-spectrum magnitude array from STFT 
/// (e.g., 1025 bins for N_FFT = 2048).
pub fn low_ratio(mag: &[f32], sample_rate: f32) -> f32 {
    if mag.is_empty() {
        return 0.0;
    }
    
    let n_fft = (mag.len() - 1) * 2;
    let hz_per_bin = sample_rate / n_fft as f32;
    
    let mut low_energy = 0.0;
    let mut total_energy = 0.0;
    
    for (i, &m) in mag.iter().enumerate() {
        let hz = i as f32 * hz_per_bin;
        let e = m * m; // energy = mag^2
        if hz < 150.0 {
            low_energy += e;
        }
        total_energy += e;
    }
    
    if total_energy < 1e-12 {
        0.0
    } else {
        low_energy / total_energy
    }
}

/// Computes frame-to-frame spectral flux as the mean of positive magnitude differences.
/// Assumes `cur` and `prev` are half-spectrum magnitude arrays of the same length
/// (e.g., 1025 bins for N_FFT = 2048).
pub fn spectral_flux(cur: &[f32], prev: &[f32]) -> f32 {
    if cur.is_empty() || cur.len() != prev.len() {
        return 0.0;
    }
    
    let mut sum = 0.0;
    for (c, p) in cur.iter().zip(prev.iter()) {
        let diff = c - p;
        if diff > 0.0 {
            sum += diff;
        }
    }
    sum / cur.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drums_rule() {
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.41, flux: 0.08 }), Role::Drums);
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.76, flux: 0.11 }), Role::Drums);
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.53, flux: 0.07 }), Role::Drums);
    }

    #[test]
    fn test_bass_is_music() {
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.56, flux: 0.01 }), Role::Music);
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.89, flux: 0.02 }), Role::Music);
        assert_eq!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.81, flux: 0.03 }), Role::Music);
    }

    #[test]
    fn test_speech_p_none_never_voice() {
        assert_ne!(classify(&RoleFeatures { speech_p: None, low_ratio: 0.0, flux: 0.0 }), Role::Voice);
        assert_ne!(classify(&RoleFeatures { speech_p: None, low_ratio: 1.0, flux: 1.0 }), Role::Voice);
    }

    #[test]
    fn test_speech_p_threshold() {
        assert_eq!(classify(&RoleFeatures { speech_p: Some(0.31), low_ratio: 0.0, flux: 0.0 }), Role::Voice);
        
        // 0.10 falls through to music/drums rules
        assert_eq!(classify(&RoleFeatures { speech_p: Some(0.10), low_ratio: 0.0, flux: 0.0 }), Role::Music);
        assert_eq!(classify(&RoleFeatures { speech_p: Some(0.10), low_ratio: 0.5, flux: 0.1 }), Role::Drums);
    }
}
