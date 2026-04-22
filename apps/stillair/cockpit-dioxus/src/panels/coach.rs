//! panels/coach.rs — Coach Panel (FM5 narrative + findings) · P11-007
//! Authority: Phase 11 task-decomposition P11-007 · LLM Adapter Amendment v1.1
//!
//! THE COACH panel (right MFD):
//!   FM5: narrative summary + per-finding cards
//!   Narrative is Optional — None if Ollama unavailable (non-fatal)
//!   TEACHER VOICE ENFORCED: no DSP values in coach output
//!   CorrelationRadar = static placeholder (A-003 §3, no canvas)

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use serde_json::json;
use crate::ipc::invoke;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{IssueJson, SessionStateJson};

#[component]
pub fn CoachPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let state = session_state.read();

    rsx! {
        div {
            class: "mfd-panel panel-coach",
            style: "display:flex; flex-direction:column; overflow:hidden;",

            // Panel title
            div {
                class: "panel-title",
                style: "color:var(--accent-coach);
                        border-bottom:2px solid var(--accent-coach);
                        padding:1rem 1.5rem 0.5rem;
                        font-size:0.7rem; letter-spacing:0.2em;
                        text-transform:uppercase; font-weight:600;
                        flex-shrink:0;",
                "THE COACH"
            }

            div {
                style: "flex:1; overflow-y:auto;",
                match state.as_ref() {
                    Some(s) => rsx! {
                        // ── Narrative summary ─────────────────────────────────
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

                        // ── Recommendation ────────────────────────────────────
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

                        // ── Pass/fail score indicator ─────────────────────────
                        ScoreBar {
                            pass:   s.findings.issues.is_empty(),
                            issues: s.findings.issues.len(),
                        }

                        // ── Finding rows (P14-008: progress bars + severity dots) ────
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

                        // ── YES / NO action buttons (§4.10) ──────────────────────
                        if !s.findings.issues.is_empty() {
                            CoachActions {}
                        }
                    },
                    None => rsx! {
                        div {
                            style: "color:var(--text-muted); padding:2rem;
                                    text-align:center; font-size:0.75rem;
                                    letter-spacing:0.1em;",
                            "—"
                        }
                    }
                }
            }
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
                ""{summary}""
            }
            div {
                style: "color:var(--text-muted); font-size:0.6rem;
                        margin-top:0.5rem;",
                "model: {model_used}"
            }
        }
    }
}

#[component]
fn ScoreBar(pass: bool, issues: usize) -> Element {
    let (label, color) = if pass {
        ("ALL CLEAR", "var(--status-ok)")
    } else if issues <= 1 {
        ("MINOR ISSUES", "var(--status-warn)")
    } else {
        ("REVIEW NEEDED", "var(--status-err)")
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
