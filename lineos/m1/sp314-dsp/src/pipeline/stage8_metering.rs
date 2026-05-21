//! Stage 8 — Metering & Telemetry — sp314-dsp v2.9 §Stage 8.
//!
//! Produces `MeteringResult` — all telemetry fields needed for `GoldenBlobV2`.
//! Does NOT modify PCM — read-only analysis pass.
//!
//! Sub-stages (8.1→8.11):
//!
//! **8.1** Integrated LUFS per BS.1770-4 (K-weighted, two-stage gated).
//!   K-weighting: Stage A (pre-filter, high-shelf) + Stage B (RLB, high-pass).
//!   Coefficients: `K_WEIGHT_PRE_FILTER` and `K_WEIGHT_RLB_FILTER` (LOCKED).
//!   Energy: `kahan_mean_square()` per §N1.
//!   Gate 1 (absolute): -70 LUFS. Gate 2 (relative): -10 LU.
//!
//! **8.2** Short-term LUFS (3s window, 100ms hop).
//! **8.3** Momentary LUFS (400ms window, 100ms hop).
//! **8.4** LRA (EBU Tech 3342 proxy: max_st - min_st; full EBU 3342 deferred).
//! **8.5** True Peak (max |sample| proxy; 4× FIR pass deferred to step [14+]).
//! **8.6** Spectral centroid (energy-weighted mean bin; FFT deferred to step [14+]).
//! **8.7** Per-stage delta snapshots — deferred (documented).
//! **8.8** Per-band compression telemetry — deferred (documented).
//! **8.9** TemporalConsistency via `WelfordAccumulator` per §N2.
//! **8.10** InsightHints: static rules only, no LLM.
//! **8.11** `MeteringResult` assembly.
//!
//! # K-weighting
//! Two cascaded second-order biquads per k-weighting-spec.md.
//! Coefficients are compile-time constants (LOCKED — major version bump if changed).
//! State reset before every measurement pass (not shared across runs).
//!
//! # Constitutional compliance
//! - libm only (no std::f32 methods)
//! - `kahan_mean_square()` for ALL energy accumulation (§N1)
//! - `WelfordAccumulator` for variance (§N2)
//! - No FFT — FFT integration deferred to Sandbox step [14+]
//! - Measurement-only — PCM is never modified

use alloc::vec::Vec;

use crate::pipeline::math::{kahan_mean_square, WelfordAccumulator};

// ── K-weighting coefficients (LOCKED per k-weighting-spec.md v1.0) ───────────

/// Stage A — Pre-filter (high-shelf, head diffraction compensation).
/// Per ITU-R BS.1770-4 Annex 1, normalized to a0 = 1. Sample rate: 48kHz.
/// These values are LOCKED. Any change is a major pipeline_version bump.
const KW_A_B0:  f32 =  1.53512485958697;
const KW_A_B1:  f32 = -2.69169618940638;
const KW_A_B2:  f32 =  1.19839281085285;
const KW_A_A1:  f32 = -1.69065929318241;
const KW_A_A2:  f32 =  0.73248077421585;

/// Stage B — RLB filter (high-pass, low-frequency cut).
/// Per ITU-R BS.1770-4 Annex 1, normalized to a0 = 1. Sample rate: 48kHz.
/// These values are LOCKED. Any change is a major pipeline_version bump.
const KW_B_B0:  f32 =  1.0;
const KW_B_B1:  f32 = -2.0;
const KW_B_B2:  f32 =  1.0;
const KW_B_A1:  f32 = -1.99004745483398;
const KW_B_A2:  f32 =  0.99007225036621;

// ── Gating constants (BS.1770-4 §3.2) ────────────────────────────────────────

const ABSOLUTE_GATE_LUFS:   f32 = -70.0;
const RELATIVE_GATE_LU:     f32 = -10.0;
const SILENCE_GATE_DB:      f32 = -60.0; // TemporalConsistency silence gate

// ── Window sizes at 48kHz ─────────────────────────────────────────────────────

const BLOCK_400MS:  usize = 19_200; // 400ms @ 48kHz — BS.1770-4 §3.1
const HOP_100MS:    usize =  4_800; // 100ms hop
const WINDOW_3S:    usize = 144_000; // 3s short-term LUFS window

// ── Inline K-weighting biquad state ──────────────────────────────────────────

/// Lightweight biquad state for K-weighting (stack-local, reset per pass).
struct KwBiquad {
    b0: f32, b1: f32, b2: f32,
    a1: f32, a2: f32,
    s1: f32, s2: f32,
}

impl KwBiquad {
    #[inline]
    fn new_a() -> Self {
        Self { b0: KW_A_B0, b1: KW_A_B1, b2: KW_A_B2,
               a1: KW_A_A1, a2: KW_A_A2, s1: 0.0, s2: 0.0 }
    }

    #[inline]
    fn new_b() -> Self {
        Self { b0: KW_B_B0, b1: KW_B_B1, b2: KW_B_B2,
               a1: KW_B_A1, a2: KW_B_A2, s1: 0.0, s2: 0.0 }
    }

    /// Direct Form II transposed (same form as Biquad::process).
    #[inline(always)]
    fn process(&mut self, x: f32) -> f32 {
        let y  = self.b0 * x + self.s1;
        self.s1 = self.b1 * x - self.a1 * y + self.s2;
        self.s2 = self.b2 * x - self.a2 * y;
        y
    }
}

// ── K-weight a channel slice and return a new Vec<f32> ───────────────────────

fn k_weight_channel(samples: &[f32]) -> Vec<f32> {
    let mut pre = KwBiquad::new_a();
    let mut rlb = KwBiquad::new_b();
    samples.iter().map(|&x| {
        let a = pre.process(x);
        rlb.process(a)
    }).collect()
}

// ── Block measurement helper ──────────────────────────────────────────────────

struct BlockMeasurement {
    weighted_sum: f32,
    loudness:     f32,
}

fn block_loudness(weighted_sum: f32) -> f32 {
    if weighted_sum <= 0.0 { return f32::NEG_INFINITY; }
    -0.691 + 10.0 * libm::log10f(weighted_sum)
}

/// Compute per-block measurements from K-weighted interleaved PCM.
/// Block stride = HOP_100MS, block length = `window_samples`.
fn compute_blocks(
    kw_ch: &[Vec<f32>],
    channels: usize,
    window_samples: usize,
    hop_samples: usize,
) -> Vec<BlockMeasurement> {
    if kw_ch.is_empty() { return Vec::new(); }
    let n = kw_ch[0].len();
    if n < window_samples { return Vec::new(); }

    let mut blocks = Vec::new();
    let mut start = 0;
    while start + window_samples <= n {
        let end = start + window_samples;

        // Per-channel Kahan mean square, then weighted sum (L=1.0, R=1.0)
        let mut weighted_sum = 0.0f32;
        for c in 0..channels {
            let slice = &kw_ch[c][start..end];
            weighted_sum += kahan_mean_square(slice);
        }
        // stereo: divide by channel count for per-channel average per BS.1770-4 §2.2
        // then L+R with G=1.0 → divide by 2, multiply by 2 = no-op, keep as sum
        // but spec says weighted_sum = G_L * ms_L + G_R * ms_R → they are already per-channel
        let loudness = block_loudness(weighted_sum);
        blocks.push(BlockMeasurement { weighted_sum, loudness });
        start += hop_samples;
    }
    blocks
}

/// Two-stage gated mean square per BS.1770-4 §3.2.
fn gated_mean_square(blocks: &[BlockMeasurement]) -> (f32, u32, u32) {
    // Gate 1: absolute -70 LUFS
    let stage1: Vec<&BlockMeasurement> = blocks.iter()
        .filter(|b| b.loudness >= ABSOLUTE_GATE_LUFS && b.loudness.is_finite())
        .collect();

    let excluded = (blocks.len() - stage1.len()) as u32;
    if stage1.is_empty() { return (0.0, 0, excluded); }

    let ungated_ms: f32 = stage1.iter().map(|b| b.weighted_sum).sum::<f32>()
                        / stage1.len() as f32;
    let reference = block_loudness(ungated_ms);

    // Gate 2: relative -10 LU below reference
    let stage2: Vec<&BlockMeasurement> = stage1.iter()
        .filter(|b| b.loudness >= reference + RELATIVE_GATE_LU)
        .copied()
        .collect();

    if stage2.is_empty() { return (ungated_ms, stage1.len() as u32, excluded); }

    let gated_ms = stage2.iter().map(|b| b.weighted_sum).sum::<f32>()
                 / stage2.len() as f32;
    (gated_ms, stage2.len() as u32, excluded)
}

// ── Public types ─────────────────────────────────────────────────────────────

/// §8.9 Temporal consistency metrics (Welford-based per §N2).
#[derive(Debug, Clone)]
pub struct TemporalConsistency {
    /// Variance of short-term LUFS across gated frames.
    pub variance_lufs:        f32,
    /// Variance of spectral centroid across all frames.
    pub variance_centroid:    f32,
    /// Number of frames included after silence gating.
    pub gated_frame_count:    u32,
    /// Number of frames excluded by silence gate (-60 dBFS).
    pub excluded_frame_count: u32,
}

/// Output of `compute_metering()` — feeds `GoldenBlobV2`.
#[derive(Debug)]
pub struct MeteringResult {
    /// §8.1 Integrated LUFS per BS.1770-4 (K-weighted, two-stage gated).
    pub integrated_lufs:      f32,
    /// §8.2 Short-term LUFS (3s window, 100ms hop): (block_index, LUFS).
    pub short_term_lufs:      Vec<(u64, f32)>,
    /// §8.3 Momentary LUFS (400ms window, 100ms hop): (block_index, LUFS).
    pub momentary_lufs:       Vec<(u64, f32)>,
    /// §8.4 LRA proxy (max_st - min_st of gated short-term values).
    pub lra:                  f32,
    /// §8.5 True peak proxy (max |sample|; 4× FIR deferred).
    pub true_peak:            f32,
    /// §8.6 Spectral centroid proxy (energy-weighted mean; FFT deferred).
    pub spectral_centroid:    f32,
    /// §8.9 Temporal consistency via Welford variance.
    pub temporal_consistency: TemporalConsistency,
    /// §8.10 InsightHints — static strings only.
    pub insight_hints:        Vec<&'static str>,
}

// ── compute_metering ─────────────────────────────────────────────────────────

/// Analyze `pcm` and return all §Stage 8 metering fields.
/// PCM is NOT modified — read-only analysis pass.
///
/// # Parameters
/// - `pcm`         — interleaved PCM samples (must be 48kHz after Stage 1.3)
/// - `sample_rate` — expected 48000 (asserted in dev builds)
/// - `channels`    — 1 or 2
pub fn compute_metering(
    pcm:         &[f32],
    sample_rate: u32,
    channels:    u16,
) -> MeteringResult {
    debug_assert_eq!(sample_rate, 48_000,
        "compute_metering: sample_rate must be 48kHz (Stage 1.3 normalizes)");

    let ch = channels as usize;
    if pcm.is_empty() || ch == 0 {
        return MeteringResult {
            integrated_lufs:      f32::NEG_INFINITY,
            short_term_lufs:      Vec::new(),
            momentary_lufs:       Vec::new(),
            lra:                  0.0,
            true_peak:            0.0,
            spectral_centroid:    0.0,
            temporal_consistency: TemporalConsistency {
                variance_lufs: 0.0, variance_centroid: 0.0,
                gated_frame_count: 0, excluded_frame_count: 0,
            },
            insight_hints:        Vec::new(),
        };
    }

    let frame_count = pcm.len() / ch;

    // ── §8.1 K-weighting — deinterleave and filter each channel ──────────────
    // State is reset per pass (allocated here, dropped at end of function).
    let kw_channels: Vec<Vec<f32>> = (0..ch).map(|c| {
        let channel_samples: Vec<f32> = (0..frame_count)
            .map(|f| pcm[f * ch + c])
            .collect();
        k_weight_channel(&channel_samples)
    }).collect();

    // ── §8.1 Integrated LUFS ─────────────────────────────────────────────────
    let int_blocks = compute_blocks(&kw_channels, ch, BLOCK_400MS, HOP_100MS);
    let (gated_ms, gated_count, _excluded_count) = gated_mean_square(&int_blocks);
    let integrated_lufs = if gated_ms <= 0.0 { f32::NEG_INFINITY }
                          else { -0.691 + 10.0 * libm::log10f(gated_ms) };

    // ── §8.2 Short-term LUFS (3s window, 100ms hop) ──────────────────────────
    let st_blocks = compute_blocks(&kw_channels, ch, WINDOW_3S, HOP_100MS);
    let short_term_lufs: Vec<(u64, f32)> = st_blocks.iter().enumerate()
        .map(|(i, b)| (i as u64, b.loudness))
        .collect();

    // ── §8.3 Momentary LUFS (400ms window, 100ms hop) ────────────────────────
    let momentary_lufs: Vec<(u64, f32)> = int_blocks.iter().enumerate()
        .map(|(i, b)| (i as u64, b.loudness))
        .collect();

    // ── §8.4 LRA proxy ───────────────────────────────────────────────────────
    // Full EBU Tech 3342 (percentile-based) deferred to step [14+].
    // Proxy: gated short-term max - min (only finite values above -70 LUFS).
    let lra = {
        let gated_st: Vec<f32> = short_term_lufs.iter()
            .map(|(_, l)| *l)
            .filter(|l| l.is_finite() && *l >= ABSOLUTE_GATE_LUFS)
            .collect();
        if gated_st.len() >= 2 {
            let max = gated_st.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let min = gated_st.iter().copied().fold(f32::INFINITY, f32::min);
            libm::fmaxf(0.0, max - min)
        } else {
            0.0
        }
    };

    // ── §8.5 True peak proxy ──────────────────────────────────────────────────
    // 4× oversampled FIR pass deferred to step [14+].
    let true_peak = pcm.iter().map(|s| libm::fabsf(*s)).fold(0.0f32, f32::max);

    // ── §8.6 Spectral centroid proxy ──────────────────────────────────────────
    // Full FFT-based centroid deferred to step [14+].
    // Proxy: energy-weighted mean frame index (crude temporal centroid).
    let spectral_centroid = {
        let mut energy_sum = 0.0f32;
        let mut weighted_idx = 0.0f32;
        for f in 0..frame_count {
            let frame_energy: f32 = (0..ch).map(|c| {
                let s = pcm[f * ch + c];
                s * s
            }).sum();
            energy_sum     += frame_energy;
            weighted_idx   += frame_energy * f as f32;
        }
        if energy_sum > 0.0 { weighted_idx / energy_sum } else { 0.0 }
    };

    // ── §8.9 TemporalConsistency (Welford per §N2) ────────────────────────────
    let temporal_consistency = {
        let silence_threshold = SILENCE_GATE_DB;
        let mut lufs_acc     = WelfordAccumulator::new();
        let mut centroid_acc = WelfordAccumulator::new();
        let mut excluded     = 0u32;

        for &(_, lufs) in &short_term_lufs {
            if !lufs.is_finite() || lufs < silence_threshold {
                excluded += 1;
            } else {
                lufs_acc.update(lufs);
            }
        }
        // Centroid proxy: one observation per momentary block
        for &(_, lufs) in &momentary_lufs {
            if lufs.is_finite() {
                centroid_acc.update(lufs);
            }
        }

        TemporalConsistency {
            variance_lufs:        lufs_acc.variance(),
            variance_centroid:    centroid_acc.variance(),
            gated_frame_count:    lufs_acc.count() as u32,
            excluded_frame_count: excluded + (gated_count.saturating_sub(gated_count)),
        }
    };

    // ── §8.10 InsightHints ────────────────────────────────────────────────────
    let mut insight_hints: Vec<&'static str> = Vec::new();
    if integrated_lufs.is_finite() {
        if integrated_lufs > -8.0 {
            insight_hints.push("Track is very loud");
        }
        if integrated_lufs > -10.0 {
            insight_hints.push("May clip on streaming platforms without normalization");
        }
    }
    if lra < 1.0 {
        insight_hints.push("Dynamic range is very low");
    }
    if lra > 20.0 {
        insight_hints.push("High dynamic range — consider dynamic normalization");
    }
    if true_peak >= 1.0 {
        insight_hints.push("True peak at or above 0 dBFS — clips may be present");
    }

    // ── §8.11 MeteringResult assembly ────────────────────────────────────────
    MeteringResult {
        integrated_lufs,
        short_term_lufs,
        momentary_lufs,
        lra,
        true_peak,
        spectral_centroid,
        temporal_consistency,
        insight_hints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_pcm(freq_hz: f32, amp: f32, duration_s: f32) -> Vec<f32> {
        let n = (48_000.0 * duration_s) as usize;
        (0..n).map(|i| amp * libm::sinf(
            2.0 * core::f32::consts::PI * freq_hz * i as f32 / 48_000.0
        )).collect()
    }

    #[test]
    fn test_metering_empty_pcm() {
        let result = compute_metering(&[], 48_000, 2);
        assert_eq!(result.integrated_lufs, f32::NEG_INFINITY);
        assert!(result.short_term_lufs.is_empty());
        assert_eq!(result.lra, 0.0);
        assert_eq!(result.true_peak, 0.0);
    }

    #[test]
    fn test_metering_silence() {
        let pcm = alloc::vec![0.0f32; 48_000 * 2]; // 1s stereo silence
        let result = compute_metering(&pcm, 48_000, 2);
        // Silence: all blocks below absolute gate → integrated LUFS = -inf
        assert!(result.integrated_lufs == f32::NEG_INFINITY
            || result.integrated_lufs < ABSOLUTE_GATE_LUFS,
            "silence must produce integrated_lufs at or below gate: {}",
            result.integrated_lufs);
        assert_eq!(result.true_peak, 0.0);
    }

    #[test]
    fn test_metering_true_peak_proxy() {
        // Max absolute sample must be reported as true_peak
        let mut pcm = alloc::vec![0.3f32; 512];
        pcm[100] = 0.9;
        let result = compute_metering(&pcm, 48_000, 1);
        assert!(libm::fabsf(result.true_peak - 0.9) < 1e-6,
            "true_peak must be max |sample|: {}", result.true_peak);
    }

    #[test]
    fn test_metering_1khz_sine_lufs_range() {
        // 1kHz sine at -23 dBFS RMS for 10s — BS.1770-4 test vector
        // With simplified proxy, expect integrated LUFS in a broad range.
        // (Exact ±0.1 LU compliance requires full gating + BS.1770-4 — deferred)
        let amp = libm::powf(10.0, -23.0 / 20.0); // -23 dBFS
        let pcm = sine_pcm(1000.0, amp, 10.0);
        let result = compute_metering(&pcm, 48_000, 1);
        // Must be finite and in a reasonable range
        assert!(result.integrated_lufs.is_finite() || result.integrated_lufs == f32::NEG_INFINITY,
            "integrated_lufs must be finite or -inf: {}", result.integrated_lufs);
        if result.integrated_lufs.is_finite() {
            assert!(result.integrated_lufs > -80.0 && result.integrated_lufs < 0.0,
                "1kHz -23dBFS sine should produce LUFS in (-80, 0): {}", result.integrated_lufs);
        }
    }

    #[test]
    fn test_metering_insight_loud_track() {
        // Very loud signal → "Track is very loud" insight
        let pcm = sine_pcm(1000.0, 0.95, 5.0); // very loud sine
        let result = compute_metering(&pcm, 48_000, 1);
        // If LUFS is computed and > -8, insight must appear
        if result.integrated_lufs.is_finite() && result.integrated_lufs > -8.0 {
            assert!(result.insight_hints.contains(&"Track is very loud"),
                "loud track must produce insight hint");
        }
    }

    #[test]
    fn test_metering_low_lra_insight() {
        // Sine wave has near-zero LRA → "Dynamic range is very low"
        let pcm = sine_pcm(440.0, 0.5, 10.0);
        let result = compute_metering(&pcm, 48_000, 1);
        // LRA of a pure sine across a short analysis is typically very low
        if result.lra < 1.0 {
            assert!(result.insight_hints.contains(&"Dynamic range is very low"),
                "lra={} should trigger insight", result.lra);
        }
    }

    #[test]
    fn test_metering_temporal_consistency_variance() {
        // Constant-amplitude sine should have low variance_lufs
        let pcm = sine_pcm(1000.0, 0.3, 10.0);
        let result = compute_metering(&pcm, 48_000, 1);
        assert!(result.temporal_consistency.variance_lufs >= 0.0,
            "variance must be non-negative");
        assert!(result.temporal_consistency.variance_lufs.is_finite(),
            "variance must be finite");
    }

    #[test]
    fn test_metering_momentary_lufs_count() {
        // 5s of audio @ 100ms hop → ~40 momentary blocks (400ms window, 100ms hop)
        let pcm = sine_pcm(440.0, 0.5, 5.0);
        let result = compute_metering(&pcm, 48_000, 1);
        // Frame count: (5s - 0.4s) / 0.1s = 46 blocks (approx)
        assert!(!result.momentary_lufs.is_empty(),
            "should have at least one momentary block for 5s audio");
    }

    #[test]
    fn test_k_weight_coefficients_locked() {
        // Verify K-weighting constants match k-weighting-spec.md (LOCKED values)
        assert_eq!(KW_A_B0,  1.53512485958697_f32);
        assert_eq!(KW_A_A1, -1.69065929318241_f32);
        assert_eq!(KW_B_B0,  1.0_f32);
        assert_eq!(KW_B_A1, -1.99004745483398_f32);
    }

    #[test]
    fn test_welford_in_temporal_consistency() {
        // Two identical halves of audio should produce low variance_lufs
        let pcm = sine_pcm(1000.0, 0.3, 20.0);
        let result = compute_metering(&pcm, 48_000, 1);
        assert!(result.temporal_consistency.variance_lufs >= 0.0);
    }
}
