//! sp314-dsp pipeline — MasteringPipeline + inline XorShiftRng
//! Authority: LineOS Constitution v2.0 §05 · Creator OS Constitution v2.6 §05
//!
//! Key design decisions:
//! - No rand_core dependency — XorShiftRng implemented inline
//! - No ports/ or slots/ — sm-core extension system not needed
//! - No AppError — errors are &'static str (no_std)
//! - Thresholds from PipelineConstants (bmr-128.schema.json) — never hardcoded
//! - master() returns GoldenBlob (audio)

pub mod gain_budget;
pub mod signal_priority;
pub mod stage_pool;
pub mod validate;
pub mod warnings;

// ── v2.9 / v2.9.1 new modules ────────────────────────────────────────────────
// Sandbox steps [3] and [4a] — not wired into master() yet.
pub mod math;             // §N1 — Kahan energy accumulation (v2.9.1)
pub mod input_profile;    // §Input Profile Detection (v2.9)
pub mod stage1_5a_analyzer; // §Stage 1.5a — Loudness Analyzer (v2.9)

pub mod stage1_analyze;
pub mod stage2_eq;
pub mod stage3_deess;
pub mod stage4_compress;
pub mod stage5_saturate;
pub mod stage6_stereo;
pub mod stage7_limit;
pub mod stage8_dither;

use alloc::vec::Vec;

use crate::analysis::AnalysisAccumulator;
use crate::types::audio::AudioChunk;
use crate::types::config::PipelineConstants;
use crate::types::golden_blob::{BlobType, GoldenBlob};
use crate::types::mastering_preset::MasteringPreset;
use crate::types::metrics::QualityMetrics;

use stage_pool::StagePool;
use validate::{validate_input, validate_preset, MAX_POOL_FRAMES};
use warnings::WarningAggregator;

use stage1_analyze::Stage1Analyze;
use stage2_eq::Stage2Eq;
use stage3_deess::Stage3DeEss;
use stage4_compress::Stage4Compress;
use stage5_saturate::Stage5Saturate;
use stage6_stereo::Stage6Stereo;
use stage7_limit::Stage7Limit;
use stage8_dither::{SimpleRng, Stage8Dither};

// ──────────────────────────────────────────────────────────────────────────────
// Inline RNG — no external dependency
// ML Origin Rule: pure algorithmic — no weights
// ──────────────────────────────────────────────────────────────────────────────

/// XorShift64 — inline deterministic RNG for TPDF dither.
/// Seed must be non-zero. Same seed → identical sequence, always.
/// No rand_core dependency permitted (LineOS Constitution v2.0 §09.2).
pub struct XorShiftRng {
    state: u64,
}

impl XorShiftRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEADBEEF } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

impl SimpleRng for XorShiftRng {
    fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// MasteringIntent
// ──────────────────────────────────────────────────────────────────────────────

/// Intent passed to MasteringPipeline::master().
/// seed is explicit — derived from caller, never from entropy.
pub struct MasteringIntent {
    /// Deterministic seed for dither RNG. Must be set from input data.
    pub seed:         u64,
    /// Target LUFS from bmr-128.schema.json presets (e.g. -14.0 for Spotify).
    /// None → use raw mode (no normalization).
    pub target_lufs:  Option<f32>,
    /// Export bit depth: true = 16-bit dither, false = 24-bit dither
    pub export_16bit: bool,
}

// ──────────────────────────────────────────────────────────────────────────────
// MasteringPipeline
// ──────────────────────────────────────────────────────────────────────────────

/// The 8-stage audio mastering pipeline.
/// Immutable between phase releases per LineOS Constitution §05.1.
pub struct MasteringPipeline {
    constants: PipelineConstants,
    pool:      StagePool,
}

impl MasteringPipeline {
    /// Create a pipeline with constants loaded from bmr-128.schema.json.
    pub fn new(constants: PipelineConstants) -> Self {
        Self {
            constants,
            pool: StagePool::new(),
        }
    }

    /// Run the full 8-stage pipeline over `chunks` and return a GoldenBlob.
    ///
    /// Two-pass algorithm:
    /// Pass 1: Analysis → normalization gain
    /// Pass 2: Process all chunks through stages 1–8
    ///
    /// Returns Err on anomaly detection (fatal per M0 Constitution §04.3).
    pub fn master(
        &mut self,
        intent:      &MasteringIntent,
        chunks:      &[AudioChunk],
        input_hash:  [u8; 32],
    ) -> Result<GoldenBlob, &'static str> {
        if chunks.is_empty() {
            return Err("MasteringPipeline: no audio chunks provided");
        }

        let preset = MasteringPreset::from_intent(
            intent.export_16bit,
            intent.seed,
            intent.target_lufs,
        );
        validate_preset(&preset)?;

        if chunks.len() == 1 {
            validate_input(&chunks[0].samples)?;
        } else {
            let total_samples: usize = chunks.iter().map(|c| c.samples.len()).sum();
            if total_samples > MAX_POOL_FRAMES * 2 {
                return Err("sp314-dsp: input exceeds maximum sample count");
            }
        }

        self.pool.reset();
        let warnings = WarningAggregator::new();

        let sample_rate = chunks[0].sample_rate;
        let channels    = chunks[0].channels;
        let c           = &self.constants;

        // ── Pass 1: Analysis ──────────────────────────────────────────────────
        let mut analysis_acc = AnalysisAccumulator::new(sample_rate, channels);
        for chunk in chunks {
            analysis_acc.feed(chunk);
        }
        let _pre_metrics = analysis_acc.finalize();

        let norm_gain = intent.target_lufs.map(|target| {
            analysis_acc.normalization_gain_linear(target)
        }).unwrap_or(1.0);

        // ── Pass 2: Processing ────────────────────────────────────────────────
        let stage1 = Stage1Analyze::new(norm_gain);

        let mut stage2 = Stage2Eq::new(
            sample_rate, channels,
            c.eq_hpf_freq_hz, c.eq_air_shelf_hz,
        );
        let mut stage3 = Stage3DeEss::new(
            sample_rate,
            channels,
            c.dess_band_low_hz,
            c.dess_band_high_hz,
        );
        let mut stage4 = Stage4Compress::new(
            sample_rate, channels,
            c.comp_threshold_dbfs, c.comp_ratio_default, c.comp_knee_db,
        );
        let mut stage5 = Stage5Saturate::new(c.sat_drive_default, channels);
        let mut stage6 = Stage6Stereo::new(
            sample_rate, channels,
            c.ms_side_gain_db, c.ms_side_hpf_hz,
        );
        let mut stage7 = Stage7Limit::new(
            sample_rate, channels,
            c.lookahead_max, -1.0,  // true_peak_ceiling: -1.0 dBFS (default platform preset)
        );

        // Dither — seed from intent (explicit, deterministic, never from entropy)
        let rng = XorShiftRng::new(intent.seed);
        let mut stage8 = Stage8Dither::new(
            intent.export_16bit, rng,
            c.dither_bits_24, c.dither_bits_16,
        );

        // Final analysis pass (post-processing metrics)
        let mut final_acc = AnalysisAccumulator::new(sample_rate, channels);

        // Collect output PCM (raw bytes for Phase 2 — FLAC encoding in Phase 3)
        let mut output_pcm: Vec<u8> = Vec::new();

        for chunk in chunks {
            let mut proc = chunk.clone();

            stage1.apply_gain_and_check(&mut proc)?;
            stage2.process_chunk(&mut proc);
            stage3.process_chunk(&mut proc);
            stage4.process_chunk(&mut proc);
            stage5.process_chunk(&mut proc);
            stage6.process_chunk(&mut proc);
            stage7.process_chunk(&mut proc);
            stage8.process_chunk(&mut proc);

            // Feed to final analysis
            final_acc.feed(&proc);

            // Collect as raw f32 LE bytes (Phase 2 stub — Phase 3 adds FLAC)
            for &s in &proc.samples {
                output_pcm.extend_from_slice(&s.to_le_bytes());
            }
        }

        let quality_metrics: QualityMetrics = final_acc.finalize();

        Ok(GoldenBlob {
            blob_type:       BlobType::Audio,
            flac_bytes:      output_pcm,
            quality_metrics,
            seed:            intent.seed,
            input_hash,
            warnings:        warnings.records().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::config::PipelineConstants;

    fn test_constants() -> PipelineConstants {
        PipelineConstants {
            lookahead_ms:        2.0,
            lookahead_max:       192,
            eq_hpf_freq_hz:      30.0,
            eq_air_shelf_hz:     12000.0,
            dess_band_low_hz:    6000.0,
            dess_band_high_hz:   8000.0,
            comp_threshold_dbfs: -18.0,
            comp_ratio_default:  2.0,
            comp_knee_db:        6.0,
            sat_drive_default:   1.3,
            ms_side_gain_db:     1.5,
            ms_side_hpf_hz:      120.0,
            smoothing_ramp_ms:   20.0,
            dither_bits_24:      0.00000011920928955078125,
            dither_bits_16:      0.000030517578125,
        }
    }

    #[test]
    fn test_xorshift_nonzero_seed() {
        let mut rng = XorShiftRng::new(42);
        let a = rng.next_u32();
        let b = rng.next_u32();
        assert_ne!(a, b, "XorShiftRng should produce different values");
    }

    #[test]
    fn test_xorshift_zero_seed_fallback() {
        let mut rng = XorShiftRng::new(0);
        // Should not stay zero (falls back to 0xDEADBEEF)
        let v = rng.next_u32();
        assert_ne!(v, 0);
    }

    #[test]
    fn test_xorshift_deterministic() {
        let mut rng1 = XorShiftRng::new(0x1337BEEF);
        let mut rng2 = XorShiftRng::new(0x1337BEEF);
        for _ in 0..100 {
            assert_eq!(rng1.next_u32(), rng2.next_u32());
        }
    }

    #[test]
    fn test_pipeline_silence() {
        let mut pipeline = MasteringPipeline::new(test_constants());
        let intent = MasteringIntent {
            seed:         0x1337BEEF,
            target_lufs:  Some(-14.0),
            export_16bit: true,
        };
        let chunk = AudioChunk {
            samples:     alloc::vec![0.0; 4096],
            sample_rate: 48000,
            channels:    2,
        };
        let result = pipeline.master(&intent, &[chunk], [0u8; 32]).unwrap();
        assert!(!result.flac_bytes.is_empty());
    }

    #[test]
    fn test_pipeline_empty_input() {
        let mut pipeline = MasteringPipeline::new(test_constants());
        let intent = MasteringIntent {
            seed: 42, target_lufs: None, export_16bit: false,
        };
        let result = pipeline.master(&intent, &[], [0u8; 32]);
        assert!(result.is_err());
    }
}
