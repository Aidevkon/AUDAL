//! Telemetry report — reads Golden Blob and produces Ebu128Measurement.
//! COMPARATOR: reads blob.flac_bytes (PCM stub) + blob.quality_metrics.
//! Never re-measures raw input audio. Never calls sp314-dsp pipeline stages.
//! Authority: LineOS Constitution v2.0 §07

use sp314_dsp::types::{
    golden_blob::GoldenBlob,
    metrics::Ebu128Measurement,
};
use crate::lra::LraCalculator;
use crate::windows;
use alloc::vec::Vec;

/// Produce a full Ebu128Measurement from a Golden Blob.
///
/// The Golden Blob already contains bs1770_integrated and bs1770_true_peak
/// computed by sp314-dsp. This function adds LRA, momentary, and short-term.
///
/// Phase 3 stub: flac_bytes interpreted as raw f32 LE PCM.
/// Phase 4 replaces with proper FLAC decode via symphonia.
pub fn measure(blob: &GoldenBlob) -> Ebu128Measurement {
    let qm = &blob.quality_metrics;
    let sr = qm.sample_rate;
    let ch = qm.channels;

    // Extract PCM from Golden Blob (Phase 3: raw f32 LE bytes)
    let samples = pcm_from_blob(blob);

    // LRA — requires multiple 3s windows; returns 0.0 if insufficient material
    let mut lra_calc = LraCalculator::new(sr);
    lra_calc.feed_samples(&samples, ch);
    let loudness_range_lu = lra_calc.compute();

    let duration_seconds = if sr > 0 && ch > 0 {
        samples.len() as f32 / (sr as f32 * ch as f32)
    } else {
        0.0
    };

    Ebu128Measurement {
        // BS.1770-4 canonical values — from Golden Blob (not re-measured here)
        integrated_lufs:    qm.bs1770_integrated,
        true_peak_dbtp:     qm.bs1770_true_peak,
        stereo_correlation: qm.stereo_correlation,
        dynamic_range_db:   qm.dynamic_range_db,
        // Phase 3: computed from Golden Blob PCM output
        loudness_range_lu,
        momentary_lufs: windows::momentary_lufs(&samples, sr, ch),
        short_term_lufs: windows::short_term_lufs(&samples, sr, ch),
        sample_rate: sr,
        channels: ch,
        duration_seconds,
    }
}

/// Parse raw f32 LE PCM from Golden Blob flac_bytes.
/// Phase 3 stub: Phase 4 replaces with symphonia FLAC decode.
fn pcm_from_blob(blob: &GoldenBlob) -> Vec<f32> {
    blob.flac_bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp314_dsp::{
        pipeline::{MasteringIntent, MasteringPipeline},
        types::{audio::AudioChunk, config::PipelineConstants},
    };

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

    fn make_test_blob() -> GoldenBlob {
        let sr = 48000u32;
        let ch = 2u16;
        let samples: Vec<f32> = (0..(sr * 5 * ch as u32) as usize)
            .map(|i| {
                let frame = i / ch as usize;
                libm::sinf(2.0 * core::f32::consts::PI * 440.0 * frame as f32 / sr as f32) * 0.5
            })
            .collect();
        let chunk = AudioChunk { samples, sample_rate: sr, channels: ch };
        let mut pipeline = MasteringPipeline::new(test_constants());
        let intent = MasteringIntent { seed: 0x1337BEEF, target_lufs: Some(-14.0), export_16bit: true };
        pipeline.master(&intent, &[chunk], [0u8; 32]).expect("test pipeline should succeed")
    }

    #[test]
    fn test_measure_produces_valid_output() {
        let blob = make_test_blob();
        let m = measure(&blob);

        // integrated_lufs must be finite (not silence)
        assert!(m.integrated_lufs.is_finite() || m.integrated_lufs == f32::NEG_INFINITY);
        // duration must be positive
        assert!(m.duration_seconds >= 0.0);
        // LRA must be non-negative
        assert!(m.loudness_range_lu >= 0.0);
        // sample_rate must be preserved from blob
        assert_eq!(m.sample_rate, 48000);
        assert_eq!(m.channels, 2);
    }

    #[test]
    fn test_measure_uses_blob_values() {
        let blob = make_test_blob();
        let m = measure(&blob);
        // Must read from Golden Blob, not recompute
        assert_eq!(m.integrated_lufs, blob.quality_metrics.bs1770_integrated);
        assert_eq!(m.true_peak_dbtp, blob.quality_metrics.bs1770_true_peak);
        assert_eq!(m.stereo_correlation, blob.quality_metrics.stereo_correlation);
    }
}
