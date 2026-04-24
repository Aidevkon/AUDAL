//! panels/coach.rs — Coach Panel (FM5 narrative + findings) · P11-007
//! Authority: Phase 11 task-decomposition P11-007 · LLM Adapter Amendment v1.1
//!
//! THE COACH panel (right MFD):
//!   FM5: narrative summary + per-finding cards
//!   Narrative is Optional — None if Ollama unavailable (non-fatal)
//!   TEACHER VOICE ENFORCED: no DSP values in coach output
//!   CorrelationRadar = static placeholder (A-003 §3, no canvas)

use dioxus::prelude::*;
use crate::components::module_frame::ModuleFrame;
use wasm_bindgen_futures::spawn_local;
use serde_json::json;
use crate::ipc::invoke;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{IssueJson, SessionStateJson};

#[derive(PartialEq, Clone)]
pub struct FindingData {
    pub label: String,
    pub desc: String,
    pub pct: f32,
    pub sev: String,
}

#[component]
pub fn CoachPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let state = session_state.read();

    let demo_findings = vec![
        FindingData {
            label: "Dynamic Range Check".to_string(),
            desc: "Consistency needed".to_string(),
            pct: 40.0,
            sev: "medium".to_string(),
        },
        FindingData {
            label: "Loudness Target".to_string(),
            desc: "Meeting -14 LUFS".to_string(),
            pct: 60.0,
            sev: "low".to_string(),
        },
        FindingData {
            label: "Stereo Width".to_string(),
            desc: "Review correlation in lows".to_string(),
            pct: 30.0,
            sev: "high".to_string(),
        },
    ];

    rsx! {
        ModuleFrame {
            title: "SOCRATIC COACH".to_string(),
            is_scrollable: true,
            
            { match state.as_ref() {
                        Some(s) => rsx! {
                            if let Some(ref n) = s.narrative {
                                NarrativeSummary {
                                    summary:    n.summary.clone(),
                                    model_used: n.model_used.clone(),
                                }
                            } else {
                                div {
                                    style: "padding:1rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
                                    div {
                                        style: "color:var(--text-muted); font-size:0.7rem;
                                                font-style:italic;",
                                        "Coach narrative unavailable — Ollama not running."
                                    }
                                }
                            }

                            div {
                                style: "padding:0.75rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
                                div {
                                    style: "color:var(--text-secondary); font-size:0.6rem;
                                            letter-spacing:0.15em; text-transform:uppercase;
                                            margin-bottom:0.35rem;",
                                    "RECOMMENDATION"
                                }
                                div {
                                    style: "color:var(--text-primary); font-size:0.8rem;
                                            line-height:1.5;",
                                    "{s.findings.recommendation}"
                                }
                            }

                            ScoreBar {
                                pass:   s.findings.issues.is_empty(),
                                issues: s.findings.issues.len(),
                            }

                            if !s.findings.issues.is_empty() {
                                div {
                                    style: "padding:0.5rem 1.5rem 0.25rem;
                                            color:var(--text-muted); font-size:0.6rem;
                                            letter-spacing:0.2em; text-transform:uppercase;",
                                    "FINDINGS  ({s.findings.issues.len()})"
                                }
                            }
                            for issue in &s.findings.issues {
                                FindingRow { issue: issue.clone() }
                            }

                            if !s.findings.issues.is_empty() {
                                CoachActions {}
                            }
                        },
                        None => rsx! {
                            div {
                                style: "padding:0.5rem 0.75rem 0.2rem;
                                        color:var(--text-muted); font-size:0.6rem;
                                        letter-spacing:0.2em; text-transform:uppercase;",
                                "FINDINGS  ({demo_findings.len()})"
                            }
                            for f in demo_findings {
                                DemoFindingRow { finding: f }
                            }
                            CoachActions {}
                        }
            } }
        }
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────────

#[component]
fn NarrativeSummary(summary: String, model_used: String) -> Element {
    rsx! {
        div {
            style: "padding:1rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
            div {
                style: "color:var(--text-secondary); font-size:0.6rem;
                        letter-spacing:0.15em; text-transform:uppercase;
                        margin-bottom:0.5rem;",
                "COACH ANALYSIS"
            }
            div {
                style: "color:var(--text-primary); font-size:0.82rem;
                        line-height:1.6; font-style:italic;",
                "{summary}"
            }
            div {
                style: "color:var(--text-muted); font-size:0.6rem;
                        margin-top:0.5rem;",
                "model: {model_used}"
            }
        }
    }
}
// ── DemoFindingRow — static demo card for FM0 state ──────────────────────────

/// Static finding row for the FM0 demo state.
/// Takes plain values — no IssueJson. Matches mockup's 3 demo cards.
#[component]
fn DemoFindingRow(finding: FindingData) -> Element {
    let pct_label = format!("{:.0}%", finding.pct);
    let label = finding.label;
    let sev = finding.sev;
    let desc = finding.desc;
    let pct = finding.pct;
    rsx! {
        div {
            class: "finding-row",
            div {
                class: "finding-row-header",
                div { class: "finding-row-label", "{label}" }
                div { class: "severity-dot {sev}" }
            }
            div { class: "finding-row-description", "{desc}" }
            div {
                class: "finding-bar-label",
                div { class: "finding-bar-track", style: "flex:1;",
                    div { class: "finding-bar-fill", style: "width:{pct}%;" }
                }
                div { class: "finding-bar-pct", "{pct_label}" }
            }
        }
    }
}

#[component]
fn ScoreBar(pass: bool, issues: usize) -> Element {
    let (label, color) = if pass {
        ("ALL CLEAR", "var(--accent-cyan)")
    } else if issues <= 1 {
        ("MINOR ISSUES", "var(--accent-amber)")
    } else {
        ("REVIEW NEEDED", "var(--severity-high)")
    };

    rsx! {
        div {
            style: "padding:0.75rem 1.5rem; border-bottom:1px solid var(--border-subtle);
                    display:flex; align-items:center; gap:0.75rem;",
            div {
                style: "width:8px; height:8px; border-radius:50%;
                        background:{color}; box-shadow:0 0 6px {color};
                        flex-shrink:0;",
            }
            div {
                style: "color:{color}; font-size:0.65rem; letter-spacing:0.15em;
                        text-transform:uppercase; font-weight:600;",
                "{label}"
            }
            if issues > 0 {
                div {
                    style: "color:var(--text-muted); font-size:0.65rem;",
                    "— {issues} finding(s)"
                }
            }
        }
    }
}

// ── P14-008: FindingRow (progress bar + severity dot) ─────────────────────

/// FindingRow per §4.10: label + description + progress bar + severity dot.
/// Colors via CSS variables only — no inline hex.
#[component]
fn FindingRow(issue: IssueJson) -> Element {
    // Score = how close current is to target (0–1 range)
    // Higher delta = worse. Clamp score to 0–1.
    let score_pct = if issue.target != 0.0 {
        let ratio = (issue.target - issue.delta.abs()) / issue.target.abs();
        (ratio * 100.0).clamp(0.0, 100.0)
    } else {
        50.0_f32  // unknown target
    };
    let pct_label = format!("{:.0}%", score_pct);
    let sev = issue.severity.as_str();

    rsx! {
        div {
            key:   "{issue.id}",
            class: "finding-row",

            // Header: label + severity dot
            div {
                class: "finding-row-header",
                div {
                    class: "finding-row-label",
                    "{issue.id}"
                }
                div { class: "severity-dot {sev}" }
            }

            // Description: current → target
            div {
                class: "finding-row-description",
                { format!("current {:.1} → target {:.1} (delta {:+.1})",
                          issue.current, issue.target, issue.delta) }
            }

            // Progress bar + percentage
            div {
                class: "finding-bar-label",
                div { class: "finding-bar-track", style: "flex:1;",
                    div {
                        class: "finding-bar-fill",
                        style: "width:{score_pct}%;",
                    }
                }
                div { class: "finding-bar-pct", "{pct_label}" }
            }
        }
    }
}

/// YES / NO coach action buttons (§4.10).
/// invoke("coachAction", { action: "yes" | "no" }) on click.
#[component]
fn CoachActions() -> Element {
    rsx! {
        div {
            class: "coach-actions",

            button {
                id:      "btn-coach-yes",
                class:   "btn-yes",
                onclick: move |_| {
                    spawn_local(async move {
                        let _ = invoke::<String, _>(
                            "coach_action",
                            json!({ "action": "yes" }),
                        ).await;
                    });
                },
                "YES"
            }

            button {
                id:      "btn-coach-no",
                class:   "btn-no",
                onclick: move |_| {
                    spawn_local(async move {
                        let _ = invoke::<String, _>(
                            "coach_action",
                            json!({ "action": "no" }),
                        ).await;
                    });
                },
                "NO"
            }
        }
    }
}

// ── Legacy sub-components (kept for backwards compat during transition) ────────

#[component]
fn MetricMini(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            div {
                style: "color:var(--text-muted); font-size:0.55rem;
                        letter-spacing:0.1em; text-transform:uppercase;",
                "{label}"
            }
            div {
                style: "color:var(--text-secondary); font-size:0.75rem;
                        font-variant-numeric:tabular-nums; font-weight:500;",
                "{value}"
            }
        }
    }
}
