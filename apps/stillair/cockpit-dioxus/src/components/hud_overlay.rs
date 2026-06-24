//! HUD Overlay — F16 spatial diagnostic display.
//! Authority: Wizard Constitution v1.1 §4, §7
//! Renders OVER the MFD where the finding occurs — not a modal.
//! INV-WZ-7: findings appear spatially, exactly where the issue occurs.
//! INV-WZ-10: one HUD instance per MFD, new finding supersedes old.

use crate::wizard::{MfdTarget, WizardFinding, WizardSeverity};
use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct MfdHudProps {
    pub findings: Vec<WizardFinding>,
    pub target: MfdTarget,
    pub on_dismiss: EventHandler<&'static str>, // finding id
}

/// Renders finding indicators spatially over a specific MFD.
/// Position: absolute, covers the MFD column.
/// Only renders findings that belong to this MFD target.
#[component]
pub fn MfdHud(props: MfdHudProps) -> Element {
    let valid = validated_findings(&props.findings);
    let mfd_findings: Vec<&&WizardFinding> =
        valid.iter().filter(|f| f.mfd == props.target).collect();

    if mfd_findings.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "hud-mfd-overlay",
            style: "position: absolute; top: 0; left: 0; \
                    width: 100%; height: 100%; \
                    pointer-events: none; \
                    z-index: 100;",

            for finding in mfd_findings {
                div {
                    key: "{finding.id}",
                    class: format!("hud-finding hud-severity-{}",
                        severity_str(&finding.severity)),
                    style: "pointer-events: auto; \
                            font-family: monospace; font-size: 9px; \
                            letter-spacing: 0.15em; text-transform: uppercase; \
                            padding: 4px 8px; \
                            border-left: 2px solid currentColor; \
                            background: rgba(0,0,0,0.75); \
                            display: flex; justify-content: space-between; \
                            align-items: center; gap: 12px; \
                            cursor: pointer; \
                            {spatial_style(finding.id)}",
                    onclick: {
                        let id = finding.id;
                        let dismiss = props.on_dismiss;
                        move |_| dismiss.call(id)
                    },
                    span { "⚠ {finding.id}" }
                    span {
                        style: "font-size: 8px; opacity: 0.6;",
                        "{severity_label(&finding.severity)}"
                    }
                }
            }
        }
    }
}

fn spatial_style(id: &str) -> &'static str {
    match id {
        // MFD1 (PSA)
        "true_peak_clip" => "position: absolute; top: 32px; right: 16px;",
        "lufs_deviation" => "position: absolute; top: 72px; right: 16px;",
        "dynamic_crush" => "position: absolute; bottom: 32px; right: 16px;",
        "cymbal_harsh" => "position: absolute; top: 30%; right: 25%;",
        "boxiness" => "position: absolute; bottom: 30%; left: 25%;",
        "harsh_resonance" => "position: absolute; top: 45%; left: 45%;",
        "verification_failed" => {
            "position: absolute; top: 12px; left: 50%; transform: translateX(-50%);"
        }

        // MFD2 (Spatial)
        "phase_issue" => "position: absolute; bottom: 32px; right: 16px;",
        "stereo_collapse" => "position: absolute; top: 32px; left: 16px;",
        "sub_rumble" => "position: absolute; bottom: 32px; left: 16px;",

        // Fallback
        _ => "position: absolute; bottom: 16px; left: 16px;",
    }
}

fn severity_str(s: &WizardSeverity) -> &'static str {
    match s {
        WizardSeverity::High => "high",
        WizardSeverity::Medium => "medium",
        WizardSeverity::Low => "low",
    }
}

fn severity_label(s: &WizardSeverity) -> &'static str {
    match s {
        WizardSeverity::High => "HIGH",
        WizardSeverity::Medium => "MED",
        WizardSeverity::Low => "LOW",
    }
}

fn validated_findings(findings: &[WizardFinding]) -> Vec<&WizardFinding> {
    let mut seen = std::collections::HashSet::new();
    findings
        .iter()
        .filter(|f| seen.insert(f.id))
        .take(8)
        .collect()
}
