//! WizardAgent — R2: telemetry → findings JSON.
//! Authority: Constitutional Agent Architecture Spec v3.1 §2.2
//! Motto: "I build the truth from data. Others show it."
//!
//! Pure rule-based analysis. No LLM. No randomness.
//! Same metrics → same findings. Always.
//! INV-AB-1: deterministic.

use super::operator::{AudioMetrics, Finding, Intent};
use tokio::sync::mpsc;

/// Apply deterministic rules to AudioMetrics → Vec<Finding>.
/// If-this-then-that only. No ML. No heuristics.
pub fn analyze(metrics: &AudioMetrics) -> Vec<Finding> {
    let mut findings = Vec::new();

    // LUFS compliance
    if metrics.integrated_lufs < -20.0 {
        findings.push(Finding {
            category: "loudness".into(),
            severity: "warning".into(),
            message: format!(
                "Integrated loudness {:.1} LUFS is very low. \
                 Consider increasing gain.",
                metrics.integrated_lufs
            ),
        });
    } else if metrics.integrated_lufs > -9.0 {
        findings.push(Finding {
            category: "loudness".into(),
            severity: "warning".into(),
            message: format!(
                "Integrated loudness {:.1} LUFS exceeds \
                 recommended range.",
                metrics.integrated_lufs
            ),
        });
    } else {
        findings.push(Finding {
            category: "loudness".into(),
            severity: "pass".into(),
            message: format!(
                "Integrated loudness {:.1} LUFS — within target.",
                metrics.integrated_lufs
            ),
        });
    }

    // True peak compliance
    if metrics.true_peak_dbtp > -1.0 {
        findings.push(Finding {
            category: "true_peak".into(),
            severity: "error".into(),
            message: format!(
                "True peak {:.1} dBTP exceeds −1.0 dBTP ceiling. \
                 ITU-R BS.1770-4 violation.",
                metrics.true_peak_dbtp
            ),
        });
    } else if metrics.true_peak_dbtp > -1.5 {
        findings.push(Finding {
            category: "true_peak".into(),
            severity: "warning".into(),
            message: format!(
                "True peak {:.1} dBTP is close to ceiling.",
                metrics.true_peak_dbtp
            ),
        });
    } else {
        findings.push(Finding {
            category: "true_peak".into(),
            severity: "pass".into(),
            message: format!("True peak {:.1} dBTP — compliant.", metrics.true_peak_dbtp),
        });
    }

    // LRA — dynamic range
    if metrics.lra_lu < 1.0 {
        findings.push(Finding {
            category: "dynamics".into(),
            severity: "warning".into(),
            message: format!(
                "Loudness range {:.1} LU is very compressed.",
                metrics.lra_lu
            ),
        });
    } else if metrics.lra_lu > 20.0 {
        findings.push(Finding {
            category: "dynamics".into(),
            severity: "info".into(),
            message: format!(
                "Loudness range {:.1} LU is wide — \
                 suitable for classical or film.",
                metrics.lra_lu
            ),
        });
    } else {
        findings.push(Finding {
            category: "dynamics".into(),
            severity: "pass".into(),
            message: format!(
                "Loudness range {:.1} LU — healthy dynamics.",
                metrics.lra_lu
            ),
        });
    }

    findings
}

/// WizardAgent main loop.
pub async fn run(mut rx: mpsc::Receiver<Intent>) {
    while let Some(intent) = rx.recv().await {
        match intent {
            Intent::Shutdown => break,
            Intent::AnalyzeTelemetry { metrics, response } => {
                let findings = analyze(&metrics);
                let _ = response.send(findings);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(lufs: f32, tp: f32, lra: f32) -> AudioMetrics {
        AudioMetrics {
            integrated_lufs: lufs,
            true_peak_dbtp: tp,
            lra_lu: lra,
        }
    }

    #[test]
    fn compliant_master_all_pass() {
        let f = analyze(&metrics(-14.0, -1.5, 8.0));
        assert!(f.iter().all(|x| x.severity == "pass"));
    }

    #[test]
    fn true_peak_violation_is_error() {
        let f = analyze(&metrics(-14.0, -0.5, 8.0));
        let tp = f.iter().find(|x| x.category == "true_peak").unwrap();
        assert_eq!(tp.severity, "error");
    }

    #[test]
    fn low_lufs_is_warning() {
        let f = analyze(&metrics(-25.0, -2.0, 8.0));
        let l = f.iter().find(|x| x.category == "loudness").unwrap();
        assert_eq!(l.severity, "warning");
    }

    #[test]
    fn compressed_dynamics_is_warning() {
        let f = analyze(&metrics(-14.0, -2.0, 0.5));
        let d = f.iter().find(|x| x.category == "dynamics").unwrap();
        assert_eq!(d.severity, "warning");
    }

    #[test]
    fn deterministic_same_input_same_output() {
        let m = metrics(-14.0, -1.5, 8.0);
        let f1 = analyze(&m);
        let f2 = analyze(&m);
        assert_eq!(f1.len(), f2.len());
        for (a, b) in f1.iter().zip(f2.iter()) {
            assert_eq!(a.category, b.category);
            assert_eq!(a.severity, b.severity);
        }
    }
}
