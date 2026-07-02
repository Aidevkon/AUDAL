//! PhantomMaster — the invisible album anchor.
//! Weighted centroid of N track analyses.
//! Authority: aether-black-spec-v1_0.md AB-P1
//! INV-AB-1: same tracks → same PhantomMaster. Always.

use lineos_types::pre_analysis::PreAnalysisData;

/// The mathematical center of an album.
/// Not an average — a weighted centroid.
/// Tracks with higher energy contribute more.
#[derive(Debug, Clone)]
pub struct PhantomMaster {
    pub integrated_lufs: f32,
    pub lra: f32,
    pub crest_factor: f32,
    pub stereo_width: f32,
    pub true_peak_dbtp: f32,
    pub spectral_profile: [f32; 8],
    pub mfcc_centroid: [f32; 13],
}

impl PhantomMaster {
    /// Build PhantomMaster from N track analyses.
    /// Weight = RMS energy of each track (louder tracks anchor more).
    /// INV-AB-1: deterministic — no randomness.
    pub fn from_tracks(analyses: &[PreAnalysisData]) -> Option<Self> {
        if analyses.is_empty() {
            return None;
        }

        // Compute per-track energy weight from integrated LUFS
        // Convert LUFS to linear energy: 10^(lufs/10)
        let weights: Vec<f32> = analyses
            .iter()
            .map(|a| {
                let lufs = a.integrated_lufs.max(-70.0);
                libm::powf(10.0_f32, lufs / 10.0)
            })
            .collect();

        let total_weight: f32 = weights.iter().sum();
        if total_weight < 1e-10 {
            return None;
        }

        // Weighted centroid for each field
        let w_lufs = weighted_mean(
            analyses.iter().map(|a| a.integrated_lufs),
            &weights,
            total_weight,
        );

        let w_lra = weighted_mean(
            analyses.iter().map(|a| a.loudness_range),
            &weights,
            total_weight,
        );

        let w_tp = weighted_mean(
            analyses.iter().map(|a| a.true_peak_dbtp),
            &weights,
            total_weight,
        );

        let w_td = weighted_mean(
            analyses.iter().map(|a| a.transient_density),
            &weights,
            total_weight,
        );

        // Stereo width from phase correlation
        // correlation=1.0 → mono, correlation=0.0 → wide
        let w_width = weighted_mean(
            analyses
                .iter()
                .map(|a| 1.0 - a.global_phase_correlation.abs()),
            &weights,
            total_weight,
        );

        // Crest factor: true_peak - integrated_lufs (higher = more dynamic)
        let w_crest = weighted_mean(
            analyses
                .iter()
                .map(|a| a.true_peak_dbtp - a.integrated_lufs),
            &weights,
            total_weight,
        );

        // Spectral profile — weighted centroid per band
        let mut spectral = [0.0f32; 8];
        for band in 0..8 {
            spectral[band] = weighted_mean(
                analyses.iter().map(|a| a.spectral_profile_db[band]),
                &weights,
                total_weight,
            );
        }

        // MFCC centroid — use transient_density as proxy for now
        // Full MFCC per-track analysis → future AB-P1b
        let mfcc_centroid = [w_td; 13];

        Some(PhantomMaster {
            integrated_lufs: w_lufs,
            lra: w_lra,
            true_peak_dbtp: w_tp,
            stereo_width: w_width,
            crest_factor: w_crest,
            spectral_profile: spectral,
            mfcc_centroid,
        })
    }

    /// L2 distance from this track to the Phantom Master.
    /// High distance = track needs more adjustment.
    pub fn distance_from(&self, track: &PreAnalysisData) -> f32 {
        let track_width = 1.0 - track.global_phase_correlation.abs();
        let track_crest = track.true_peak_dbtp - track.integrated_lufs;

        let d_lufs = self.integrated_lufs - track.integrated_lufs;
        let d_lra = self.lra - track.loudness_range;
        let d_width = self.stereo_width - track_width;
        let d_crest = self.crest_factor - track_crest;

        libm::sqrtf(d_lufs * d_lufs + d_lra * d_lra + d_width * d_width + d_crest * d_crest)
    }
}

fn weighted_mean(values: impl Iterator<Item = f32>, weights: &[f32], total: f32) -> f32 {
    values.zip(weights.iter()).map(|(v, &w)| v * w).sum::<f32>() / total
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::pre_analysis::PreAnalysisData;

    fn make_analysis(lufs: f32, lra: f32, tp: f32) -> PreAnalysisData {
        PreAnalysisData {
            integrated_lufs: lufs,
            loudness_range: lra,
            true_peak_dbtp: tp,
            transient_density: 1.0,
            global_phase_correlation: 0.5,
            spectral_profile_db: [0.0; 8],
            ..PreAnalysisData::silent()
        }
    }

    #[test]
    fn phantom_master_single_track() {
        let a = make_analysis(-14.0, 8.0, -1.0);
        let pm = PhantomMaster::from_tracks(&[a]).unwrap();
        assert!((pm.integrated_lufs - (-14.0)).abs() < 0.1);
        assert!((pm.lra - 8.0).abs() < 0.1);
    }

    #[test]
    fn phantom_master_weighted_toward_louder() {
        // Louder track (-8 LUFS) should pull centroid toward itself
        let quiet = make_analysis(-20.0, 10.0, -2.0);
        let loud = make_analysis(-8.0, 4.0, -0.5);
        let pm = PhantomMaster::from_tracks(&[quiet, loud]).unwrap();
        // Centroid should be closer to loud track
        assert!(
            pm.integrated_lufs > -14.0,
            "Centroid should be pulled toward louder track, got {}",
            pm.integrated_lufs
        );
    }

    #[test]
    fn phantom_master_empty_returns_none() {
        assert!(PhantomMaster::from_tracks(&[]).is_none());
    }

    #[test]
    fn distance_from_identical_is_zero() {
        let a = make_analysis(-14.0, 8.0, -1.0);
        let pm = PhantomMaster::from_tracks(&[a.clone()]).unwrap();
        let d = pm.distance_from(&a);
        assert!(d < 0.01, "Distance from self should be ~0, got {}", d);
    }
}
