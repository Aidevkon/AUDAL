//! Project manifest — a unique identifier + metadata record for a mastering session.
//! Written once per GoldenBlob. Immutable after creation.
//! Authority: LineOS Constitution v2.0 §07

use serde::{Deserialize, Serialize};
use sp314_dsp::types::{golden_blob::GoldenBlob, metrics::Ebu128Measurement};

/// Project manifest — produced once per mastering session.
/// Contains the session's input hash (from GoldenBlob) + measurement summary.
/// Downstream M1.6 sync uses this for cloud export — never syncs raw audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub version:       String,
    /// SHA-256 of input audio — from GoldenBlob (for audit trail)
    pub input_hash:    String,
    /// Deterministic seed used for dither — for reproducibility audit
    pub seed:          u64,
    pub integrated_lufs:    f32,
    pub true_peak_dbtp:     f32,
    pub loudness_range_lu:  f32,
    pub duration_seconds:   f32,
    pub sample_rate:        u32,
    pub channels:           u16,
}

impl ProjectManifest {
    pub fn generate(blob: &GoldenBlob, measurement: &Ebu128Measurement) -> Self {
        Self {
            version:            "1.0".to_string(),
            input_hash:         hex_encode(&blob.input_hash),
            seed:               blob.seed,
            integrated_lufs:    measurement.integrated_lufs,
            true_peak_dbtp:     measurement.true_peak_dbtp,
            loudness_range_lu:  measurement.loudness_range_lu,
            duration_seconds:   measurement.duration_seconds,
            sample_rate:        measurement.sample_rate,
            channels:           measurement.channels,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// Encode a byte slice as a lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp314_dsp::types::metrics::Ebu128Measurement;

    fn fake_blob() -> GoldenBlob {
        use sp314_dsp::types::{golden_blob::BlobType, metrics::QualityMetrics};
        GoldenBlob {
            blob_type:       BlobType::Audio,
            flac_bytes:      vec![],
            quality_metrics: QualityMetrics::default(),
            seed:            0x1337BEEF,
            input_hash:      [0xABu8; 32],
        }
    }

    fn test_measurement() -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs:    -14.0,
            true_peak_dbtp:     -1.2,
            loudness_range_lu:  6.0,
            momentary_lufs:     -12.0,
            short_term_lufs:    -13.0,
            stereo_correlation: 0.95,
            dynamic_range_db:   12.0,
            sample_rate:        48000,
            channels:           2,
            duration_seconds:   5.0,
        }
    }

    #[test]
    fn test_manifest_generates() {
        let blob = fake_blob();
        let m = test_measurement();
        let manifest = ProjectManifest::generate(&blob, &m);
        assert_eq!(manifest.seed, 0x1337BEEF);
        assert_eq!(manifest.sample_rate, 48000);
        // input_hash should be 64 hex chars (32 bytes × 2)
        assert_eq!(manifest.input_hash.len(), 64);
    }

    #[test]
    fn test_manifest_json() {
        let blob = fake_blob();
        let m = test_measurement();
        let manifest = ProjectManifest::generate(&blob, &m);
        let json = manifest.to_json().unwrap();
        assert!(json.contains("input_hash"));
        assert!(json.contains("seed"));
    }
}
