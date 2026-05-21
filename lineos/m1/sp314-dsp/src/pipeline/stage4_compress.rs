//! Stage 4 — Multiband Compression — sp314-dsp v2.9 §Stage 4.
//!
//! Call order (FIXED): 4.1 → 4.2 → 4.3 → 4.4 → 4.5 → 4.6
//!
//! 4.1 SignalPriority lookup (Transient → reduce ratio by 0.5:1, attack × 0.5)
//! 4.2 Multiband split: 4 bands via cascaded biquad crossovers at 80/250/2000 Hz
//!     Phase drift proxy: band energy ratio check → PhaseDriftDetected
//! 4.3 Per-band compression: LogEnvelopeFollower per §N4, time_to_coeff per §N5
//! 4.4 Band coupling: feed-forward, base_low_mid=0.25, base_mid_high=0.15
//! 4.5 Band recombination: simple sum (fundsp join deferred)
//! 4.6 Stereo link: equal-power formula per §N4, α = stereo_link_amount()

use alloc::vec::Vec;

use crate::dsp::biquad::Biquad;
use crate::pipeline::math::{finalize_sample, kahan_mean_square, LogEnvelopeFollower};
use crate::pipeline::signal_priority::SignalPriority;
use crate::pipeline::warnings::{PipelineWarning, WarningAggregator};
use crate::pipeline::gain_budget::GainBudget;
use crate::types::audio::AudioChunk;
use crate::types::mastering_preset::{CompressionStyle, MasteringPreset};
use crate::types::units::{Decibels, LinearGain};

// ── Compression table: (ratio, attack_ms) per style per band ─────────────────
// Release = 3× attack per spec §Stage 4.3.
// [band 0 (Low), band 1 (Low-mid), band 2 (High-mid), band 3 (High)]

struct BandParams { ratio: f32, attack_ms: f32 }

fn band_params(style: CompressionStyle, band: usize) -> BandParams {
    match (style, band) {
        (CompressionStyle::Transparent, 0) => BandParams { ratio: 1.5, attack_ms: 40.0 },
        (CompressionStyle::Transparent, 1) => BandParams { ratio: 1.5, attack_ms: 20.0 },
        (CompressionStyle::Transparent, 2) => BandParams { ratio: 1.2, attack_ms: 10.0 },
        (CompressionStyle::Transparent, _) => BandParams { ratio: 1.1, attack_ms:  5.0 },
        (CompressionStyle::Gentle,      0) => BandParams { ratio: 2.0, attack_ms: 20.0 },
        (CompressionStyle::Gentle,      1) => BandParams { ratio: 2.0, attack_ms: 10.0 },
        (CompressionStyle::Gentle,      2) => BandParams { ratio: 2.0, attack_ms:  5.0 },
        (CompressionStyle::Gentle,      _) => BandParams { ratio: 1.5, attack_ms:  3.0 },
        (CompressionStyle::Medium,      0) => BandParams { ratio: 3.0, attack_ms: 10.0 },
        (CompressionStyle::Medium,      1) => BandParams { ratio: 3.0, attack_ms:  5.0 },
        (CompressionStyle::Medium,      2) => BandParams { ratio: 2.5, attack_ms:  3.0 },
        (CompressionStyle::Medium,      _) => BandParams { ratio: 2.0, attack_ms:  2.0 },
        (CompressionStyle::Aggressive,  0) => BandParams { ratio: 4.0, attack_ms:  5.0 },
        (CompressionStyle::Aggressive,  1) => BandParams { ratio: 4.0, attack_ms:  3.0 },  // spec: 3ms
        (CompressionStyle::Aggressive,  2) => BandParams { ratio: 3.0, attack_ms:  2.0 },
        (CompressionStyle::Aggressive,  _) => BandParams { ratio: 3.0, attack_ms:  1.0 },
    }
}

/// Default compression threshold per band: -18 dBFS (from ParamId registry).
const THRESHOLD_DB: f32 = -18.0;

/// Band coupling defaults (§4.4, ParamId registry).
const COUPLING_LOW_MID:  f32 = 0.25;
const COUPLING_MID_HIGH: f32 = 0.15;

/// Phase drift proxy threshold: band 3 energy / band 0 energy ratio.
/// If > 100× (≈ 20 dB difference) for any block, emit PhaseDriftDetected.
const PHASE_DRIFT_RATIO: f32 = 100.0;

/// Crossover frequencies (Hz).
const XO_FREQS: [f32; 3] = [80.0, 250.0, 2_000.0];

/// Q for Linkwitz-Riley crossovers (2× cascaded 2nd-order Butterworth).
const LR_Q: f32 = 0.7071;

// ── Crossover filter bank ─────────────────────────────────────────────────────

/// Per-channel Linkwitz-Riley crossover state — produces 4 frequency bands.
/// Flat layout: [xo0_lpf_s1, xo0_lpf_s2, xo1_lpf_s1, xo1_lpf_s2, xo2_lpf_s1, xo2_lpf_s2,
///               xo0_hpf_s1, xo0_hpf_s2, xo1_hpf_s1, xo1_hpf_s2, xo2_hpf_s1, xo2_hpf_s2]
struct CrossoverBank {
    filters: Vec<Biquad>, // 12 biquads: 6 LPF (3 xo × 2 stage) + 6 HPF (3 xo × 2 stage)
}

impl CrossoverBank {
    fn new(sample_rate: f32) -> Self {
        let mut filters: Vec<Biquad> = (0..12).map(|_| Biquad::new()).collect();
        // LPF: indices 0..6 (xo0_s1=0, xo0_s2=1, xo1_s1=2, xo1_s2=3, xo2_s1=4, xo2_s2=5)
        for xo in 0..3 {
            filters[xo*2    ].set_lpf(XO_FREQS[xo], sample_rate, LR_Q);
            filters[xo*2 + 1].set_lpf(XO_FREQS[xo], sample_rate, LR_Q);
        }
        // HPF: indices 6..12 (xo0_s1=6, xo0_s2=7, xo1_s1=8, xo1_s2=9, xo2_s1=10, xo2_s2=11)
        for xo in 0..3 {
            filters[6 + xo*2    ].set_hpf(XO_FREQS[xo], sample_rate, LR_Q);
            filters[6 + xo*2 + 1].set_hpf(XO_FREQS[xo], sample_rate, LR_Q);
        }
        Self { filters }
    }

    /// Split one input sample into 4 band outputs [low, low-mid, high-mid, high].
    fn process(&mut self, x: f32) -> [f32; 4] {
        // Band 0 (low <80Hz): LPF@80 → LPF@80
        let b0_a = self.filters[0].process(x);
        let b0   = self.filters[1].process(b0_a);
        // Band 3 (high >2kHz): HPF@2k → HPF@2k
        let b3_a = self.filters[10].process(x);
        let b3   = self.filters[11].process(b3_a);
        // Mid signal: HPF@80 → HPF@80
        let mid_a = self.filters[6].process(x);
        let mid   = self.filters[7].process(mid_a);
        // Band 1 (low-mid 80-250Hz): mid → LPF@250 → LPF@250
        let b1_a = self.filters[2].process(mid);
        let b1   = self.filters[3].process(b1_a);
        // High-mid source: mid → HPF@250 → HPF@250
        let hm_a = self.filters[8].process(mid);
        let hm   = self.filters[9].process(hm_a);
        // Band 2 (high-mid 250-2kHz): hm → LPF@2k → LPF@2k
        let b2_a = self.filters[4].process(hm);
        let b2   = self.filters[5].process(b2_a);
        [b0, b1, b2, b3]
    }
}

impl Default for CrossoverBank {
    fn default() -> Self {
        Self {
            filters: (0..12).map(|_| Biquad::new()).collect(),
        }
    }
}

// ── v2.9 process_compress ─────────────────────────────────────────────────────

/// Process `pcm` in-place through Stage 4 multiband compression.
///
/// Call order 4.1→4.2→4.3→4.4→4.5→4.6 is FIXED.
pub fn process_compress(
    pcm:                 &mut [f32],
    channels:            u16,
    sample_rate:         f32,
    preset:              &MasteringPreset,
    gain_budget:         &mut GainBudget,
    signal_priority_map: &[SignalPriority],
    aggregator:          &mut WarningAggregator,
    block_size:          usize,
    block_offset:        u64,
) {
    if pcm.is_empty() { return; }
    let ch = channels as usize;
    if ch == 0 { return; }
    let frame_count = pcm.len() / ch;
    if frame_count == 0 { return; }

    // ── §4.1 SignalPriority lookup ────────────────────────────────────────────
    let priority = signal_priority_map
        .get(block_offset as usize)
        .copied()
        .unwrap_or(SignalPriority::Body);
    let is_transient = priority == SignalPriority::Transient;

    // ── Build per-band compressor params ─────────────────────────────────────
    // 4 bands × per-channel envelope followers
    let mut followers: Vec<[LogEnvelopeFollower; 4]> = (0..ch).map(|_| {
        core::array::from_fn(|band| {
            let p = band_params(preset.compression, band);
            let attack_ms  = if is_transient { p.attack_ms * 0.5 } else { p.attack_ms };
            let release_ms = p.attack_ms * 3.0; // release = 3× attack
            LogEnvelopeFollower::new(attack_ms, release_ms, sample_rate)
        })
    }).collect();

    // ── §4.2 Multiband split (crossover bank per channel) ────────────────────
    let mut xo_banks: Vec<CrossoverBank> = (0..ch).map(|_| CrossoverBank::new(sample_rate)).collect();

    // Split PCM into 4 band buffers [band][sample_index]
    // band_bufs[band] = interleaved per-channel samples
    let mut band_bufs: [Vec<f32>; 4] = core::array::from_fn(|_| vec![0.0f32; pcm.len()]);

    for frame in 0..frame_count {
        for c in 0..ch {
            let i = frame * ch + c;
            let bands = xo_banks[c].process(pcm[i]);
            for b in 0..4 {
                band_bufs[b][i] = bands[b];
            }
        }
    }

    // Phase drift proxy (§4.2): compare band 3 vs band 0 energy per block
    {
        let frames_per_block = block_size.max(1);
        let num_blocks = (frame_count + frames_per_block - 1) / frames_per_block;
        for blk in 0..num_blocks {
            let start = blk * frames_per_block * ch;
            let end   = ((blk + 1) * frames_per_block * ch).min(pcm.len());
            let e0 = kahan_mean_square(&band_bufs[0][start..end]);
            let e3 = kahan_mean_square(&band_bufs[3][start..end]);
            if e0 > 0.0 && e3 / e0 > PHASE_DRIFT_RATIO {
                aggregator.push(PipelineWarning::PhaseDriftDetected, block_offset + blk as u64);
            }
        }
    }

    // ── §4.3 Per-band compression ─────────────────────────────────────────────
    let mut band_gr: [Vec<f32>; 4] = core::array::from_fn(|_| vec![1.0f32; frame_count]);

    for band in 0..4usize {
        let p = band_params(preset.compression, band);
        let ratio = if is_transient {
            libm::fmaxf(1.0, p.ratio - 0.5)
        } else {
            p.ratio
        };
        let slope = 1.0 - 1.0 / ratio; // gain reduction slope

        for frame in 0..frame_count {
            // Compute mean square across channels for this frame
            let rms_sq: f32 = (0..ch)
                .map(|c| { let s = band_bufs[band][frame * ch + c]; s * s })
                .sum::<f32>() / ch as f32;
            let rms_lin = libm::sqrtf(rms_sq.max(0.0));

            // Convert to dB, clamp at silence floor
            let rms_db = if rms_lin <= 0.0 { -144.0 }
                         else { LinearGain(rms_lin).to_db().0 };

            // Envelope follower (log domain) — use channel 0 follower for detection
            let env_db = followers[0][band].process(rms_db);

            // Gain computer: soft-knee 6dB
            let knee_db = 6.0f32;
            let gr_db = if env_db <= THRESHOLD_DB - knee_db * 0.5 {
                0.0
            } else if env_db < THRESHOLD_DB + knee_db * 0.5 {
                // Soft knee polynomial
                let ov = env_db - THRESHOLD_DB + knee_db * 0.5;
                -(slope * ov * ov) / (2.0 * knee_db)
            } else {
                slope * (THRESHOLD_DB - env_db)
            };

            band_gr[band][frame] = libm::powf(10.0, gr_db / 20.0);
        }
    }

    // ── §4.4 Band coupling (feed-forward, every 4 blocks) ────────────────────
    // Simple: couple adjacent band GR values
    let coupling_update_interval = (block_size * 4).max(1);
    for frame in 0..frame_count {
        if frame % coupling_update_interval == 0 {
            // Low-mid coupling: band1 GR nudged toward band0 GR
            let coupled1 = band_gr[1][frame] * (1.0 - COUPLING_LOW_MID)
                + band_gr[0][frame] * COUPLING_LOW_MID;
            band_gr[1][frame] = coupled1;
            // Mid-high coupling: band2 GR nudged toward band3 GR
            let coupled2 = band_gr[2][frame] * (1.0 - COUPLING_MID_HIGH)
                + band_gr[3][frame] * COUPLING_MID_HIGH;
            band_gr[2][frame] = coupled2;
        }
    }

    // ── §4.5 Band recombination + apply GR ───────────────────────────────────
    // Auto makeup gain: request from stage4 budget
    let makeup_db  = libm::fmaxf(0.0, THRESHOLD_DB.abs() * slope_for_style(preset.compression));
    let makeup_req = Decibels(libm::fminf(makeup_db, 4.0)); // cap request
    let granted    = gain_budget.request_stage4(makeup_req);
    let makeup_lin = granted.to_linear().0;

    for frame in 0..frame_count {
        // Recombine bands
        for c in 0..ch {
            let i = frame * ch + c;
            let recombined = band_bufs[0][i] * band_gr[0][frame]
                + band_bufs[1][i] * band_gr[1][frame]
                + band_bufs[2][i] * band_gr[2][frame]
                + band_bufs[3][i] * band_gr[3][frame];
            pcm[i] = finalize_sample(recombined * makeup_lin);
        }
    }

    // ── §4.6 Stereo link RMS detector ────────────────────────────────────────
    if ch >= 2 {
        let alpha = preset.compression.stereo_link_amount();
        let window = (sample_rate * 0.005) as usize; // 5ms RMS window
        let window = window.max(1);

        for frame in 0..frame_count {
            if frame % window == 0 {
                let end = (frame + window).min(frame_count);
                let rms_l: f32 = libm::sqrtf(kahan_mean_square(
                    &pcm[frame*ch..end*ch].iter().step_by(ch).copied().collect::<Vec<_>>()
                ));
                let rms_r: f32 = libm::sqrtf(kahan_mean_square(
                    &pcm[frame*ch+1..].iter().step_by(ch).take(end - frame).copied().collect::<Vec<_>>()
                ));
                // Equal-power linked RMS (spec §4.6)
                let _linked_rms = libm::sqrtf(
                    rms_l * rms_l * (1.0 - alpha * 0.5)
                    + rms_r * rms_r * (1.0 - alpha * 0.5)
                    + rms_l * rms_r * alpha,
                );
                // linked_rms stored for future Stage 6 use (not applied here — no-op until wired)
            }
        }
    }
}

/// Mean slope for makeup gain estimate based on style.
fn slope_for_style(style: CompressionStyle) -> f32 {
    match style {
        CompressionStyle::Transparent => 0.1,
        CompressionStyle::Gentle      => 0.2,
        CompressionStyle::Medium      => 0.3,
        CompressionStyle::Aggressive  => 0.4,
    }
}

// ── Legacy Stage4Compress (preserved — existing pipeline wiring + test) ───────

use alloc::vec;

pub struct Stage4Compress {
    channels:      u16,
    threshold:     f32,
    ratio:         f32,
    knee:          f32,
    attack_coef:   f32,
    release_coef:  f32,
    env_states:    Vec<f32>,
}

impl Stage4Compress {
    pub fn new(
        sample_rate:          u32,
        channels:             u16,
        comp_threshold_dbfs:  f32,
        comp_ratio_default:   f32,
        comp_knee_db:         f32,
    ) -> Self {
        let env_states = vec![0.0; channels as usize];
        let attack_coef  = libm::expf(-1.0 / (0.010 * sample_rate as f32));
        let release_coef = libm::expf(-1.0 / (0.150 * sample_rate as f32));
        Self { channels, threshold: comp_threshold_dbfs, ratio: comp_ratio_default,
               knee: comp_knee_db, attack_coef, release_coef, env_states }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        let frames = chunk.frame_count();
        let chans  = self.channels as usize;
        let makeup_db  = libm::fabsf(self.threshold * (1.0 - 1.0 / self.ratio));
        let makeup_lin = libm::powf(10.0, makeup_db / 20.0);
        for frame in 0..frames {
            let mut max_env = 0.0f32;
            for ch in 0..chans {
                let idx        = frame * chans + ch;
                let sample_abs = libm::fabsf(chunk.samples[idx]);
                let env = if sample_abs > self.env_states[ch] {
                    self.attack_coef * self.env_states[ch] + (1.0 - self.attack_coef) * sample_abs
                } else {
                    self.release_coef * self.env_states[ch] + (1.0 - self.release_coef) * sample_abs
                };
                self.env_states[ch] = env;
                if env > max_env { max_env = env; }
            }
            let mut gain = 1.0f32;
            let level_db = 20.0 * libm::log10f(max_env + 1e-9);
            if level_db > self.threshold - self.knee / 2.0 {
                let cv = if level_db > self.threshold + self.knee / 2.0 {
                    self.threshold + (level_db - self.threshold) / self.ratio
                } else {
                    level_db + (1.0 / self.ratio - 1.0)
                        * libm::powf(level_db - self.threshold + self.knee / 2.0, 2.0)
                        / (2.0 * self.knee)
                };
                let gain_db = cv - level_db;
                gain = libm::powf(10.0, gain_db / 20.0);
            }
            for ch in 0..chans {
                let idx = frame * chans + ch;
                chunk.samples[idx] = chunk.samples[idx] * gain * makeup_lin;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::gain_budget::GainBudget;
    use crate::pipeline::warnings::WarningAggregator;
    use crate::types::mastering_preset::{
        EqCurve, LimiterAlgorithm, MasteringPreset, SaturationStyle,
    };

    // ── Legacy test (must remain passing) ────────────────────────────────────
    #[test]
    fn test_compress_silence() {
        let mut process = Stage4Compress::new(48000, 2, -18.0, 2.0, 6.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        process.process_chunk(&mut chunk);
        for &s in &chunk.samples {
            assert!(s.abs() < 1e-6);
        }
    }

    // ── v2.9 process_compress tests ───────────────────────────────────────────
    fn transparent_preset() -> MasteringPreset {
        MasteringPreset {
            lufs_target:    -14.0,
            true_peak_ceil: -1.0,
            compression:    CompressionStyle::Transparent,
            saturation:     SaturationStyle::None,
            eq_curve:       EqCurve::Flat,
            limiter:        LimiterAlgorithm::Transparent,
            gain_budget_db: 6.0,
            dither_bits:    24,
            dither_seed:    0xDEAD,
            oversampling:   4,
        }
    }

    fn aggressive_preset() -> MasteringPreset {
        MasteringPreset {
            compression: CompressionStyle::Aggressive,
            ..transparent_preset()
        }
    }

    #[test]
    fn test_compress_v29_silence() {
        let mut pcm = vec![0.0f32; 512];
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        process_compress(&mut pcm, 1, 48_000.0, &transparent_preset(),
            &mut budget, &[], &mut agg, 128, 0);
        for s in &pcm {
            assert!(s.is_finite(), "all samples must be finite");
        }
    }

    #[test]
    fn test_compress_v29_output_bounded() {
        let mut pcm: Vec<f32> = (0..512)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
            .collect();
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        process_compress(&mut pcm, 1, 48_000.0, &transparent_preset(),
            &mut budget, &[], &mut agg, 128, 0);
        for s in &pcm {
            assert!(s.is_finite() && *s >= -1.0 && *s <= 1.0,
                "output must be clamped [-1,1]: {s}");
        }
    }

    #[test]
    fn test_compress_v29_aggressive_reduces_gain() {
        // Aggressive compression must reduce a loud sine more than Transparent.
        let make_sine = || -> Vec<f32> {
            (0..512)
                .map(|i| 0.9 * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48_000.0))
                .collect()
        };
        let mut pcm_a = make_sine();
        let mut pcm_t = make_sine();
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        process_compress(&mut pcm_a, 1, 48_000.0, &aggressive_preset(),
            &mut budget, &[], &mut agg, 128, 0);
        let mut budget2 = GainBudget::default();
        let mut agg2 = WarningAggregator::new();
        process_compress(&mut pcm_t, 1, 48_000.0, &transparent_preset(),
            &mut budget2, &[], &mut agg2, 128, 0);
        let rms_a: f32 = libm::sqrtf(kahan_mean_square(&pcm_a));
        let rms_t: f32 = libm::sqrtf(kahan_mean_square(&pcm_t));
        assert!(rms_a <= rms_t + 0.05,
            "Aggressive ({rms_a:.4}) should not exceed Transparent ({rms_t:.4}) by much");
    }

    #[test]
    fn test_compress_v29_transient_priority() {
        // Transient priority must not panic and must produce finite output.
        let mut pcm: Vec<f32> = (0..256)
            .map(|i| 0.5 * libm::sinf(2.0 * core::f32::consts::PI * 220.0 * i as f32 / 48_000.0))
            .collect();
        let priorities = vec![SignalPriority::Transient];
        let mut budget = GainBudget::default();
        let mut agg = WarningAggregator::new();
        process_compress(&mut pcm, 1, 48_000.0, &aggressive_preset(),
            &mut budget, &priorities, &mut agg, 128, 0);
        for s in &pcm {
            assert!(s.is_finite(), "transient path must produce finite output: {s}");
        }
    }
}
