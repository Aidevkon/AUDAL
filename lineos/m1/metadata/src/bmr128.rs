//! BMR-128 Report — structured compliance report for a platform preset.
//! All thresholds from bmr-128.schema.json via PresetThresholds — never hardcoded.
//! Authority: LineOS Constitution v2.0 §07 · LineOS §12 "BMR-128 thresholds hardcoded — build failure"

use lineos_types::Ebu128Measurement;
use serde::{Deserialize, Serialize};

/// Full BMR-128 compliance report for a specific platform preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bmr128Report {
    pub version: String,
    pub preset: String,
    /// Target LUFS from bmr-128.schema.json (None for raw preset)
    pub target_lufs: Option<f32>,
    /// True peak ceiling from bmr-128.schema.json
    pub true_peak_ceiling: f32,
    pub measured: MeasuredValues,
    pub compliance: ComplianceResult,
}

/// Values measured from the Golden Blob (via Ebu128Measurement).
/// Never derived from raw audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasuredValues {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub loudness_range_lu: f32,
    pub stereo_correlation: f32,
    pub dynamic_range_db: f32,
    pub momentary_lufs: f32,
    pub short_term_lufs: f32,
    pub bpm: f32,
    pub transient_density: f32,
}

/// Compliance evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceResult {
    pub passes: bool,
    /// integrated_lufs − target_lufs (None for raw preset).
    /// Negative = too quiet; positive = too loud.
    pub lufs_delta: Option<f32>,
    /// true_peak_ceiling − true_peak_dbtp (positive = headroom available)
    pub peak_headroom: f32,
    pub violations: Vec<String>,
}

impl Bmr128Report {
    /// Generate a BMR-128 report.
    ///
    /// `target_lufs` and `true_peak_ceiling` MUST come from bmr-128.schema.json
    /// PresetThresholds — never hardcode these values.
    pub fn generate(
        measurement: &Ebu128Measurement,
        pre_analysis: &lineos_types::PreAnalysisData,
        preset: &str,
        target_lufs: Option<f32>,
        true_peak_ceiling: f32,
    ) -> Self {
        let mut violations: Vec<String> = Vec::new();

        // LUFS compliance (±0.5 LU tolerance per BMR-128 spec)
        let lufs_delta = target_lufs.map(|target| measurement.integrated_lufs - target);

        if let Some(delta) = lufs_delta {
            if delta.abs() > 0.5 {
                violations.push(format!(
                    "LUFS out of tolerance: {:.1} LUFS (target {:.1}, delta {:+.2})",
                    measurement.integrated_lufs,
                    target_lufs.unwrap(),
                    delta
                ));
            }
        }

        // True peak compliance
        let peak_headroom = true_peak_ceiling - measurement.true_peak_dbfs; // Changed to dbfs
        if peak_headroom < 0.0 {
            violations.push(format!(
                "True peak exceeds ceiling: {:.2} dBTP (ceiling {:.1}, excess {:.2})",
                measurement.true_peak_dbfs, true_peak_ceiling, -peak_headroom
            ));
        }

        Bmr128Report {
            version: "1.0".to_string(),
            preset: preset.to_string(),
            target_lufs,
            true_peak_ceiling,
            measured: MeasuredValues {
                integrated_lufs: measurement.integrated_lufs,
                true_peak_dbtp: measurement.true_peak_dbfs,
                loudness_range_lu: measurement.loudness_range_lu,
                stereo_correlation: pre_analysis.global_phase_correlation,
                dynamic_range_db: pre_analysis.dynamic_range_db,
                momentary_lufs: pre_analysis.integrated_lufs, // proxy for momentary_lufs
                short_term_lufs: measurement.short_term_lufs.unwrap_or(-14.0),
                bpm: pre_analysis.bpm,
                transient_density: pre_analysis.transient_density,
            },
            compliance: ComplianceResult {
                passes: violations.is_empty(),
                lufs_delta,
                peak_headroom,
                violations,
            },
        }
    }

    /// Serialize this report to a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::Ebu128Measurement;

    fn test_measurement() -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs: -14.1,
            true_peak_dbfs: -1.2,
            loudness_range_lu: 6.5,
            short_term_lufs: Some(-13.5),
        }
    }

    #[test]
    fn test_bmr128_passes_within_tolerance() {
        let m = test_measurement(); // -14.1 vs target -14.0 → delta = -0.1 → passes
        let mut pre = lineos_types::PreAnalysisData::silent();
        pre.bpm = 120.0;
        let report = Bmr128Report::generate(&m, &pre, "spotify", Some(-14.0), -1.0);
        assert!(
            report.compliance.passes,
            "Should pass within ±0.5 LU tolerance"
        );
        assert!(report.compliance.violations.is_empty());
    }

    #[test]
    fn test_bmr128_fails_lufs_out_of_tolerance() {
        let mut m = test_measurement();
        m.integrated_lufs = -12.0; // delta = +2.0 → exceeds ±0.5
        let report = Bmr128Report::generate(
            &m,
            &lineos_types::PreAnalysisData::silent(),
            "spotify",
            Some(-14.0),
            -1.0,
        );
        assert!(!report.compliance.passes);
        assert!(!report.compliance.violations.is_empty());
    }

    #[test]
    fn test_bmr128_fails_true_peak_exceeded() {
        let mut m = test_measurement();
        m.true_peak_dbfs = -0.5; // exceeds -1.0 ceiling
        let report = Bmr128Report::generate(
            &m,
            &lineos_types::PreAnalysisData::silent(),
            "spotify",
            Some(-14.0),
            -1.0,
        );
        assert!(!report.compliance.passes);
        assert!(report.compliance.peak_headroom < 0.0);
    }

    #[test]
    fn test_bmr128_raw_preset_no_lufs_check() {
        let m = test_measurement();
        let report = Bmr128Report::generate(
            &m,
            &lineos_types::PreAnalysisData::silent(),
            "raw",
            None,
            -0.1,
        );
        // Raw preset has no LUFS target → no LUFS violation possible
        assert!(report.compliance.lufs_delta.is_none());
    }

    #[test]
    fn test_bmr128_serializes_to_json() {
        let m = test_measurement();
        let report = Bmr128Report::generate(
            &m,
            &lineos_types::PreAnalysisData::silent(),
            "spotify",
            Some(-14.0),
            -1.0,
        );
        let json = report.to_json().unwrap();
        assert!(json.contains("spotify"));
        assert!(json.contains("compliance"));
    }
}
