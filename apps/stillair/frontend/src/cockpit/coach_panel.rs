//! CoachPanel — Right MFD. FM5 CoachFindings display.
//! Authority: Phase 5 task-decomposition P5-007 · state-machine.md §7
//!
//! FM0-FM4:  Off ("—")
//! FM5:      CoachFindings list + recommendation
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

use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::CoachFindings;

#[component]
pub fn CoachPanel(
    mode:     ReadSignal<CockpitMode>,
    findings: ReadSignal<Option<CoachFindings>>,
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
                        match findings.get() {
                            Some(f) => view! { <FindingsList findings=f /> }.into_any(),
                            None    => view! {
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

#[component]
fn FindingsList(findings: CoachFindings) -> impl IntoView {
    let recommendation = findings.recommendation.clone();
    let issues = findings.issues.clone();

    view! {
        <div class="findings-list">
            <div class="recommendation-box">
                <div class="recommendation-label">"Recommendation"</div>
                <div class="recommendation-text">{recommendation}</div>
            </div>

            {issues.into_iter().map(|issue| view! {
                <div class=format!("finding-card {}", issue.severity.card_class())>
                    <div class="finding-header">
                        <span class="finding-id">{issue.id.clone()}</span>
                        <span class=format!("severity-badge {}", issue.severity.badge_class())>
                            {issue.severity.display_name()}
                        </span>
                    </div>
                    <div class="finding-params">
                        <span>"cur: "{format!("{:.1}", issue.current)}</span>
                        <span>"tgt: "{format!("{:.1}", issue.target)}</span>
                        <span>"Δ: "{format!("{:+.1}", issue.delta)}</span>
                    </div>
                    {if !issue.tags.is_empty() {
                        view! {
                            <div class="finding-params">
                                {issue.tags.join("  ")}
                            </div>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            }).collect_view()}
        </div>
    }
}
