// analysis/analyzer.rs — StemFeatureAnalyzer
// S-002 §5 Processing Pipeline
// All spectral features computed per-channel, averaged (Option A per DeepSeek audit)

use crate::stft::stem_renderer::FiveStems;
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
    pub fn analyze(stems: &FiveStems, sample_rate: u32) -> StemFeatures {
        let bass      = Self::analyze_stem(&stems.bass,      sample_rate);
        let harmonics = Self::analyze_stem(&stems.harmonics, sample_rate);
        let voice     = Self::analyze_stem(&stems.voice,     sample_rate);
        let drums     = Self::analyze_stem(&stems.drums,     sample_rate);
        let ambience  = Self::analyze_stem(&stems.ambience,  sample_rate);

        // Mix energy for ratios
        let mix_energy = Self::stereo_energy(&stems.bass)
                       + Self::stereo_energy(&stems.harmonics)
                       + Self::stereo_energy(&stems.voice)
                       + Self::stereo_energy(&stems.drums)
                       + Self::stereo_energy(&stems.ambience);

        let bass_ratio      = Self::energy_ratio(&stems.bass,      mix_energy);
        let harmonics_ratio = Self::energy_ratio(&stems.harmonics, mix_energy);
        let voice_ratio     = Self::energy_ratio(&stems.voice,     mix_energy);
        let drums_ratio     = Self::energy_ratio(&stems.drums,     mix_energy);
        let ambience_ratio  = Self::energy_ratio(&stems.ambience,  mix_energy);

        // Sum check assertion (constitutional)
        debug_assert!(
            bass_ratio + harmonics_ratio + voice_ratio + drums_ratio + ambience_ratio
                <= 1.0 + ENERGY_RATIO_EPSILON
        );

        // Mix: combine all stems
        let mix_stereo = Self::combine_stereo_five(
            &stems.bass, &stems.harmonics, &stems.voice, &stems.drums, &stems.ambience);
        let mix_l: Vec<f32> = mix_stereo.iter().step_by(2).copied().collect();
        let mix_r: Vec<f32> = mix_stereo.iter().skip(1).step_by(2).copied().collect();

        // Mix centroid: energy-weighted average of stem centroids (S-008)
        // Mix centroid: energy-weighted average of stem centroids (S-008)
        let ratios = [bass_ratio, harmonics_ratio, voice_ratio, drums_ratio, ambience_ratio];
        let centroids = [bass.spectral_centroid_hz, harmonics.spectral_centroid_hz, voice.spectral_centroid_hz,
                         drums.spectral_centroid_hz, ambience.spectral_centroid_hz];
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
            stem_energy_ratios: [bass_ratio, harmonics_ratio,
                                  voice_ratio, drums_ratio, ambience_ratio],
            spectral_centroid_hz: mix_centroid,
        };

        StemFeatures { bass, harmonics, voice, drums, ambience, mix }
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

    fn combine_stereo_five(a: &[f32], b: &[f32], c: &[f32], d: &[f32], e: &[f32]) -> Vec<f32> {
        let n = a.len().min(b.len()).min(c.len()).min(d.len()).min(e.len());
        (0..n).map(|i| a[i] + b[i] + c[i] + d[i] + e[i]).collect()
    }

    /// Analyze a stereo PCM signal — mix metrics only.
    /// Stems (bass, vocals, drums, other) = StemMetrics::default().
    /// Lightweight alternative to analyze() — no NMF/STFT separation.
    /// TODO v2.0: Replace with real stem separation (S-001 FourStems)
    ///            when stem separation is integrated into m0-daemon.
    pub fn analyze_stereo(
        left:        &[f32],
        right:       &[f32],
        sample_rate: u32,
    ) -> StemFeatures {
        use super::spectral::spectral_centroid_hz;
        use super::dynamics::{rms_db, dynamic_range_db};
        use super::stereo::{stereo_correlation, stereo_width};

        if left.is_empty() || right.is_empty() {
            return StemFeatures {
                bass:      StemMetrics::default(),
                harmonics: StemMetrics::default(),
                voice:     StemMetrics::default(),
                drums:     StemMetrics::default(),
                ambience:  StemMetrics::default(),
                mix:       MixMetrics::default(),
            };
        }

        // Interleave for stereo functions
        let stereo: Vec<f32> = left.iter().zip(right.iter())
            .flat_map(|(&l, &r)| [l, r])
            .collect();

        // Mix centroid: average of L and R centroids (S-008)
        let centroid = (spectral_centroid_hz(left,  sample_rate)
                      + spectral_centroid_hz(right, sample_rate))
                      * 0.5;

        // Integrated LUFS on stereo pair
        let lufs = crate::metering::measure_integrated_lufs(left, right);

        // RMS: average of L and R
        let _rms  = (rms_db(left) + rms_db(right)) * 0.5;

        // Dynamic range on L channel (representative)
        let dyn_range = dynamic_range_db(left, sample_rate);

        // Stereo metrics
        let corr  = stereo_correlation(&stereo);
        let width = stereo_width(&stereo);

        // Energy ratios: all equal (no stem separation)
        // TODO v2.0: use real NMF stem energies
        let equal_ratio = 0.2_f32;

        let mix = MixMetrics {
            integrated_lufs:     lufs,
            true_peak_dbtp:      -1.0,   // TODO v2.0: real true peak
            loudness_range:      0.0,    // TODO v2.0: LraCalculator
            stereo_correlation:  corr,
            stereo_width:        width,
            dynamic_range_db:    dyn_range,
            stem_energy_ratios:  [equal_ratio; 5],
            spectral_centroid_hz: centroid,
        };

        StemFeatures {
            bass:      StemMetrics::default(),
            harmonics: StemMetrics::default(),
            voice:     StemMetrics::default(),
            drums:     StemMetrics::default(),
            ambience:  StemMetrics::default(),
            mix,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::features::{StemMetrics, MixMetrics};

    #[test]
    fn analyze_stereo_mix_metrics_reasonable() {
        // Sine-like signal: centroid should be above 0
        let signal: Vec<f32> = (0..4800)
            .map(|i| libm::sinf(2.0 * 3.14159 * 440.0
                                * i as f32 / 48000.0))
            .collect();
        let result = StemFeatureAnalyzer::analyze_stereo(
            &signal, &signal, 48000);
        assert!(result.mix.spectral_centroid_hz > 0.0);
        assert!(result.mix.stereo_correlation > 0.99); // L==R
        assert_eq!(result.mix.stem_energy_ratios, [0.25; 4]);
    }

    #[test]
    fn analyze_stereo_empty_returns_default() {
        let result = StemFeatureAnalyzer::analyze_stereo(
            &[], &[], 48000);
        assert_eq!(result.mix.integrated_lufs,
                   MixMetrics::default().integrated_lufs);
    }

    #[test]
    fn analyze_stereo_stems_are_default() {
        let signal = vec![0.1_f32; 4800];
        let result = StemFeatureAnalyzer::analyze_stereo(
            &signal, &signal, 48000);
        assert_eq!(result.bass,   StemMetrics::default());
        assert_eq!(result.harmonics, StemMetrics::default());
        assert_eq!(result.drums,  StemMetrics::default());
        assert_eq!(result.ambience,  StemMetrics::default());
    }
}
