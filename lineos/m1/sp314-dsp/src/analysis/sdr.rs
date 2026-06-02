//! SDR — Signal-to-Distortion Ratio for stem separation quality.
//! Authority: NMF Upgrade Plan v2.1 NMF-V2-P5
//! Gate: SDR > 6dB per stem = acceptable separation quality.

/// Compute Signal-to-Distortion Ratio between reference and estimated stem.
/// Returns SDR in dB. Higher = better separation.
/// SDR > 6dB = acceptable. SDR > 10dB = good. SDR > 15dB = excellent.
pub fn sdr_db(reference: &[f32], estimated: &[f32]) -> f32 {
    assert_eq!(reference.len(), estimated.len());
    let signal_power: f32 = reference.iter().map(|x| x * x).sum();
    let noise: Vec<f32> = reference.iter()
        .zip(estimated.iter())
        .map(|(r, e)| r - e)
        .collect();
    let noise_power: f32 = noise.iter().map(|x| x * x).sum();
    if noise_power < 1e-10 {
        return 100.0; // perfect separation
    }
    10.0 * libm::log10f(signal_power / noise_power)
}

/// Compute SDR for all 4 stems.
pub struct StemSdr {
    pub drums:     f32,
    pub bass:      f32,
    pub harmonics: f32,
    pub ambience:  f32,
}

impl StemSdr {
    pub fn all_above(&self, threshold_db: f32) -> bool {
        self.drums     > threshold_db &&
        self.bass      > threshold_db &&
        self.harmonics > threshold_db &&
        self.ambience  > threshold_db
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_reconstruction_returns_high_sdr() {
        let signal: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.1).sin()).collect();
        let sdr = sdr_db(&signal, &signal);
        assert!(sdr > 60.0, "perfect reconstruction SDR should be >> 60dB");
    }

    #[test]
    fn zero_signal_returns_low_sdr() {
        let reference: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.1).sin()).collect();
        let zeros = vec![0.0f32; 1000];
        let sdr = sdr_db(&reference, &zeros);
        assert!(sdr <= 0.0, "zero estimate SDR should be <= 0dB");
    }

    #[test]
    fn sdr_above_6db_gate() {
        // Simulate decent separation: estimate = reference + small noise
        let reference: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.1).sin()).collect();
        let estimated: Vec<f32> = reference.iter()
            .enumerate()
            .map(|(i, r)| r + 0.1 * (i as f32 * 0.3).sin())
            .collect();
        let sdr = sdr_db(&reference, &estimated);
        assert!(sdr > 6.0, "decent separation should exceed 6dB gate");
    }
}
