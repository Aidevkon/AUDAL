//! panels/coach.rs — Coach Panel (FM5 narrative + findings) · P11-007
//! Authority: Phase 11 task-decomposition P11-007 · LLM Adapter Amendment v1.1
//!
//! THE COACH panel (right MFD):
//!   FM5: narrative summary + per-finding cards
//!   Narrative is Optional — None if Ollama unavailable (non-fatal)
//!   TEACHER VOICE ENFORCED: no DSP values in coach output
//!   CorrelationRadar = static placeholder (A-003 §3, no canvas)

use dioxus::prelude::*;
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

                        // ── Finding cards ─────────────────────────────────────
                        if !s.findings.issues.is_empty() {
                            div {
                                style: "padding:0.5rem 1.5rem 0.25rem;
                                        color:var(--text-muted); font-size:0.6rem;
                                        letter-spacing:0.2em; text-transform:uppercase;",
                                "FINDINGS  ({s.findings.issues.len()})"
                            }
                        }
                        for issue in &s.findings.issues {
                            FindingCard { issue: issue.clone() }
                        }

                        // ── Correlation radar placeholder (A-003 §3) ─────────
                        RadarPlaceholder {}
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

#[component]
fn FindingCard(issue: IssueJson) -> Element {
    let (bg_color, text_color) = match issue.severity.as_str() {
        "high"   => ("rgba(239,68,68,0.08)",  "var(--status-err)"),
        "medium" => ("rgba(251,191,36,0.08)", "var(--status-warn)"),
        "low"    => ("rgba(59,130,246,0.08)", "var(--status-info)"),
        _        => ("rgba(255,255,255,0.04)","var(--text-muted)"),
    };
    let severity_upper = issue.severity.to_uppercase();

    rsx! {
        div {
            key: "{issue.id}",
            style: "margin:0.4rem 1rem; padding:0.75rem;
                    background:{bg_color}; border-radius:6px;
                    border-left:3px solid {text_color};",

            // Header row
            div {
                style: "display:flex; justify-content:space-between;
                        align-items:center; margin-bottom:0.4rem;",
                div {
                    style: "color:{text_color}; font-size:0.65rem;
                            letter-spacing:0.15em; text-transform:uppercase;
                            font-weight:700;",
                    "{severity_upper}"
                }
                div {
                    style: "color:var(--text-muted); font-size:0.6rem;
                            font-family:monospace;",
                    "{issue.id}"
                }
            }

            // Delta row
            div {
                style: "display:grid; grid-template-columns:1fr 1fr 1fr; gap:0.5rem;",
                MetricMini { label: "CURRENT", value: format!("{:.1}", issue.current) }
                MetricMini { label: "TARGET",  value: format!("{:.1}", issue.target) }
                MetricMini { label: "DELTA",   value: format!("{:+.1}", issue.delta) }
            }

            // Tags
            if !issue.tags.is_empty() {
                div {
                    style: "margin-top:0.4rem; display:flex; flex-wrap:wrap; gap:0.25rem;",
                    for tag in &issue.tags {
                        div {
                            key: "{tag}",
                            style: "background:rgba(255,255,255,0.05);
                                    color:var(--text-muted); font-size:0.55rem;
                                    letter-spacing:0.1em; text-transform:uppercase;
                                    padding:0.15rem 0.4rem; border-radius:3px;",
                            "{tag}"
                        }
                    }
                }
            }
        }
    }
}

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

/// CorrelationRadar placeholder — A-003 §3: no canvas, no live audio in Phase 11.
#[component]
fn RadarPlaceholder() -> Element {
    rsx! {
        div {
            style: "margin:0.5rem 1rem 1rem;
                    border:1px solid var(--border-subtle); border-radius:4px;
                    padding:1.5rem; text-align:center;",
            div {
                style: "font-size:2rem; opacity:0.2; margin-bottom:0.5rem;",
                "◎"
            }
            div {
                style: "color:var(--text-muted); font-size:0.6rem; letter-spacing:0.1em;",
                "CORRELATION RADAR · PHASE 12"
            }
        }
    }
}
