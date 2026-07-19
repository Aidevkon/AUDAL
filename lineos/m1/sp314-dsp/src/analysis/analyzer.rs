// analysis/analyzer.rs — StemFeatureAnalyzer
// S-002 §5 Processing Pipeline
// All spectral features computed per-channel, averaged (Option A per DeepSeek audit)

use super::dynamics::StreamingDynamicsAnalyzer;
use super::dynamics::{crest_factor_db, dynamic_range_db, rms_db};
use super::features::{MixMetrics, StemFeatures, StemMetrics, ENERGY_RATIO_EPSILON};
use super::spectral::StreamingSpectralAnalyzer;
use super::spectral::{spectral_centroid_hz, spectral_crest_factor_db, spectral_flatness};
use super::stereo::{stereo_correlation, stereo_width};
use crate::limiter::true_peak::measure_true_peak_dbtp;
use crate::limiter::true_peak::TruePeakDetector;
use crate::metering::lra::measure_loudness_range;
use crate::metering::lra::StreamingLraMeter;
use crate::metering::lufs_meter::LufsMeter;
use crate::metering::measure_integrated_lufs;
use crate::stft::stem_renderer::FiveStems;

pub struct StemFeatureAnalyzer;

impl StemFeatureAnalyzer {
    pub fn analyze(stems: &FiveStems, sample_rate: u32) -> StemFeatures {
        let mut bass = Self::analyze_stem(&stems.bass, sample_rate);
        let mut harmonics = Self::analyze_stem(&stems.harmonics, sample_rate);
        let mut voice = Self::analyze_stem(&stems.voice, sample_rate);
        let mut drums = Self::analyze_stem(&stems.drums, sample_rate);
        let mut ambience = Self::analyze_stem(&stems.ambience, sample_rate);

        // Use real NMF transient density — not crest approximation
        voice.transient_density = stems.voice_transient_density;
        drums.transient_density = stems.drums_transient_density;
        bass.transient_density = stems.bass_transient_density;
        harmonics.transient_density = stems.harmonics_transient_density;
        ambience.transient_density = stems.ambience_transient_density;

        // Mix energy for ratios
        let mix_energy = Self::stereo_energy(&stems.bass)
            + Self::stereo_energy(&stems.harmonics)
            + Self::stereo_energy(&stems.voice)
            + Self::stereo_energy(&stems.drums)
            + Self::stereo_energy(&stems.ambience);

        let bass_ratio = Self::energy_ratio(&stems.bass, mix_energy);
        let harmonics_ratio = Self::energy_ratio(&stems.harmonics, mix_energy);
        let voice_ratio = Self::energy_ratio(&stems.voice, mix_energy);
        let drums_ratio = Self::energy_ratio(&stems.drums, mix_energy);
        let ambience_ratio = Self::energy_ratio(&stems.ambience, mix_energy);

        // Sum check assertion (constitutional)
        debug_assert!(
            bass_ratio + harmonics_ratio + voice_ratio + drums_ratio + ambience_ratio
                <= 1.0 + ENERGY_RATIO_EPSILON
        );

        // Mix: combine all stems
        let mix_stereo = Self::combine_stereo_five(
            &stems.bass,
            &stems.harmonics,
            &stems.voice,
            &stems.drums,
            &stems.ambience,
        );
        let mix_l: Vec<f32> = mix_stereo.iter().step_by(2).copied().collect();
        let mix_r: Vec<f32> = mix_stereo.iter().skip(1).step_by(2).copied().collect();

        // Mix centroid: energy-weighted average of stem centroids (S-008)
        // Mix centroid: energy-weighted average of stem centroids (S-008)
        let ratios = [
            bass_ratio,
            harmonics_ratio,
            voice_ratio,
            drums_ratio,
            ambience_ratio,
        ];
        let centroids = [
            bass.spectral_centroid_hz,
            harmonics.spectral_centroid_hz,
            voice.spectral_centroid_hz,
            drums.spectral_centroid_hz,
            ambience.spectral_centroid_hz,
        ];
        let total_w: f32 = ratios.iter().sum();
        let mix_centroid = if total_w > 1e-10 {
            ratios
                .iter()
                .zip(centroids.iter())
                .map(|(w, c)| w * c)
                .sum::<f32>()
                / total_w
        } else {
            1000.0
        };

        let mix = MixMetrics {
            integrated_lufs: measure_integrated_lufs(&mix_l, &mix_r),
            true_peak_dbtp: measure_true_peak_dbtp(&mix_l, &mix_r),
            loudness_range: measure_loudness_range(&mix_l, &mix_r, sample_rate),
            stereo_correlation: stereo_correlation(&mix_l, &mix_r),
            stereo_width: stereo_width(&mix_l, &mix_r),
            dynamic_range_db: dynamic_range_db(&mix_l, sample_rate),
            stem_energy_ratios: [
                bass_ratio,
                harmonics_ratio,
                voice_ratio,
                drums_ratio,
                ambience_ratio,
            ],
            spectral_centroid_hz: mix_centroid,
        };

        StemFeatures {
            bass,
            harmonics,
            voice,
            drums,
            ambience,
            mix,
        }
    }

    fn analyze_stem(stereo: &[f32], sample_rate: u32) -> StemMetrics {
        if stereo.is_empty() {
            return StemMetrics::default();
        }

        // Deinterleave
        let l: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let r: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();

        // Loudness: on stereo pair
        let lufs = measure_integrated_lufs(&l, &r);
        let rms = (rms_db(&l) + rms_db(&r)) * 0.5;

        // Spectral: per-channel, averaged (S-002 Option A)
        let centroid = if rms > -80.0 {
            (spectral_centroid_hz(&l, sample_rate) + spectral_centroid_hz(&r, sample_rate)) * 0.5
        } else {
            0.0
        };
        let flatness = (spectral_flatness(&l) + spectral_flatness(&r)) * 0.5;
        let spec_crest = (spectral_crest_factor_db(&l) + spectral_crest_factor_db(&r)) * 0.5;

        // Dynamics
        let raw_crest = (crest_factor_db(&l) + crest_factor_db(&r)) * 0.5;
        let crest = if raw_crest.is_nan() {
            0.0
        } else {
            raw_crest.clamp(0.0, 30.0)
        };
        let dyn_rng = dynamic_range_db(&l, sample_rate);

        // Stereo
        let corr = stereo_correlation(&l, &r);
        let width = stereo_width(&l, &r);

        StemMetrics {
            spectral_centroid_hz: centroid,
            spectral_flatness: flatness,
            spectral_crest_factor: spec_crest,
            integrated_lufs: lufs,
            true_peak_dbtp: measure_true_peak_dbtp(&l, &r),
            loudness_range: measure_loudness_range(&l, &r, sample_rate),
            rms_db: rms,
            stereo_correlation: corr,
            stereo_width: width,
            crest_factor_db: crest,
            dynamic_range_db: dyn_rng,
            energy_ratio: 0.0,      // set by caller
            transient_density: 0.0, // set by caller
        }
    }

    fn stereo_energy(stereo: &[f32]) -> f32 {
        stereo.iter().map(|s| s * s).sum()
    }

    fn energy_ratio(stem: &[f32], mix_energy: f32) -> f32 {
        if mix_energy < 1e-10 {
            return 0.0;
        }
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
    pub fn analyze_stereo(left: &[f32], right: &[f32], sample_rate: u32) -> StemFeatures {
        use super::dynamics::{dynamic_range_db, rms_db};
        use super::spectral::spectral_centroid_hz;
        use super::stereo::{stereo_correlation, stereo_width};

        if left.is_empty() || right.is_empty() {
            return StemFeatures {
                bass: StemMetrics::default(),
                harmonics: StemMetrics::default(),
                voice: StemMetrics::default(),
                drums: StemMetrics::default(),
                ambience: StemMetrics::default(),
                mix: MixMetrics::default(),
            };
        }

        // Mix centroid: average of L and R centroids (S-008)
        let centroid = (spectral_centroid_hz(left, sample_rate)
            + spectral_centroid_hz(right, sample_rate))
            * 0.5;

        // Integrated LUFS on stereo pair
        let lufs = crate::metering::measure_integrated_lufs(left, right);

        // RMS: average of L and R
        let _rms = (rms_db(left) + rms_db(right)) * 0.5;

        // Dynamic range on L channel (representative)
        let dyn_range = dynamic_range_db(left, sample_rate);

        // Stereo metrics
        let corr = stereo_correlation(left, right);
        let width = stereo_width(left, right);

        let mix = MixMetrics {
            integrated_lufs: lufs,
            true_peak_dbtp: measure_true_peak_dbtp(left, right),
            loudness_range: measure_loudness_range(left, right, sample_rate),
            stereo_correlation: corr,
            stereo_width: width,
            dynamic_range_db: dyn_range,
            // Unavailable: analyze_stereo() skips
            // NMF/STFT — no stem separation.
            // Use analyze() for real ratios.
            stem_energy_ratios: [0.0; 5],
            spectral_centroid_hz: centroid,
        };

        StemFeatures {
            bass: StemMetrics::default(),
            harmonics: StemMetrics::default(),
            voice: StemMetrics::default(),
            drums: StemMetrics::default(),
            ambience: StemMetrics::default(),
            mix,
        }
    }
}

/// Streaming Stem Analyzer.
/// Exact bit-identical streaming counterpart to analyze_stem.
/// Memory profile: bounded (combines bounded components).
pub struct StreamingStemAnalyzer {
    spec_l: StreamingSpectralAnalyzer,
    spec_r: StreamingSpectralAnalyzer,
    dyn_l: StreamingDynamicsAnalyzer,
    dyn_r: StreamingDynamicsAnalyzer,
    lufs: LufsMeter,
    lra: StreamingLraMeter,
    tp_detector: TruePeakDetector,
    tp_max: f32,
    corr_cross: f32,
    corr_sum_l: f32,
    corr_sum_r: f32,
    has_fed: bool,
}

impl StreamingStemAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            spec_l: StreamingSpectralAnalyzer::new(sample_rate),
            spec_r: StreamingSpectralAnalyzer::new(sample_rate),
            dyn_l: StreamingDynamicsAnalyzer::new(sample_rate),
            dyn_r: StreamingDynamicsAnalyzer::new(sample_rate),
            lufs: LufsMeter::new(),
            lra: StreamingLraMeter::new(sample_rate),
            tp_detector: TruePeakDetector::new(),
            tp_max: 0.0,
            corr_cross: 0.0,
            corr_sum_l: 0.0,
            corr_sum_r: 0.0,
            has_fed: false,
        }
    }

    /// L and R chunks of equal length, de-interleaved by the caller.
    pub fn feed_chunk(&mut self, left: &[f32], right: &[f32]) {
        if left.is_empty() || right.is_empty() {
            return;
        }
        self.has_fed = true;

        self.spec_l.feed_chunk(left);
        self.spec_r.feed_chunk(right);
        self.dyn_l.feed_chunk(left);
        self.dyn_r.feed_chunk(right);
        self.lufs.process_chunk(left, right);
        self.lra.process_chunk(left, right);

        let n = left.len().min(right.len());
        for i in 0..n {
            let tp = self.tp_detector.process(left[i], right[i]);
            if tp > self.tp_max {
                self.tp_max = tp;
            }

            self.corr_cross += left[i] * right[i];
            self.corr_sum_l += left[i] * left[i];
            self.corr_sum_r += right[i] * right[i];
        }
    }

    /// Same StemMetrics analyze_stem returns for the concatenated signal,
    /// with energy_ratio and transient_density left at 0.0 (caller sets them).
    pub fn finish(self) -> StemMetrics {
        if !self.has_fed {
            return StemMetrics::default();
        }

        let lufs = self.lufs.finish().unwrap_or(-144.0);
        let lra = self.lra.finish();

        let (dyn_rms_l, dyn_crest_l, dyn_rng_l) = self.dyn_l.finish();
        let (dyn_rms_r, dyn_crest_r, _dyn_rng_r) = self.dyn_r.finish();

        let (spec_c_l, spec_f_l, spec_cr_l) = self.spec_l.finish();
        let (spec_c_r, spec_f_r, spec_cr_r) = self.spec_r.finish();

        let rms = (dyn_rms_l + dyn_rms_r) * 0.5;

        let centroid = if rms > -80.0 {
            (spec_c_l + spec_c_r) * 0.5
        } else {
            0.0
        };
        let flatness = (spec_f_l + spec_f_r) * 0.5;
        let spec_crest = (spec_cr_l + spec_cr_r) * 0.5;

        let raw_crest = (dyn_crest_l + dyn_crest_r) * 0.5;
        let crest = if raw_crest.is_nan() {
            0.0
        } else {
            raw_crest.clamp(0.0, 30.0)
        };

        // Parked Finding (P3): dynamic_range is from left channel only
        let dyn_rng = dyn_rng_l;

        let true_peak_dbtp = if self.tp_max > 1e-10 {
            20.0 * libm::log10f(self.tp_max)
        } else {
            -144.0
        };

        let denom = libm::sqrtf(self.corr_sum_l * self.corr_sum_r);
        let corr = if denom < 1e-10 {
            1.0
        } else {
            (self.corr_cross / denom).clamp(-1.0, 1.0)
        };
        let width = 1.0 - libm::fabsf(corr);

        StemMetrics {
            spectral_centroid_hz: centroid,
            spectral_flatness: flatness,
            spectral_crest_factor: spec_crest,
            integrated_lufs: lufs,
            true_peak_dbtp,
            loudness_range: lra,
            rms_db: rms,
            stereo_correlation: corr,
            stereo_width: width,
            crest_factor_db: crest,
            dynamic_range_db: dyn_rng,
            energy_ratio: 0.0,      // set by caller
            transient_density: 0.0, // set by caller
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::features::{MixMetrics, StemMetrics};

    #[test]
    fn analyze_stereo_mix_metrics_reasonable() {
        // Sine-like signal: centroid should be above 0
        let signal: Vec<f32> = (0..4800)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();
        let result = StemFeatureAnalyzer::analyze_stereo(&signal, &signal, 48000);
        assert!(result.mix.spectral_centroid_hz > 0.0);
        assert!(result.mix.stereo_correlation > 0.99); // L==R
        assert_eq!(result.mix.stem_energy_ratios, [0.0; 5]);
    }

    #[test]
    fn test_analyze_stereo_true_peak() {
        let gain = libm::powf(10.0, -1.0 / 20.0);
        let signal: Vec<f32> = (0..4800)
            .map(|i| gain * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();

        let result = StemFeatureAnalyzer::analyze_stereo(&signal, &signal, 48000);

        assert!(
            result.mix.true_peak_dbtp >= -2.0 && result.mix.true_peak_dbtp <= 0.0,
            "True peak was {}",
            result.mix.true_peak_dbtp
        );

        let silence = vec![0.0_f32; 100];
        let silence_result = StemFeatureAnalyzer::analyze_stereo(&silence, &silence, 48000);
        assert!(silence_result.mix.true_peak_dbtp <= -100.0);
    }

    #[test]
    fn analyze_stereo_empty_returns_default() {
        let result = StemFeatureAnalyzer::analyze_stereo(&[], &[], 48000);
        assert_eq!(
            result.mix.integrated_lufs,
            MixMetrics::default().integrated_lufs
        );
    }

    #[test]
    fn analyze_stereo_stems_are_default() {
        let signal = vec![0.1_f32; 4800];
        let result = StemFeatureAnalyzer::analyze_stereo(&signal, &signal, 48000);
        assert_eq!(result.bass, StemMetrics::default());
        assert_eq!(result.harmonics, StemMetrics::default());
        assert_eq!(result.drums, StemMetrics::default());
        assert_eq!(result.ambience, StemMetrics::default());
    }

    #[test]
    fn streaming_stem_matches_offline_reference() {
        let sr = 48000;
        let block_len = sr as usize * 3; // 3 seconds

        let mut full_signal_identical = Vec::new();
        let mut full_signal_left = Vec::new();
        let mut full_signal_right = Vec::new();
        let mut full_signal_quiet = Vec::new();
        let mut full_signal_negative_corr = Vec::new();

        for i in 0..(block_len * 2) {
            let t = i as f32 / sr as f32;
            let sine_l = libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t);
            let sine_r = libm::sinf(2.0 * core::f32::consts::PI * 880.0 * t);
            let noise = ((i * 1103515245 + 12345) % 2147483648) as f32 / 2147483648.0 * 2.0 - 1.0;
            let env = if i < block_len { 1.0 } else { 0.1 };

            let val_id = (sine_l * 0.9 + noise * 0.1) * env;
            full_signal_identical.push(val_id);
            full_signal_identical.push(val_id);

            // Negative correlation fixture: Right channel is inverted
            full_signal_negative_corr.push(val_id);
            full_signal_negative_corr.push(-val_id);

            let amp_l = if i < block_len / 2 { 1.0 } else { 0.3 };
            let amp_r = if i > block_len { 1.0 } else { 0.2 };
            full_signal_left.push((sine_l * 0.9 + noise * 0.1) * amp_l);
            full_signal_right.push((sine_r * 0.7 + noise * 0.3) * amp_r);

            // Quiet fixture: triggers the `rms > -80.0` centroid gate.
            let quiet_val = sine_l * 1e-6;
            full_signal_quiet.push(quiet_val);
            full_signal_quiet.push(quiet_val);
        }

        let mut full_signal_distinct = Vec::new();
        for i in 0..(block_len * 2) {
            full_signal_distinct.push(full_signal_left[i]);
            full_signal_distinct.push(full_signal_right[i]);
        }

        let signal_lengths = [
            0,                      // length 0
            1,                      // length 1
            sr as usize,            // ~1s
            sr as usize * 4,        // ~4s
            sr as usize * 2 + 1500, // non-multiple
        ];

        let chunk_sizes = [4800, 4801, 1024, 48000, 1];

        let run_matrix = |interleaved_signal: &[f32], name: &str| {
            for &len in &signal_lengths {
                let sig = &interleaved_signal[..len * 2];
                let offline = StemFeatureAnalyzer::analyze_stem(sig, sr);

                let mut l_full = Vec::with_capacity(len);
                let mut r_full = Vec::with_capacity(len);
                for chunk in sig.chunks(2) {
                    l_full.push(chunk[0]);
                    r_full.push(chunk[1]);
                }

                for &csize in &chunk_sizes {
                    let mut streaming = StreamingStemAnalyzer::new(sr);
                    let mut offset = 0;
                    while offset < len {
                        let clen = csize.min(len - offset);
                        streaming.feed_chunk(
                            &l_full[offset..offset + clen],
                            &r_full[offset..offset + clen],
                        );
                        offset += clen;
                    }
                    let online = streaming.finish();

                    assert_eq!(
                        offline.spectral_centroid_hz.to_bits(),
                        online.spectral_centroid_hz.to_bits(),
                        "centroid mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.spectral_flatness.to_bits(),
                        online.spectral_flatness.to_bits(),
                        "flatness mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.spectral_crest_factor.to_bits(),
                        online.spectral_crest_factor.to_bits(),
                        "spec_crest mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.integrated_lufs.to_bits(),
                        online.integrated_lufs.to_bits(),
                        "lufs mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.true_peak_dbtp.to_bits(),
                        online.true_peak_dbtp.to_bits(),
                        "true_peak mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.loudness_range.to_bits(),
                        online.loudness_range.to_bits(),
                        "lra mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.rms_db.to_bits(),
                        online.rms_db.to_bits(),
                        "rms mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.stereo_correlation.to_bits(),
                        online.stereo_correlation.to_bits(),
                        "stereo_corr mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.stereo_width.to_bits(),
                        online.stereo_width.to_bits(),
                        "stereo_width mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.crest_factor_db.to_bits(),
                        online.crest_factor_db.to_bits(),
                        "crest mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.dynamic_range_db.to_bits(),
                        online.dynamic_range_db.to_bits(),
                        "dyn_rng mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.energy_ratio.to_bits(),
                        online.energy_ratio.to_bits(),
                        "energy mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                    assert_eq!(
                        offline.transient_density.to_bits(),
                        online.transient_density.to_bits(),
                        "transient mismatch on {} (len: {}, chunk: {})",
                        name,
                        len,
                        csize
                    );
                }

                // Mixed sequence
                let mut streaming = StreamingStemAnalyzer::new(sr);
                let mut offset = 0;
                let mut idx = 0;
                while offset < len {
                    let clen = chunk_sizes[idx % chunk_sizes.len()].min(len - offset);
                    streaming.feed_chunk(
                        &l_full[offset..offset + clen],
                        &r_full[offset..offset + clen],
                    );
                    offset += clen;
                    idx += 1;
                }
                let online = streaming.finish();

                assert_eq!(
                    offline.spectral_centroid_hz.to_bits(),
                    online.spectral_centroid_hz.to_bits(),
                    "centroid mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.spectral_flatness.to_bits(),
                    online.spectral_flatness.to_bits(),
                    "flatness mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.spectral_crest_factor.to_bits(),
                    online.spectral_crest_factor.to_bits(),
                    "spec_crest mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.integrated_lufs.to_bits(),
                    online.integrated_lufs.to_bits(),
                    "lufs mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.true_peak_dbtp.to_bits(),
                    online.true_peak_dbtp.to_bits(),
                    "true_peak mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.loudness_range.to_bits(),
                    online.loudness_range.to_bits(),
                    "lra mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.rms_db.to_bits(),
                    online.rms_db.to_bits(),
                    "rms mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.stereo_correlation.to_bits(),
                    online.stereo_correlation.to_bits(),
                    "stereo_corr mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.stereo_width.to_bits(),
                    online.stereo_width.to_bits(),
                    "stereo_width mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.crest_factor_db.to_bits(),
                    online.crest_factor_db.to_bits(),
                    "crest mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.dynamic_range_db.to_bits(),
                    online.dynamic_range_db.to_bits(),
                    "dyn_rng mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.energy_ratio.to_bits(),
                    online.energy_ratio.to_bits(),
                    "energy mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
                assert_eq!(
                    offline.transient_density.to_bits(),
                    online.transient_density.to_bits(),
                    "transient mismatch on {} (len: {}, chunk=mixed)",
                    name,
                    len
                );
            }
        };

        run_matrix(&full_signal_identical, "Identical L/R");
        run_matrix(&full_signal_distinct, "Distinct L/R");
        run_matrix(
            &full_signal_negative_corr,
            "Negative Correlation (Right = -Left)",
        );
        run_matrix(
            &full_signal_quiet,
            "Quiet (exercises rms > -80.0 centroid gate)",
        );
    }
}
