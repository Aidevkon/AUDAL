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
        let ratio_sum = bass_ratio + harmonics_ratio + voice_ratio + drums_ratio + ambience_ratio;
        if mix_energy >= 1e-10 {
            debug_assert!(
                (1.0 - ENERGY_RATIO_EPSILON..=1.0 + ENERGY_RATIO_EPSILON).contains(&ratio_sum),
                "energy ratio sum out of bounds: bass={bass_ratio} harmonics={harmonics_ratio} \
                 voice={voice_ratio} drums={drums_ratio} ambience={ambience_ratio} sum={ratio_sum}"
            );
        } else {
            debug_assert_eq!(
                ratio_sum, 0.0,
                "energy ratio sum must be 0.0 for silent mix: bass={bass_ratio} \
                 harmonics={harmonics_ratio} voice={voice_ratio} drums={drums_ratio} \
                 ambience={ambience_ratio} sum={ratio_sum}"
            );
        }

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
            dynamic_range_db: (dynamic_range_db(&mix_l, sample_rate)
                + dynamic_range_db(&mix_r, sample_rate))
                * 0.5,
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
        let dyn_rng = (dynamic_range_db(&l, sample_rate) + dynamic_range_db(&r, sample_rate)) * 0.5;

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
        let (dyn_rms_r, dyn_crest_r, dyn_rng_r) = self.dyn_r.finish();

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

        // Dynamic range: average of L and R channels (P3 fix matches offline)
        let dyn_rng = (dyn_rng_l + dyn_rng_r) * 0.5;

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

/// Streaming counterpart of `StemFeatureAnalyzer::analyze()`.
///
/// Accepts five interleaved stem chunks per call, accumulates per-stem and
/// per-mix state, and emits `StemFeatures` on `finish()` that is bit-identical
/// to the offline oracle when fed the same samples in any order of chunks.
///
/// Memory: bounded — six `StreamingStemAnalyzer` instances, five f32 energy
/// accumulators, and two chunk-sized scratch `Vec<f32>` buffers that are
/// allocated once in `new()` and grown lazily (never shrunk). No full-mix
/// buffer is ever materialised.
///
/// Note: the mix `StreamingStemAnalyzer` is also used for spectral analysis,
/// even though `MixMetrics` does not store a waveform-derived centroid. Its
/// spectral outputs are simply discarded in `finish()`. A leaner mix-only
/// analyzer (LUFS, LRA, true-peak, stereo) is a possible later optimisation.
pub struct StreamingStemFeaturesAnalyzer {
    sr: u32,
    stem_ba: StreamingStemAnalyzer,
    stem_ha: StreamingStemAnalyzer,
    stem_vo: StreamingStemAnalyzer,
    stem_dr: StreamingStemAnalyzer,
    stem_am: StreamingStemAnalyzer,
    mix_an: StreamingStemAnalyzer,
    /// Energy accumulators: folded over the INTERLEAVED sequence, one sample
    /// at a time — `sum += s * s` — to match `stereo_energy`'s left-fold.
    e_ba: f32,
    e_ha: f32,
    e_vo: f32,
    e_dr: f32,
    e_am: f32,
    /// Scratch: de-interleaved L channel (reused every feed_chunk, grown lazily).
    scratch_l: Vec<f32>,
    /// Scratch: de-interleaved R channel (reused every feed_chunk, grown lazily).
    scratch_r: Vec<f32>,
    /// Scratch: summed interleaved mix (reused every feed_chunk, grown lazily).
    scratch_mix: Vec<f32>,
}

impl StreamingStemFeaturesAnalyzer {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sr: sample_rate,
            stem_ba: StreamingStemAnalyzer::new(sample_rate),
            stem_ha: StreamingStemAnalyzer::new(sample_rate),
            stem_vo: StreamingStemAnalyzer::new(sample_rate),
            stem_dr: StreamingStemAnalyzer::new(sample_rate),
            stem_am: StreamingStemAnalyzer::new(sample_rate),
            mix_an: StreamingStemAnalyzer::new(sample_rate),
            e_ba: 0.0,
            e_ha: 0.0,
            e_vo: 0.0,
            e_dr: 0.0,
            e_am: 0.0,
            scratch_l: Vec::new(),
            scratch_r: Vec::new(),
            scratch_mix: Vec::new(),
        }
    }

    /// Feed one chunk of five interleaved stem buffers.
    ///
    /// Preconditions (asserted in debug builds):
    ///  - All five slices have exactly the same length `n`.
    ///  - `n` is even (complete L/R frame pairs).
    ///  - Order: [bass, harmonics, voice, drums, ambience], matching `FiveStems`.
    ///
    /// This mirrors `combine_stereo_five`'s behaviour: the mix is the
    /// element-wise sum bass[i] + harmonics[i] + voice[i] + drums[i] +
    /// ambience[i], and is truncated to `min(lengths)` — but since all five
    /// slices must have equal length, that is just `n`.
    pub fn feed_chunk(&mut self, stems: [&[f32]; 5]) {
        let [ba, ha, vo, dr, am] = stems;
        let n = ba.len();
        debug_assert_eq!(n, ha.len(), "stem chunk lengths must be equal");
        debug_assert_eq!(n, vo.len(), "stem chunk lengths must be equal");
        debug_assert_eq!(n, dr.len(), "stem chunk lengths must be equal");
        debug_assert_eq!(n, am.len(), "stem chunk lengths must be equal");
        debug_assert!(
            n % 2 == 0,
            "chunk length must be even (complete L/R frames)"
        );

        if n == 0 {
            return;
        }

        let n_frames = n / 2;

        // Grow scratch buffers if needed (never shrunk — amortised O(1) alloc).
        if self.scratch_l.len() < n_frames {
            self.scratch_l.resize(n_frames, 0.0);
            self.scratch_r.resize(n_frames, 0.0);
        }
        if self.scratch_mix.len() < n {
            self.scratch_mix.resize(n, 0.0);
        }

        // Build summed mix interleaved chunk and feed all six analyzers.
        // Process each stem: fold energy over interleaved order, de-interleave,
        // feed its StreamingStemAnalyzer.
        {
            // bass — fold energy directly into self.e_ba per sample, matching
            // stereo_energy's single left-fold over the interleaved buffer.
            for (idx, &s) in ba.iter().enumerate() {
                self.e_ba += s * s;
                if idx % 2 == 0 {
                    self.scratch_l[idx / 2] = s;
                } else {
                    self.scratch_r[idx / 2] = s;
                }
            }
            self.stem_ba
                .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
        }
        {
            // harmonics
            for (idx, &s) in ha.iter().enumerate() {
                self.e_ha += s * s;
                if idx % 2 == 0 {
                    self.scratch_l[idx / 2] = s;
                } else {
                    self.scratch_r[idx / 2] = s;
                }
            }
            self.stem_ha
                .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
        }
        {
            // voice
            for (idx, &s) in vo.iter().enumerate() {
                self.e_vo += s * s;
                if idx % 2 == 0 {
                    self.scratch_l[idx / 2] = s;
                } else {
                    self.scratch_r[idx / 2] = s;
                }
            }
            self.stem_vo
                .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
        }
        {
            // drums
            for (idx, &s) in dr.iter().enumerate() {
                self.e_dr += s * s;
                if idx % 2 == 0 {
                    self.scratch_l[idx / 2] = s;
                } else {
                    self.scratch_r[idx / 2] = s;
                }
            }
            self.stem_dr
                .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
        }
        {
            // ambience
            for (idx, &s) in am.iter().enumerate() {
                self.e_am += s * s;
                if idx % 2 == 0 {
                    self.scratch_l[idx / 2] = s;
                } else {
                    self.scratch_r[idx / 2] = s;
                }
            }
            self.stem_am
                .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
        }

        // Build summed mix chunk: bass[i] + harmonics[i] + voice[i] + drums[i] + ambience[i]
        // exactly matching combine_stereo_five's expression and iteration order.
        for i in 0..n {
            self.scratch_mix[i] = ba[i] + ha[i] + vo[i] + dr[i] + am[i];
        }
        // De-interleave mix into scratch_l / scratch_r and feed mix analyzer.
        for f in 0..n_frames {
            self.scratch_l[f] = self.scratch_mix[f * 2];
            self.scratch_r[f] = self.scratch_mix[f * 2 + 1];
        }
        self.mix_an
            .feed_chunk(&self.scratch_l[..n_frames], &self.scratch_r[..n_frames]);
    }

    /// Combine per-stem state into `StemFeatures` bit-identical to
    /// `StemFeatureAnalyzer::analyze()`.
    ///
    /// `transient_densities`: [bass, harmonics, voice, drums, ambience],
    /// matching the order `analyze()` copies from `FiveStems`.
    pub fn finish(self, transient_densities: [f32; 5]) -> StemFeatures {
        let sr = self.sr;
        let _ = sr; // used in StreamingStemAnalyzer::new; kept for symmetry

        let mut bass = self.stem_ba.finish();
        let mut harmonics = self.stem_ha.finish();
        let mut voice = self.stem_vo.finish();
        let mut drums = self.stem_dr.finish();
        let mut ambience = self.stem_am.finish();

        // Mirror analyze()'s transient_density assignment order exactly.
        voice.transient_density = transient_densities[2];
        drums.transient_density = transient_densities[3];
        bass.transient_density = transient_densities[0];
        harmonics.transient_density = transient_densities[1];
        ambience.transient_density = transient_densities[4];

        // Energy and ratios — same expression as energy_ratio() / stereo_energy().
        let mix_energy = self.e_ba + self.e_ha + self.e_vo + self.e_dr + self.e_am;

        let ratio = |e: f32| -> f32 {
            if mix_energy < 1e-10 {
                0.0
            } else {
                (e / mix_energy).clamp(0.0, 1.0)
            }
        };
        let bass_ratio = ratio(self.e_ba);
        let harmonics_ratio = ratio(self.e_ha);
        let voice_ratio = ratio(self.e_vo);
        let drums_ratio = ratio(self.e_dr);
        let ambience_ratio = ratio(self.e_am);

        let ratio_sum = bass_ratio + harmonics_ratio + voice_ratio + drums_ratio + ambience_ratio;
        if mix_energy >= 1e-10 {
            debug_assert!(
                (1.0 - ENERGY_RATIO_EPSILON..=1.0 + ENERGY_RATIO_EPSILON).contains(&ratio_sum),
                "energy ratio sum out of bounds: bass={bass_ratio} harmonics={harmonics_ratio} \
                 voice={voice_ratio} drums={drums_ratio} ambience={ambience_ratio} sum={ratio_sum}"
            );
        } else {
            debug_assert_eq!(
                ratio_sum, 0.0,
                "energy ratio sum must be 0.0 for silent mix: bass={bass_ratio} \
                 harmonics={harmonics_ratio} voice={voice_ratio} drums={drums_ratio} \
                 ambience={ambience_ratio} sum={ratio_sum}"
            );
        }
        // analyze() does NOT write energy_ratio back to per-stem StemMetrics;
        // those fields stay 0.0 from StreamingStemAnalyzer::finish(). Match exactly.

        // Mix centroid: energy-weighted average of stem centroids (S-008).
        // Verbatim from analyze() — ratios/centroids arrays, total_w guard,
        // map/sum/divide and 1000.0 fallback.
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

        // Assemble MixMetrics from mix analyzer's StemMetrics.
        // Spectral outputs of mix_stem are discarded (MixMetrics has no
        // waveform-derived centroid; centroid computed above from stem ratios).
        let mix_stem = self.mix_an.finish();
        let mix = MixMetrics {
            integrated_lufs: mix_stem.integrated_lufs,
            true_peak_dbtp: mix_stem.true_peak_dbtp,
            loudness_range: mix_stem.loudness_range,
            stereo_correlation: mix_stem.stereo_correlation,
            stereo_width: mix_stem.stereo_width,
            // dynamic_range from left channel only — matches P3 in analyze().
            dynamic_range_db: mix_stem.dynamic_range_db,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::features::{MixMetrics, StemMetrics};

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

    /// Builds a `FiveStems` with five DISTINCT interleaved stem signals plus
    /// five distinct transient_density values, then asserts bit-identical
    /// equality on all 13 StemMetrics fields for each stem and all 8 MixMetrics
    /// fields across a full chunk-size × signal-length matrix.
    ///
    /// Fixture 2 (one silent stem) exercises the mix_energy < 1e-10 ratio guard
    /// and the energy_ratio == 0.0 path for the silent stem.
    #[test]
    fn streaming_stem_features_matches_offline_reference() {
        use crate::stft::stem_renderer::FiveStems;

        let sr: u32 = 48000;
        let n_frames_full = sr as usize * 4; // ~4 s of frames

        // ── Fixture 1: five distinct interleaved signals ─────────────────────
        let make_stem = |freq_l: f32, freq_r: f32, amp_l: f32, amp_r: f32| -> Vec<f32> {
            let mut v = Vec::with_capacity(n_frames_full * 2);
            for i in 0..n_frames_full {
                let t = i as f32 / sr as f32;
                // Distinct L and R channels per stem.
                let l = amp_l * libm::sinf(2.0 * core::f32::consts::PI * freq_l * t);
                let r = amp_r * libm::sinf(2.0 * core::f32::consts::PI * freq_r * t);
                // Amplitude envelope: full first half, quiet second half.
                let env = if i < n_frames_full / 2 { 1.0 } else { 0.15 };
                v.push(l * env);
                v.push(r * env);
            }
            v
        };

        let bass_full = make_stem(80.0, 100.0, 0.9, 0.8);
        let harm_full = make_stem(440.0, 550.0, 0.7, 0.65);
        let voice_full = make_stem(1200.0, 1400.0, 0.6, 0.55);
        let drums_full = make_stem(200.0, 250.0, 0.85, 0.82);
        let ambi_full = make_stem(3000.0, 3500.0, 0.4, 0.45);

        let td_full: [f32; 5] = [0.12, 0.35, 0.58, 0.72, 0.09];

        // ── Fixture 2: drums stem is entirely silent ─────────────────────────
        // Exercises: mix_energy guard, drums energy_ratio == 0.0.
        let drums_silent = vec![0.0f32; n_frames_full * 2];
        let td_silent: [f32; 5] = [0.11, 0.30, 0.50, 0.00, 0.08];

        // ── Signal lengths in frames ─────────────────────────────────────────
        let frame_lengths: &[usize] = &[
            0,
            1,
            sr as usize,            // ~1 s
            sr as usize * 4,        // ~4 s
            sr as usize * 2 + 1500, // non-multiple
        ];

        // ── Chunk sizes (interleaved samples = 2 * frames) ───────────────────
        // 2400 frames → 4800 samples; 2401 frames → 4802 samples (odd-frame);
        // 512 frames → 1024 samples; 1 frame → 2 samples; 24000 frames → 48000 samples.
        let chunk_frames: &[usize] = &[2400, 2401, 512, 1, 24000];

        // ── Run parity matrix ────────────────────────────────────────────────
        let run = |bass: &[f32],
                   harm: &[f32],
                   voice: &[f32],
                   drums: &[f32],
                   ambi: &[f32],
                   tds: [f32; 5],
                   fixture_name: &str| {
            for &len in frame_lengths {
                let ba = &bass[..len * 2];
                let ha = &harm[..len * 2];
                let vo = &voice[..len * 2];
                let dr = &drums[..len * 2];
                let am = &ambi[..len * 2];

                // Build FiveStems for the offline oracle.
                let five = FiveStems {
                    bass: ba.to_vec(),
                    harmonics: ha.to_vec(),
                    voice: vo.to_vec(),
                    drums: dr.to_vec(),
                    ambience: am.to_vec(),
                    bass_transient_density: tds[0],
                    harmonics_transient_density: tds[1],
                    voice_transient_density: tds[2],
                    drums_transient_density: tds[3],
                    ambience_transient_density: tds[4],
                };
                let offline = StemFeatureAnalyzer::analyze(&five, sr);

                // Helper: assert all 13 StemMetrics fields.
                let assert_stem =
                    |off: &StemMetrics, on: &StemMetrics, stem: &str, chunk_desc: &str| {
                        macro_rules! chk {
                            ($field:ident) => {
                                assert_eq!(
                                    off.$field.to_bits(),
                                    on.$field.to_bits(),
                                    "{}/{}: {} mismatch (fixture={}, len={}, {})",
                                    stem,
                                    stringify!($field),
                                    stringify!($field),
                                    fixture_name,
                                    len,
                                    chunk_desc,
                                );
                            };
                        }
                        chk!(spectral_centroid_hz);
                        chk!(spectral_flatness);
                        chk!(spectral_crest_factor);
                        chk!(integrated_lufs);
                        chk!(true_peak_dbtp);
                        chk!(loudness_range);
                        chk!(rms_db);
                        chk!(stereo_correlation);
                        chk!(stereo_width);
                        chk!(crest_factor_db);
                        chk!(dynamic_range_db);
                        chk!(energy_ratio);
                        chk!(transient_density);
                    };

                // Helper: assert all 8 MixMetrics fields.
                let assert_mix = |off: &MixMetrics, on: &MixMetrics, chunk_desc: &str| {
                    macro_rules! chk {
                        ($field:ident) => {
                            assert_eq!(
                                off.$field.to_bits(),
                                on.$field.to_bits(),
                                "mix/{}: {} mismatch (fixture={}, len={}, {})",
                                stringify!($field),
                                stringify!($field),
                                fixture_name,
                                len,
                                chunk_desc,
                            );
                        };
                    }
                    chk!(integrated_lufs);
                    chk!(true_peak_dbtp);
                    chk!(loudness_range);
                    chk!(stereo_correlation);
                    chk!(stereo_width);
                    chk!(dynamic_range_db);
                    chk!(spectral_centroid_hz);
                    for idx in 0..5 {
                        assert_eq!(
                            off.stem_energy_ratios[idx].to_bits(),
                            on.stem_energy_ratios[idx].to_bits(),
                            "mix/stem_energy_ratios[{}] mismatch (fixture={}, len={}, {})",
                            idx,
                            fixture_name,
                            len,
                            chunk_desc,
                        );
                    }
                };

                // Fixed chunk sizes.
                for &cf in chunk_frames {
                    let csamples = cf * 2; // interleaved samples per chunk
                    let desc = format!("chunk={}frames", cf);

                    let mut online = StreamingStemFeaturesAnalyzer::new(sr);
                    let mut offset = 0; // in frames
                    while offset < len {
                        let frames_this = cf.min(len - offset);
                        let s = offset * 2;
                        let e = s + frames_this * 2;
                        online.feed_chunk([&ba[s..e], &ha[s..e], &vo[s..e], &dr[s..e], &am[s..e]]);
                        offset += frames_this;
                    }
                    let _ = csamples; // suppress unused warning
                    let on = online.finish(tds);

                    assert_stem(&offline.bass, &on.bass, "bass", &desc);
                    assert_stem(&offline.harmonics, &on.harmonics, "harmonics", &desc);
                    assert_stem(&offline.voice, &on.voice, "voice", &desc);
                    assert_stem(&offline.drums, &on.drums, "drums", &desc);
                    assert_stem(&offline.ambience, &on.ambience, "ambience", &desc);
                    assert_mix(&offline.mix, &on.mix, &desc);
                }

                // Mixed (uneven) sequence.
                {
                    let desc = "chunk=mixed";
                    let mixed_cf = [2400, 2401, 512, 1, 24000];
                    let mut online = StreamingStemFeaturesAnalyzer::new(sr);
                    let mut offset = 0;
                    let mut idx = 0;
                    while offset < len {
                        let cf = mixed_cf[idx % mixed_cf.len()];
                        let frames_this = cf.min(len - offset);
                        let s = offset * 2;
                        let e = s + frames_this * 2;
                        online.feed_chunk([&ba[s..e], &ha[s..e], &vo[s..e], &dr[s..e], &am[s..e]]);
                        offset += frames_this;
                        idx += 1;
                    }
                    let on = online.finish(tds);

                    assert_stem(&offline.bass, &on.bass, "bass", desc);
                    assert_stem(&offline.harmonics, &on.harmonics, "harmonics", desc);
                    assert_stem(&offline.voice, &on.voice, "voice", desc);
                    assert_stem(&offline.drums, &on.drums, "drums", desc);
                    assert_stem(&offline.ambience, &on.ambience, "ambience", desc);
                    assert_mix(&offline.mix, &on.mix, desc);
                }
            }
        };

        // Fixture 1: five distinct stems.
        run(
            &bass_full,
            &harm_full,
            &voice_full,
            &drums_full,
            &ambi_full,
            td_full,
            "Distinct5",
        );

        // Fixture 2: drums stem entirely silent — exercises mix_energy guard.
        run(
            &bass_full,
            &harm_full,
            &voice_full,
            &drums_silent,
            &ambi_full,
            td_silent,
            "DrumsSilent",
        );

        // Fixture 3: ALL five stems entirely silent.
        // This is the only fixture that exercises BOTH:
        //   - the mix_energy < 1e-10 -> 0.0 ratio guard (on any non-empty signal), AND
        //   - the total_w > 1e-10 -> 1000.0 mix-centroid fallback (all ratios are 0.0,
        //     so total_w == 0.0 for every non-empty length).
        let all_silent = vec![0.0f32; n_frames_full * 2];
        let td_all_silent: [f32; 5] = [0.0; 5];
        run(
            &all_silent,
            &all_silent,
            &all_silent,
            &all_silent,
            &all_silent,
            td_all_silent,
            "AllSilent",
        );
    }
}
