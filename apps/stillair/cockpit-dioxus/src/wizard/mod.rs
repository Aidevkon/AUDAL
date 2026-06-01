//! Wizard — diagnostic finding detection.
//! Authority: Wizard Constitution v1.1
//! Pure function. No personality. No DSP modification.
//! Input: SessionStateJson → Output: Vec<WizardFinding>

use crate::types::SessionStateJson;

#[derive(Debug, Clone, PartialEq)]
pub struct WizardFinding {
    pub id:       &'static str,
    pub severity: WizardSeverity,
    pub mfd:      MfdTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WizardSeverity { High, Medium, Low }

#[derive(Debug, Clone, PartialEq)]
pub enum MfdTarget { Mfd1SignalAnalyzer, Mfd2SpatialTelemetry }

/// Deterministic finding detection — same input = same findings, always.
/// INV-WZ-1: no personality. INV-WZ-2: read-only.
pub fn detect_findings(session: &SessionStateJson) -> Vec<WizardFinding> {
    let mut findings = vec![];
    let q = &session.quality;
    let l = &session.loudness;

    // phase_issue — INV-WZ-7: spatial → MFD2
    if q.stereo_correlation < 0.3 {
        findings.push(WizardFinding {
            id: "phase_issue",
            severity: WizardSeverity::High,
            mfd: MfdTarget::Mfd2SpatialTelemetry,
        });
    }

    // stereo_collapse
    if q.stereo_width < 0.1 {
        findings.push(WizardFinding {
            id: "stereo_collapse",
            severity: WizardSeverity::High,
            mfd: MfdTarget::Mfd2SpatialTelemetry,
        });
    }

    // true_peak_clip
    if l.true_peak_dbtp > -0.1 {
        findings.push(WizardFinding {
            id: "true_peak_clip",
            severity: WizardSeverity::High,
            mfd: MfdTarget::Mfd1SignalAnalyzer,
        });
    }

    // lufs_deviation
    let lufs_delta = (l.integrated_lufs - l.ebu_r128_target_lufs).abs();
    if lufs_delta > 3.0 {
        findings.push(WizardFinding {
            id: "lufs_deviation",
            severity: WizardSeverity::Medium,
            mfd: MfdTarget::Mfd1SignalAnalyzer,
        });
    }

    // dynamic_crush
    if q.dynamic_range_db < 8.0 {
        findings.push(WizardFinding {
            id: "dynamic_crush",
            severity: WizardSeverity::Low,
            mfd: MfdTarget::Mfd1SignalAnalyzer,
        });
    }

    // Zone flags from PreAnalysis (RFC-008)
    if let Some(ref zf) = session.zone_flags {
        if zf.zone_phase_issue {
            findings.push(WizardFinding {
                id: "phase_issue",
                severity: WizardSeverity::High,
                mfd: MfdTarget::Mfd2SpatialTelemetry,
            });
        }
        if zf.zone_sub_rumble {
            findings.push(WizardFinding {
                id: "sub_rumble",
                severity: WizardSeverity::Medium,
                mfd: MfdTarget::Mfd2SpatialTelemetry,
            });
        }
        if zf.zone_cymbal_harsh {
            findings.push(WizardFinding {
                id: "cymbal_harsh",
                severity: WizardSeverity::Medium,
                mfd: MfdTarget::Mfd1SignalAnalyzer,
            });
        }
        if zf.zone_boxiness {
            findings.push(WizardFinding {
                id: "boxiness",
                severity: WizardSeverity::Low,
                mfd: MfdTarget::Mfd1SignalAnalyzer,
            });
        }
        if zf.zone_harsh_resonance {
            findings.push(WizardFinding {
                id: "harsh_resonance",
                severity: WizardSeverity::Medium,
                mfd: MfdTarget::Mfd1SignalAnalyzer,
            });
        }
    }

    // PostFlightVerifier results (W-09)
    if let Some(ref vr) = session.verification {
        if !vr.passed {
            findings.push(WizardFinding {
                id: "verification_failed",
                severity: WizardSeverity::High,
                mfd: MfdTarget::Mfd1SignalAnalyzer,
            });
        }
        if vr.was_trimmed {
            findings.push(WizardFinding {
                id: "true_peak_clip",
                severity: WizardSeverity::High,
                mfd: MfdTarget::Mfd1SignalAnalyzer,
            });
        }
    }

    findings
}

/// Finding cooldown tracker.
/// Prevents same finding from re-triggering before cooldown expires.
/// Relevant only when real-time telemetry polling is added.
/// Authority: Wizard Constitution v1.1 §6 — 45 second cooldown.
pub struct CooldownTracker {
    dismissed_at: std::collections::HashMap<&'static str, f64>,
    pub cooldown_ms: f64,
}

impl CooldownTracker {
    pub fn new() -> Self {
        Self {
            dismissed_at: std::collections::HashMap::new(),
            cooldown_ms: 45_000.0,
        }
    }

    pub fn dismiss(&mut self, id: &'static str, now_ms: f64) {
        self.dismissed_at.insert(id, now_ms);
    }

    pub fn is_cooled_down(&self, id: &'static str, now_ms: f64) -> bool {
        match self.dismissed_at.get(id) {
            None => true,
            Some(&dismissed) => (now_ms - dismissed) >= self.cooldown_ms,
        }
    }

    pub fn filter_findings<'a>(
        &self,
        findings: &'a [WizardFinding],
        now_ms: f64,
    ) -> Vec<&'a WizardFinding> {
        findings.iter()
            .filter(|f| self.is_cooled_down(f.id, now_ms))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn clean_session() -> SessionStateJson {
        SessionStateJson {
            blob_id: "test_blob".to_string(),
            loudness: LoudnessMetricsJson {
                integrated_lufs: -14.2,
                short_term_lufs: -12.1,
                momentary_lufs: -10.5,
                true_peak_dbtp: -0.3,
                lra: 4.5,
                k_weighted: true,
                ebu_r128_target_lufs: -14.0,
                ebu_r128_compliant: true,
                spotify_compliant: true,
                youtube_compliant: true,
                apple_music_compliant: true,
                apple_podcasts_compliant: true,
                broadcast_compliant: true,
                tidal_compliant: true,
            },
            quality: QualityMetricsJson {
                stereo_correlation: 0.85,
                phase_coherence: 0.92,
                stereo_width: 0.75,
                dynamic_range_db: 8.5,
                rms_db: -12.0,
                spectral_centroid: 2500.0,
                spectral_flatness: 0.15,
                clips_detected: 0,
                clip_free: true,
            },
            compliance: ComplianceJson {
                spotify: true,
                youtube: true,
                apple: true,
                tidal: true,
                broadcast: true,
                ebu_r128: true,
            },
            findings: CoachFindingsJson {
                issues: vec![],
                recommendation: "".to_string(),
            },
            narrative: None,
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            zone_flags: Some(ZoneFlagsJson::default()),
            verification: Some(VerificationResultJson::default()),
            jini: None,
            dsp_chain: None,
        }
    }

    #[test]
    fn healthy_signal_produces_no_findings() {
        let session = clean_session();
        let findings = detect_findings(&session);
        assert!(findings.is_empty());
    }

    #[test]
    fn phase_issue_triggers_on_low_correlation() {
        let mut session = clean_session();
        session.quality.stereo_correlation = 0.2;
        let findings = detect_findings(&session);
        assert!(findings.contains(&WizardFinding {
            id: "phase_issue",
            severity: WizardSeverity::High,
            mfd: MfdTarget::Mfd2SpatialTelemetry,
        }));
    }

    #[test]
    fn true_peak_clip_triggers_correctly() {
        let mut session = clean_session();
        session.loudness.true_peak_dbtp = 0.0;
        let findings = detect_findings(&session);
        assert!(findings.contains(&WizardFinding {
            id: "true_peak_clip",
            severity: WizardSeverity::High,
            mfd: MfdTarget::Mfd1SignalAnalyzer,
        }));
    }
}
