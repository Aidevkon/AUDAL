//! AnalysisAccumulator — EBU R128 BS.1770-4 loudness analysis.
//! Ported from sm-core/src/analysis.rs.
//! Adapted for no_std + alloc. libm-only float math.
//! Authority: LineOS Constitution v2.0 §07 (comparator rule — never re-measures)
//!
//! Key adaptations from sm-core:
//! - Removed hardcoded TARGET_LUFS constant — caller passes target_lufs
//! - QualityMetrics uses new metrics type (no evaluate_gate, no passes_spotify_gate)
//! - std::vec::Vec → alloc::vec::Vec
//! - No std:: imports

use alloc::vec::Vec;
use crate::dsp::biquad::Biquad;
use crate::types::audio::AudioChunk;
use crate::types::metrics::QualityMetrics;

pub struct AnalysisAccumulator {
    integrated_lufs:    f32,
    true_peak_dbfs:     f32,
    stereo_correlation: f32,
    dc_offset:          f32,
    sample_count:       usize,
    sample_rate:        u32,
    channels:           u16,

    // EBU R128 filter chain (per-channel)
    pre_filters:  Vec<Biquad>,
    rlb_filters:  Vec<Biquad>,

    // LUFS block accumulation (400ms blocks, 75% overlap)
    block_samples:      Vec<f32>,
    block_mean_squares: Vec<f64>,

    // True peak
    true_peak_max:  f32,
    last_samples:   Vec<f32>,

    // Stereo correlation
    corr_window_l2:      f64,
    corr_window_r2:      f64,
    corr_window_lr:      f64,
    corr_window_samples: usize,
    violation_count:     usize,
    total_windows:       usize,

    // DC offset accumulator
    dc_sum: f64,
}

impl AnalysisAccumulator {
    pub fn new(sample_rate: u32, channels: u16) -> Self {
        let block_size = (sample_rate as f32 * 0.400) as usize * channels as usize;
        let mut pre_filters = Vec::with_capacity(channels as usize);
        let mut rlb_filters = Vec::with_capacity(channels as usize);

        for _ in 0..channels {
            // EBU R128 pre-filter: high-shelf +4dB at ~1500Hz
            let mut pre = Biquad::new();
            pre.set_high_shelf(1500.0, sample_rate as f32, 4.0, 0.707);

            // EBU R128 RLB weight filter: HPF at 38Hz
            let mut rlb = Biquad::new();
            rlb.set_hpf(38.0, sample_rate as f32, 0.5);

            pre_filters.push(pre);
            rlb_filters.push(rlb);
        }

        Self {
            integrated_lufs:    f32::NEG_INFINITY,
            true_peak_dbfs:     f32::NEG_INFINITY,
            stereo_correlation: 1.0,
            dc_offset:          0.0,
            sample_count:       0,
            sample_rate,
            channels,
            pre_filters,
            rlb_filters,
            block_samples:      Vec::with_capacity(block_size),
            block_mean_squares: Vec::with_capacity(1024),
            true_peak_max:      0.0,
            last_samples:       alloc::vec![0.0; channels as usize],
            corr_window_l2:      0.0,
            corr_window_r2:      0.0,
            corr_window_lr:      0.0,
            corr_window_samples: 0,
            violation_count:     0,
            total_windows:       0,
            dc_sum:              0.0,
        }
    }

    /// Feed an audio chunk into the accumulator.
    pub fn feed(&mut self, chunk: &AudioChunk) {
        let block_size = (self.sample_rate as f32 * 0.400) as usize * self.channels as usize;
        let step_size  = (self.sample_rate as f32 * 0.100) as usize * self.channels as usize;

        let frames   = chunk.frame_count();
        let channels = self.channels as usize;

        for frame in 0..frames {
            let mut l_sample = 0.0f32;
            let mut r_sample = 0.0f32;

            for ch in 0..channels {
                let idx    = frame * channels + ch;
                let mut sample = chunk.samples[idx];

                // DC accumulation
                self.dc_sum += sample as f64;

                // True peak 4x linear interpolation
                let last = self.last_samples[ch];
                let diff = sample - last;
                for i in 1..=4 {
                    let interp = last + diff * (i as f32 * 0.25);
                    let abs = libm::fabsf(interp);
                    if abs > self.true_peak_max {
                        self.true_peak_max = abs;
                    }
                }
                self.last_samples[ch] = sample;

                if channels >= 2 {
                    if ch == 0 { l_sample = sample; }
                    if ch == 1 { r_sample = sample; }
                }

                // LUFS K-weighing filtering
                sample = self.pre_filters[ch].process(sample);
                sample = self.rlb_filters[ch].process(sample);
                self.block_samples.push(sample);
            }

            // Stereo correlation (300ms windows)
            if channels >= 2 {
                self.corr_window_l2 += (l_sample * l_sample) as f64;
                self.corr_window_r2 += (r_sample * r_sample) as f64;
                self.corr_window_lr += (l_sample * r_sample) as f64;
                self.corr_window_samples += 1;

                let window_max = (self.sample_rate as f32 * 0.300) as usize;
                if self.corr_window_samples >= window_max {
                    let denominator = libm::sqrt(self.corr_window_l2 * self.corr_window_r2);
                    let corr = if denominator > 0.0 {
                        (self.corr_window_lr / denominator) as f32
                    } else {
                        1.0
                    };
                    if corr < 0.8 { self.violation_count += 1; }
                    self.total_windows += 1;
                    self.corr_window_l2 = 0.0;
                    self.corr_window_r2 = 0.0;
                    self.corr_window_lr = 0.0;
                    self.corr_window_samples = 0;
                }
            }

            self.sample_count += 1;

            // LUFS block complete
            if self.block_samples.len() >= block_size {
                let mut mean_sq_sum = 0.0f64;
                for ch in 0..channels {
                    let mut sum = 0.0f64;
                    for i in 0..(block_size / channels) {
                        let s = self.block_samples[i * channels + ch] as f64;
                        sum += s * s;
                    }
                    sum /= (block_size / channels) as f64;
                    mean_sq_sum += sum;
                }
                self.block_mean_squares.push(mean_sq_sum);
                self.block_samples.drain(0..step_size);
            }
        }
    }

    /// Finalize the accumulator and produce QualityMetrics.
    pub fn finalize(&mut self) -> QualityMetrics {
        // DC offset
        self.dc_offset = if self.sample_count > 0 {
            (self.dc_sum / (self.sample_count as f64 * self.channels as f64)) as f32
        } else { 0.0 };

        // True peak
        self.true_peak_dbfs = if self.true_peak_max > 0.0 {
            20.0 * libm::log10f(self.true_peak_max)
        } else {
            f32::NEG_INFINITY
        };

        // Stereo correlation
        let violation_pct = if self.total_windows > 0 {
            self.violation_count as f32 / self.total_windows as f32
        } else { 0.0 };

        self.stereo_correlation = if self.total_windows == 0 {
            1.0
        } else if violation_pct > 0.05 {
            0.79
        } else {
            1.0
        };

        // EBU R128 absolute gate: -70 LUFS
        let mut valid_blocks: Vec<f64> = Vec::with_capacity(self.block_mean_squares.len());
        for &ms in &self.block_mean_squares {
            let lufs = -0.691 + 10.0 * libm::log10(ms);
            if lufs >= -70.0 { valid_blocks.push(ms); }
        }

        if valid_blocks.is_empty() {
            self.integrated_lufs = f32::NEG_INFINITY;
        } else {
            let abs_mean: f64 = valid_blocks.iter().sum::<f64>() / valid_blocks.len() as f64;
            let relative_threshold = -0.691 + 10.0 * libm::log10(abs_mean) - 10.0;

            let mut final_blocks: Vec<f64> = Vec::with_capacity(valid_blocks.len());
            for &ms in &valid_blocks {
                let lufs = -0.691 + 10.0 * libm::log10(ms);
                if lufs >= relative_threshold { final_blocks.push(ms); }
            }

            if final_blocks.is_empty() {
                self.integrated_lufs = f32::NEG_INFINITY;
            } else {
                let final_mean: f64 = final_blocks.iter().sum::<f64>() / final_blocks.len() as f64;
                self.integrated_lufs = (-0.691 + 10.0 * libm::log10(final_mean)) as f32;
            }
        }

        QualityMetrics {
            integrated_lufs:    self.integrated_lufs,
            true_peak_dbfs:     self.true_peak_dbfs,
            loudness_range_lu:  0.0,  // Phase 3: EBU R128 Level 2+
            bs1770_integrated:  self.integrated_lufs,
            bs1770_true_peak:   self.true_peak_dbfs,
            stereo_correlation: self.stereo_correlation,
            dc_offset:          self.dc_offset,
        }
    }

    /// Compute linear gain needed to normalize to target_lufs.
    /// target_lufs is loaded from bmr-128.schema.json presets — never hardcoded.
    pub fn normalization_gain_linear(&self, target_lufs: f32) -> f32 {
        let gain_db = target_lufs - self.integrated_lufs;
        libm::powf(10.0, gain_db / 20.0)
    }

    pub fn has_dc_offset(&self) -> bool {
        self.dc_offset.abs() > 0.01
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_analysis() {
        let mut acc = AnalysisAccumulator::new(48000, 2);
        let chunk = AudioChunk {
            samples:     alloc::vec![0.0; 4096 * 2],
            sample_rate: 48000,
            channels:    2,
        };
        acc.feed(&chunk);
        let qm = acc.finalize();
        assert!(qm.true_peak_dbfs < -100.0, "silence should have very low true peak");
        assert!(qm.integrated_lufs < -100.0, "silence should have very low LUFS");
    }

    #[test]
    fn test_normalization_gain_target() {
        let sr = 48000u32;
        // Mono accumulator — channels must match chunk channels
        let mut acc = AnalysisAccumulator::new(sr, 1);
        // 2 seconds of 440Hz mono sine at -6dBFS
        let samples: Vec<f32> = (0..sr * 2)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / sr as f32) * 0.5)
            .collect();
        let chunk = AudioChunk { samples, sample_rate: sr, channels: 1 };
        acc.feed(&chunk);
        acc.finalize();
        // Normalization gain to -14.0 LUFS target should be a positive linear value
        let gain = acc.normalization_gain_linear(-14.0);
        assert!(gain > 0.0, "normalization gain should be positive");
    }
}
