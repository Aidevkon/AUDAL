// analysis/analyzer.rs — StemFeatureAnalyzer
// S-002 §5 Processing Pipeline
// All spectral features computed per-channel, averaged (Option A per DeepSeek audit)

use crate::stft::stem_renderer::FourStems;
use crate::metering::measure_integrated_lufs;
use super::features::{
    StemFeatures, StemMetrics, MixMetrics,
    ENERGY_RATIO_EPSILON,
};
use super::spectral::{spectral_centroid_hz, spectral_flatness,
                      spectral_crest_factor_db};
use super::dynamics::{crest_factor_db, rms_db, dynamic_range_db};
use super::stereo::{stereo_correlation, stereo_width};

pub struct StemFeatureAnalyzer;

impl StemFeatureAnalyzer {
    /// Analyze 4 stereo interleaved stems from S-001 (FourStems).
    /// Returns StemFeatures with all metrics computed.
    pub fn analyze(stems: &FourStems, sample_rate: u32) -> StemFeatures {
        let bass   = Self::analyze_stem(&stems.bass,   sample_rate);
        let vocals = Self::analyze_stem(&stems.vocals, sample_rate);
        let drums  = Self::analyze_stem(&stems.drums,  sample_rate);
        let other  = Self::analyze_stem(&stems.other,  sample_rate);

        // Mix energy for ratios
        let mix_energy = Self::stereo_energy(&stems.bass)
                       + Self::stereo_energy(&stems.vocals)
                       + Self::stereo_energy(&stems.drums)
                       + Self::stereo_energy(&stems.other);

        let bass_ratio   = Self::energy_ratio(&stems.bass,   mix_energy);
        let vocals_ratio = Self::energy_ratio(&stems.vocals, mix_energy);
        let drums_ratio  = Self::energy_ratio(&stems.drums,  mix_energy);
        let other_ratio  = Self::energy_ratio(&stems.other,  mix_energy);

        // Sum check assertion (constitutional)
        debug_assert!(
            bass_ratio + vocals_ratio + drums_ratio + other_ratio
                <= 1.0 + ENERGY_RATIO_EPSILON
        );

        // Mix: combine all stems
        let mix_stereo = Self::combine_stereo(
            &stems.bass, &stems.vocals, &stems.drums, &stems.other);
        let mix_l: Vec<f32> = mix_stereo.iter().step_by(2).copied().collect();
        let mix_r: Vec<f32> = mix_stereo.iter().skip(1).step_by(2).copied().collect();

        // Mix centroid: energy-weighted average of stem centroids (S-008)
        let ratios = [bass_ratio, vocals_ratio, drums_ratio, other_ratio];
        let centroids = [bass.spectral_centroid_hz, vocals.spectral_centroid_hz,
                         drums.spectral_centroid_hz, other.spectral_centroid_hz];
        let total_w: f32 = ratios.iter().sum();
        let mix_centroid = if total_w > 1e-10 {
            ratios.iter().zip(centroids.iter())
                .map(|(w, c)| w * c)
                .sum::<f32>() / total_w
        } else { 1000.0 };

        let mix = MixMetrics {
            integrated_lufs:    measure_integrated_lufs(&mix_l, &mix_r),
            true_peak_dbtp:     -1.0,  // simplified for v1.0
            loudness_range:     0.0,   // simplified for v1.0
            stereo_correlation: stereo_correlation(&mix_stereo),
            stereo_width:       stereo_width(&mix_stereo),
            dynamic_range_db:   dynamic_range_db(&mix_l, sample_rate),
            stem_energy_ratios: [bass_ratio, vocals_ratio,
                                  drums_ratio, other_ratio],
            spectral_centroid_hz: mix_centroid,
        };

        StemFeatures { bass, vocals, drums, other, mix }
    }

    fn analyze_stem(stereo: &[f32], sample_rate: u32) -> StemMetrics {
        if stereo.is_empty() { return StemMetrics::default(); }

        // Deinterleave
        let l: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let r: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();

        // Spectral: per-channel, averaged (S-002 Option A)
        let centroid = (spectral_centroid_hz(&l, sample_rate)
                      + spectral_centroid_hz(&r, sample_rate)) * 0.5;
        let flatness = (spectral_flatness(&l)
                      + spectral_flatness(&r)) * 0.5;
        let spec_crest = (spectral_crest_factor_db(&l)
                        + spectral_crest_factor_db(&r)) * 0.5;

        // Loudness: on stereo pair
        let lufs    = measure_integrated_lufs(&l, &r);
        let rms     = (rms_db(&l) + rms_db(&r)) * 0.5;

        // Dynamics
        let crest   = (crest_factor_db(&l) + crest_factor_db(&r)) * 0.5;
        let dyn_rng = dynamic_range_db(&l, sample_rate);

        // Stereo
        let corr    = stereo_correlation(stereo);
        let width   = stereo_width(stereo);

        StemMetrics {
            spectral_centroid_hz:  centroid,
            spectral_flatness:     flatness,
            spectral_crest_factor: spec_crest,
            integrated_lufs:       lufs,
            true_peak_dbtp:        -1.0,  // simplified v1.0
            loudness_range:        0.0,   // simplified v1.0
            rms_db:                rms,
            stereo_correlation:    corr,
            stereo_width:          width,
            crest_factor_db:       crest,
            dynamic_range_db:      dyn_rng,
            energy_ratio:          0.0,  // set by caller
        }
    }

    fn stereo_energy(stereo: &[f32]) -> f32 {
        stereo.iter().map(|s| s * s).sum()
    }

    fn energy_ratio(stem: &[f32], mix_energy: f32) -> f32 {
        if mix_energy < 1e-10 { return 0.0; }
        (Self::stereo_energy(stem) / mix_energy).clamp(0.0, 1.0)
    }

    fn combine_stereo(a: &[f32], b: &[f32], c: &[f32], d: &[f32]) -> Vec<f32> {
        let n = a.len().min(b.len()).min(c.len()).min(d.len());
        (0..n).map(|i| a[i] + b[i] + c[i] + d[i]).collect()
    }
}
