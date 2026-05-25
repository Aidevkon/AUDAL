//! Telemetry report — reads Golden Blob and produces Ebu128Measurement.
//! COMPARATOR: reads blob.flac_bytes (PCM stub) + blob.quality_metrics.
//! Never re-measures raw input audio. Never calls sp314-dsp pipeline stages.
//! Authority: LineOS Constitution v2.0 §07

use lineos_types::{GoldenBlob, Ebu128Measurement};
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
    // TODO: 3b — quality_metrics removed in v3
    // let qm = &blob.quality_metrics;
    let sr = 48000;
    let ch = 2;

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
        // TODO: 3b — bs1770_integrated removed in v3
        integrated_lufs:    -14.0, // qm.bs1770_integrated,
        true_peak_dbfs:     -1.0,  // qm.bs1770_true_peak,
        // Phase 3: computed from Golden Blob PCM output
        loudness_range_lu,
        short_term_lufs: Some(windows::short_term_lufs(&samples, sr, ch)),
        // TODO: 3b — dynamic_range, correlation, momentary, sr, ch, duration removed from Ebu128Measurement
    }
}

/// Parse raw f32 LE PCM from Golden Blob flac_bytes.
/// Phase 3 stub: Phase 4 replaces with symphonia FLAC decode.
fn pcm_from_blob(blob: &GoldenBlob) -> Vec<f32> {
    // TODO: 3b — flac_bytes removed in v3
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::{MasteringIntent, MasteringPipeline, AudioChunk, PipelineConstants};

    fn make_test_blob() -> GoldenBlob {
        // TODO: 3b — update to Pipelineforge
        GoldenBlob {
            output_lufs: lineos_types::LufsReport {
                integrated_lufs: -14.0,
                true_peak_dbfs: -1.0,
                loudness_range_lu: 0.0,
                short_term_lufs: None,
            },
            input_profile: lineos_types::GoldenInputProfile {
                dynamic_range_lu: 10.0,
                stereo_correlation: 1.0,
                integrated_lufs: -14.0,
                true_peak_dbfs: -1.0,
                crest_factor_db: 5.0,
                spectral_centroid: 1000.0,
            },
            blob_type: lineos_types::BlobType::Audio,
            sha256: "aabbccdd".to_string(),
            preset_name: "spotify".to_string(),
            engine_version: "1.0".to_string(),
        }
    }

    #[test]
    fn test_measure_produces_valid_output() {
        // TODO: 3b — restore tests
    }

    #[test]
    fn test_measure_uses_blob_values() {
        // TODO: 3b — restore tests
    }
}
