//! CoachPanel — Right MFD. FM5 CoachFindings + CoachNarrative display.
//! Authority: Phase 5 P5-007 · Phase 8 P8-006 · state-machine.md §7
//!
//! FM0-FM4:  Off ("—")
//! FM5:      Summary (LLM narrative) + per-finding cards with explanation + direction
//! FM6:      "Locked during export"
//!
//! Severity colors — design-tokens-v1.0.md §5:
//!   High   → var(--severity-high)   badge-high / sev-high
//!   Medium → var(--severity-medium) badge-medium / sev-medium
//!   Low    → var(--severity-low)    badge-low / sev-low
//!   Info   → var(--severity-info)   badge-info / sev-info
//!
//! FORBIDDEN: No business logic or rule evaluation in this component.
//! FORBIDDEN: No direct import of lineos-rule-engine.
//! FORBIDDEN: No LLM calls — narrative arrives via ReadSignal from app.rs.

use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{CoachFindings, CoachNarrativeJson};

#[component]
pub fn CoachPanel(
    mode:      ReadSignal<CockpitMode>,
    findings:  ReadSignal<Option<CoachFindings>>,
    narrative: ReadSignal<Option<CoachNarrativeJson>>,  // Phase 8
) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-coach">
            <div class="panel-title">
                <span class="panel-dot"></span>
                "THE COACH"
            </div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::CoachReady => {
                        let f = findings.get();
                        let n = narrative.get();
                        match f {
                            Some(findings) => view! {
                                <CoachDisplay findings=findings narrative=n />
                            }.into_any(),
                            None => view! {
                                <div class="coach-off">"No findings"</div>
                            }.into_any(),
                        }
                    },
                    CockpitMode::Exporting => view! {
                        <div class="locked">"Locked during export"</div>
                    }.into_any(),
                    _ => view! {
                        <div class="coach-off">"—"</div>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

/// Full display: narrative summary (if available) + finding cards.
#[component]
fn CoachDisplay(
    findings:  CoachFindings,
    narrative: Option<CoachNarrativeJson>,
) -> impl IntoView {
    let recommendation = findings.recommendation.clone();
    let issues = findings.issues.clone();

    // Build a map of issue_id → FindingExplanation for enriched display
    let explanations: std::collections::HashMap<String, (String, String, String)> =
        narrative.as_ref().map(|n| {
            n.explanations.iter().map(|e| (
                e.issue_id.clone(),
                (e.title.clone(), e.why.clone(), e.suggestion.clone()),
            )).collect()
        }).unwrap_or_default();

    let model_tag = narrative.as_ref().map(|n| n.model_used.clone());
    let summary   = narrative.as_ref().map(|n| n.summary.clone());

    view! {
        <div class="findings-list">

            // ── Summary block (LLM narrative or fallback) ──────────────────
            <div class="recommendation-box">
                {if let Some(s) = summary {
                    view! {
                        <div class="recommendation-label">"SUMMARY"</div>
                        <div class="coach-summary">{s}</div>
                        <div class="recommendation-label" style="margin-top:8px">"ASSESSMENT"</div>
                        <div class="recommendation-text">{recommendation}</div>
                    }.into_any()
                } else {
                    view! {
                        <div class="recommendation-label">"Recommendation"</div>
                        <div class="recommendation-text">{recommendation}</div>
                    }.into_any()
                }}
            </div>

            // ── Per-finding cards ──────────────────────────────────────────
            {issues.into_iter().map(|issue| {
                let explanation = explanations.get(&issue.id).cloned();
                view! {
                    <div class=format!("finding-card {}", issue.severity.card_class())>
                        <div class="finding-header">
                            <span class="finding-id">
                                {explanation.as_ref()
                                    .map(|(title, _, _)| title.clone())
                                    .unwrap_or_else(|| issue.id.clone())}
                            </span>
                            <span class=format!("severity-badge {}", issue.severity.badge_class())>
                                {issue.severity.display_name()}
                            </span>
                        </div>

                        // LLM explanation (if available)
                        {if let Some((_, why, suggestion)) = explanation {
                            view! {
                                <div class="coach-why">{why}</div>
                                <div class="coach-suggestion">"→ "{suggestion}</div>
                            }.into_any()
                        } else {
                            // Fallback: raw metrics when no narrative
                            view! {
                                <div class="finding-params">
                                    <span>"cur: "{format!("{:.1}", issue.current)}</span>
                                    <span>"tgt: "{format!("{:.1}", issue.target)}</span>
                                    <span>"Δ: "{format!("{:+.1}", issue.delta)}</span>
                                </div>
                            }.into_any()
                        }}
                    </div>
                }
            }).collect_view()}

            // ── Model attribution ──────────────────────────────────────────
            {if let Some(model) = model_tag {
                view! {
                    <div class="coach-model-tag">"model: "{model}</div>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}

        </div>
    }
}
