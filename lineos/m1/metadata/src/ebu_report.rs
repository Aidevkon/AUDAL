//! EBU R128 Report — full formatted measurement report.
//! Wraps Ebu128Measurement with serializable JSON output.
//! Authority: LineOS Constitution v2.0 §07

use serde::{Deserialize, Serialize};
use sp314_dsp::types::metrics::Ebu128Measurement;

/// Full EBU R128 report for export or downstream consumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbuReport {
    pub version:    String,
    pub schema:     String,
    pub integrated_lufs:    f32,
    pub true_peak_dbtp:     f32,
    pub loudness_range_lu:  f32,
    pub momentary_lufs:     f32,
    pub short_term_lufs:    f32,
    pub stereo_correlation: f32,
    pub dynamic_range_db:   f32,
    pub sample_rate:        u32,
    pub channels:           u16,
    pub duration_seconds:   f32,
}

impl EbuReport {
    /// Build from an Ebu128Measurement (produced by lineos-telemetry).
    pub fn from_measurement(m: &Ebu128Measurement) -> Self {
        Self {
            version:            "1.0".to_string(),
            schema:             "lineos/shared/schema/ebu-r128.schema.json".to_string(),
            integrated_lufs:    m.integrated_lufs,
            true_peak_dbtp:     m.true_peak_dbtp,
            loudness_range_lu:  m.loudness_range_lu,
            momentary_lufs:     m.momentary_lufs,
            short_term_lufs:    m.short_term_lufs,
            stereo_correlation: m.stereo_correlation,
            dynamic_range_db:   m.dynamic_range_db,
            sample_rate:        m.sample_rate,
            channels:           m.channels,
            duration_seconds:   m.duration_seconds,
        }
    }

    /// Serialize to JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_measurement() -> Ebu128Measurement {
        Ebu128Measurement {
            integrated_lufs:    -14.0,
            true_peak_dbtp:     -1.5,
            loudness_range_lu:  7.2,
            momentary_lufs:     -11.5,
            short_term_lufs:    -13.0,
            stereo_correlation: 0.98,
            dynamic_range_db:   14.0,
            sample_rate:        48000,
            channels:           2,
            duration_seconds:   10.0,
        }
    }

    #[test]
    fn test_ebu_report_round_trips() {
        let m = test_measurement();
        let report = EbuReport::from_measurement(&m);
        assert_eq!(report.integrated_lufs, m.integrated_lufs);
        assert_eq!(report.loudness_range_lu, m.loudness_range_lu);
        assert_eq!(report.sample_rate, 48000);
    }

    #[test]
    fn test_ebu_report_json_contains_schema() {
        let m = test_measurement();
        let report = EbuReport::from_measurement(&m);
        let json = report.to_json().unwrap();
        assert!(json.contains("ebu-r128.schema.json"));
    }
}
