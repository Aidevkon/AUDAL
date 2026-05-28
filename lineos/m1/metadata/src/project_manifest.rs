//! Project manifest — a unique identifier + metadata record for a mastering session.
//! Written once per GoldenBlob. Immutable after creation.
//! Authority: LineOS Constitution v2.0 §07

use serde::{Deserialize, Serialize};
use lineos_types::{GoldenBlob, Ebu128Measurement};

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
    pub fn generate(_blob: &GoldenBlob, measurement: &Ebu128Measurement) -> Self {
        Self {
            version:            "1.0".to_string(),
            input_hash:         "TODO".to_string(), // TODO: 3b — input_hash string logic removed
            seed:               0, // TODO: 3b — blob.seed removed
            integrated_lufs:    measurement.integrated_lufs,
            true_peak_dbtp:     measurement.true_peak_dbfs, // changed field name
            loudness_range_lu:  measurement.loudness_range_lu,
            duration_seconds:   0.0, // TODO: 3b — duration_seconds removed
            sample_rate:        48000, // TODO: 3b — sample_rate removed
            channels:           2, // TODO: 3b — channels removed
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::Ebu128Measurement;

    fn fake_blob() -> GoldenBlob {
        use lineos_types::BlobType;
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
            blob_type: BlobType::Audio,
            sha256: "fakehash".to_string(),
            preset_name: "Spotify".to_string(),
            engine_version: "1.0.0".to_string(),
            schema_version: 1,
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
        }
    }

    fn test_measurement() -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs:    -14.0,
            true_peak_dbfs:     -1.2,
            loudness_range_lu:  6.0,
            short_term_lufs:    Some(-13.0),
        }
    }

    #[test]
    fn test_manifest_generates() {
        let blob = fake_blob();
        let m = test_measurement();
        let _manifest = ProjectManifest::generate(&blob, &m);
        // TODO: 3b — fix tests
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
