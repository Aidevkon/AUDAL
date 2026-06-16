//! panels/coach.rs — JINI Panel (personality narrative + findings) · J-P5
//! Authority: JINI Spec v1.0 §7 · Phase 11 task-decomposition P11-007
//!
//! THE JINI panel (Hangar Bay, right side):
//!   - JiniNarrative: persona-aware narrative from JINI engine
//!   - JiniActionCard: APPLY / DISMISS for suggested macro/flavour changes
//!   - ScoreBar + WizardFinding rows (unchanged from Coach)
//!     TEACHER VOICE ENFORCED: no DSP values in JINI output

use crate::components::module_frame::ModuleFrame;
use crate::ipc::invoke;
use crate::state::cockpit_event::CockpitEvent;
use crate::state::cockpit_mode::CockpitMode;
use crate::state::reducer::dispatch;
use crate::types::{IssueJson, JiniPersonaState, JiniSuggestionJson, SessionStateJson};
use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

#[derive(PartialEq, Clone)]
pub struct FindingData {
    pub label: String,
    pub desc: String,
    pub pct: f32,
    pub sev: String,
}

#[component]
pub fn CoachPanel(
    mode: Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    wizard_findings: ReadOnlySignal<Vec<crate::wizard::WizardFinding>>,
    jini_suggestion: Signal<Option<JiniSuggestionJson>>,
    jini_persona: Signal<JiniPersonaState>,
) -> Element {
    let state = session_state.read();

    // J-P8: Sync JINI suggestion from session state into signal
    {
        let mut jini_sig = jini_suggestion;
        if let Some(ref s) = *state {
            if let Some(ref j) = s.jini {
                if jini_sig.read().as_ref() != Some(j) {
                    jini_sig.set(Some(j.clone()));
                }
            }
        }
    }

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            let window = web_sys::window().expect("no window");
            let window_val: wasm_bindgen::JsValue = window.into();

            if let Ok(tauri) = js_sys::Reflect::get(&window_val, &wasm_bindgen::JsValue::from_str("__TAURI__")) {
                if !tauri.is_undefined() {
                    if let Ok(event_api) = js_sys::Reflect::get(&tauri, &wasm_bindgen::JsValue::from_str("event")) {
                        if let Ok(listen_val) = js_sys::Reflect::get(&event_api, &wasm_bindgen::JsValue::from_str("listen")) {
                            if let Ok(listen_fn) = listen_val.dyn_into::<js_sys::Function>() {
                                
                                let cb = wasm_bindgen::closure::Closure::<dyn FnMut(wasm_bindgen::JsValue)>::wrap(Box::new(move |ev: wasm_bindgen::JsValue| {
                                    if let Ok(payload) = js_sys::Reflect::get(&ev, &wasm_bindgen::JsValue::from_str("payload")) {
                                        if let Ok(json_str) = js_sys::JSON::stringify(&payload) {
                                            if let Some(stringified) = json_str.as_string() {
                                                if let Ok(suggestion) = serde_json::from_str::<JiniSuggestionJson>(&stringified) {
                                                    let mut sig = jini_suggestion;
                                                    sig.set(Some(suggestion));
                                                }
                                            }
                                        }
                                    }
                                }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
                                
                                let _ = listen_fn.call2(&event_api, &wasm_bindgen::JsValue::from_str("jini://album_fatigue"), cb.as_ref().unchecked_ref());
                                cb.forget();
                            }
                        }
                    }
                }
            }
        });
    });

    rsx! {
        ModuleFrame {
            show_screws: false,
            title: "HANGAR".to_string(),
            is_scrollable: true,

            { match state.as_ref() {
                        Some(_s) => rsx! {
                            // ── JINI Narrative (replaces NarrativeSummary) ────
                            JiniNarrative {
                                suggestion: jini_suggestion.read().clone(),
                                persona:    jini_persona,
                            }

                            // ── JINI Action Card (replaces RECOMMENDATION) ───
                            JiniActionCard {
                                suggestion: jini_suggestion.read().clone(),
                                mode,
                            }

                            ScoreBar {
                                wizard_findings,
                            }

                            if !wizard_findings.read().is_empty() {
                                div {
                                    style: "padding:0.5rem 1.5rem 0.25rem;
                                            color:var(--text-muted); font-size:0.6rem;
                                            letter-spacing:0.2em; text-transform:uppercase;",
                                    "FINDINGS  ({wizard_findings.read().len()})"
                                }
                            }
                            for finding in wizard_findings.read().iter() {
                                {
                                    let severity_str = match finding.severity {
                                        crate::wizard::WizardSeverity::High   => "high",
                                        crate::wizard::WizardSeverity::Medium => "medium",
                                        crate::wizard::WizardSeverity::Low    => "low",
                                    };
                                    let mfd_str = match finding.mfd {
                                        crate::wizard::MfdTarget::Mfd1SignalAnalyzer   => "MFD 1",
                                        crate::wizard::MfdTarget::Mfd2SpatialTelemetry => "MFD 2",
                                    };
                                    rsx! {
                                        div {
                                            class: "finding-row hud-severity-{severity_str}",
                                            style: "font-family: monospace; font-size: 10px; \
                                                    letter-spacing: 0.1em; text-transform: uppercase; \
                                                    padding: 6px 8px; border-left: 2px solid currentColor; \
                                                    display: flex; justify-content: space-between;",
                                            span { "⚠ {finding.id}" }
                                            span {
                                                style: "font-size: 9px; opacity: 0.6;",
                                                "{mfd_str}"
                                            }
                                        }
                                    }
                                }
                            }

                            if !_s.findings.issues.is_empty() {
                                // CoachActions removed per request
                            }
                        },
                        None => rsx! {
                            // JINI first-launch — inference-first onboarding
                            div {
                                style: "padding:1.5rem; display:flex; flex-direction:column; gap:1.5rem;",

                                // JINI greeting
                                div {
                                    style: "font-family:monospace; font-size:0.82rem; \
                                            color:var(--text-primary); line-height:1.8; font-style:italic;",
                                    "Drop your first file. I'll figure out the rest."
                                }

                                // Persona selector — always visible
                                JiniNarrative {
                                    suggestion: jini_suggestion.read().clone(),
                                    persona:    jini_persona,
                                }

                                // Subtle hint
                                div {
                                    style: "font-family:monospace; font-size:0.6rem; \
                                            color:var(--text-muted); letter-spacing:0.15em; \
                                            text-transform:uppercase;",
                                    "WAITING FOR INPUT"
                                }
                            }
                        }
            } }
        }
    }
}

// ── JINI Sub-components (J-P5) ─────────────────────────────────────────────────

#[component]
fn JiniNarrative(
    suggestion: Option<JiniSuggestionJson>,
    persona: Signal<JiniPersonaState>,
) -> Element {
    let current_persona = persona.read().clone();
    let mut jini_click_count = use_signal(|| 0u8);
    let mut show_persona_override = use_signal(|| false);
    rsx! {
        div {
            style: "padding:1rem 1.5rem; border-bottom:1px solid var(--border-subtle);",

            // Persona selector — always visible
            div {
                style: "display:flex; gap:8px; margin-bottom:0.75rem;",
                for (label, variant) in [
                    ("BEGINNER", JiniPersonaState::Beginner),
                    ("MID", JiniPersonaState::Intermediate),
                    ("PRO", JiniPersonaState::Pro),
                ] {
                    {
                        let is_active = current_persona == variant;
                        let variant_clone = variant.clone();
                        let mut persona_sig = persona;
                        rsx! {
                            div {
                                style: format!(
                                    "font-family:monospace; font-size:0.55rem; \
                                     letter-spacing:0.15em; cursor:pointer; \
                                     padding:2px 6px; border:1px solid {}; color:{};",
                                    if is_active { "var(--accent-cyan)" } else { "var(--border-subtle)" },
                                    if is_active { "var(--accent-cyan)" } else { "var(--text-muted)" }
                                ),
                                onclick: move |_| {
                                    persona_sig.set(variant_clone.clone());
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }

            // JINI narrative
            { match &suggestion {
                Some(s) if !s.narrative.is_empty() => rsx! {
                    div {
                        style: "color:var(--text-secondary); font-size:0.6rem; \
                                letter-spacing:0.15em; text-transform:uppercase; \
                                margin-bottom:0.5rem;",
                        "JINI"
                    }
                    div {
                        onclick: move |_| {
                            let count = *jini_click_count.read() + 1;
                            jini_click_count.set(count);
                            if count >= 3 {
                                jini_click_count.set(0);
                                show_persona_override.set(true);
                            }
                        },
                        style: "color:var(--text-primary); font-size:0.82rem; \
                                line-height:1.6; font-style:italic;",
                        "{s.narrative}"
                    }
                },
                _ => rsx! {
                    div {
                        style: "color:var(--text-muted); font-size:0.75rem; font-style:italic;",
                        "Analysing..."
                    }
                }
            } }

            if *show_persona_override.read() {
                div { class: "persona-override-modal",
                    button { onclick: move |_| {
                        show_persona_override.set(false);
                    }, "BEGINNER" }
                    button { onclick: move |_| {
                        show_persona_override.set(false);
                    }, "INTERMEDIATE" }
                    button { onclick: move |_| {
                        show_persona_override.set(false);
                    }, "PRO" }
                    button { onclick: move |_| {
                        show_persona_override.set(false);
                    }, "✕" }
                }
            }
        }
    }
}

#[component]
fn JiniActionCard(suggestion: Option<JiniSuggestionJson>, mode: Signal<CockpitMode>) -> Element {
    let Some(ref s) = suggestion else {
        return rsx! {};
    };
    if s.action_type == "nothing" || s.action_label.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            style: "margin: 0.5rem 1.5rem; padding: 0.75rem 1rem; \
                    border: 1px solid var(--border-subtle); \
                    background: rgba(0,255,65,0.03);",
            div {
                style: "color:var(--text-secondary); font-size:0.6rem; \
                        letter-spacing:0.15em; text-transform:uppercase; \
                        margin-bottom:0.5rem;",
                "SUGGESTION"
            }
            div {
                style: "color:var(--text-primary); font-size:0.78rem; \
                        margin-bottom:0.75rem;",
                "{s.action_label}"
            }
            div {
                style: "display:flex; gap:8px;",
                button {
                    style: "font-family:monospace; font-size:0.6rem; \
                            letter-spacing:0.15em; padding:4px 12px; \
                            background:transparent; border:1px solid var(--accent-cyan); \
                            color:var(--accent-cyan); cursor:pointer;",
                    onclick: move |_| {
                        dispatch(mode, CockpitEvent::JiniSuggestionAccepted);
                    },
                    "APPLY"
                }
                button {
                    style: "font-family:monospace; font-size:0.6rem; \
                            letter-spacing:0.15em; padding:4px 12px; \
                            background:transparent; border:1px solid var(--border-subtle); \
                            color:var(--text-muted); cursor:pointer;",
                    onclick: move |_| {
                        dispatch(mode, CockpitEvent::JiniSuggestionDismissed);
                    },
                    "DISMISS"
                }
            }
        }
    }
}

// ── Legacy Sub-components (preserved) ──────────────────────────────────────────

/// Static finding row for the FM0 demo state.
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

            // Label (fixed width)
            div { class: "finding-row-label", "{label}" }

            // Description (flex grow)
            div { class: "finding-row-description", "{desc}" }

            // Segmented OLED Mini-bar
            div { class: "finding-mini-bar-track",
                div {
                    class: "finding-mini-bar-fill {sev}",
                    style: "width:{pct}%;",
                }
            }

            // Percentage value
            div { class: "finding-row-pct", "{pct_label}" }

            // Severity dot
            div { class: "severity-dot {sev}" }
        }
    }
}

#[component]
fn ScoreBar(wizard_findings: ReadOnlySignal<Vec<crate::wizard::WizardFinding>>) -> Element {
    let has_high = wizard_findings
        .read()
        .iter()
        .any(|f| f.severity == crate::wizard::WizardSeverity::High);
    let has_medium = wizard_findings
        .read()
        .iter()
        .any(|f| f.severity == crate::wizard::WizardSeverity::Medium);

    let (label, color) = if !has_high && !has_medium {
        ("ALL CLEAR", "var(--accent-cyan)")
    } else if has_medium && !has_high {
        ("MINOR ISSUES", "var(--accent-amber)")
    } else {
        ("REVIEW NEEDED", "var(--severity-high)")
    };

    let issues = wizard_findings.read().len();

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
    let score_pct = if issue.target != 0.0 {
        let ratio = (issue.target - issue.delta.abs()) / issue.target.abs();
        (ratio * 100.0).clamp(0.0, 100.0)
    } else {
        50.0_f32
    };
    let pct_label = format!("{:.0}%", score_pct);
    let sev = issue.severity.as_str();

    rsx! {
        div {
            key:   "{issue.id}",
            class: "finding-row",

            div { class: "finding-row-label", "{issue.id}" }

            div {
                class: "finding-row-description",
                { format!("current {:.1} → target {:.1} (delta {:+.1})",
                          issue.current, issue.target, issue.delta) }
            }

            div { class: "finding-mini-bar-track",
                div {
                    class: "finding-mini-bar-fill {sev}",
                    style: "width:{score_pct}%;",
                }
            }

            div { class: "finding-row-pct", "{pct_label}" }
            div { class: "severity-dot {sev}" }
        }
    }
}

/// YES / NO coach action buttons (§4.10).
/// invoke("coachAction", { action: "yes" | "no" }) on click.
#[component]
#[allow(dead_code)]
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

#[allow(dead_code)]
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
