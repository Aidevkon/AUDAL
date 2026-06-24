//! panels/session.rs — Session Panel (FM0→FM2) · P11-005
//! Authority: Phase 11 task-decomposition P11-005 · state-machine.md §2
//!
//! THE SESSION panel (left MFD):
//!   FM0: Drop zone + LOAD NEW button
//!   FM1: File info + preset selector
//!   FM1.5: File info + preset confirmed + MASTER button
//!   FM2: Mastering progress indicator
//!   FM5+: Golden Blob badge + EXPORT controls

use crate::components::module_frame::ModuleFrame;
use crate::components::primary_signal_analyzer::PrimarySignalAnalyzer;
use dioxus::prelude::*;

use serde_json::json;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::ipc::invoke;
use crate::state::cockpit_event::CockpitEvent;
use crate::state::cockpit_mode::{AscCode, CockpitMode};
use crate::state::presets::PLATFORMS;
use crate::state::reducer::dispatch;
use crate::types::{SessionStateJson, VisualizationDataJson};

#[component]
pub fn SessionPanel(
    mode: Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    viz_data: Signal<Option<VisualizationDataJson>>,
    show_mastered: Signal<bool>,
    mut wizard_findings: Signal<Vec<crate::wizard::WizardFinding>>,
    tone_angle: Signal<f32>,
    dyn_angle: Signal<f32>,
    space_angle: Signal<f32>,
    loud_angle: Signal<f32>,
    jini_persona: Signal<crate::types::JiniPersonaState>,
    journey_stage: Signal<String>,
) -> Element {
    rsx! {
    ModuleFrame {
        title: "PRIMARY SIGNAL ANALYZER".to_string(),
        panel_class: "panel-session".to_string(),
        header_style: "color:var(--accent-session); border-bottom:2px solid var(--accent-session);".to_string(),
        is_scrollable: true,

        { match mode.read().clone() {
                    CockpitMode::Idle => {
                        rsx! {
                            PrimarySignalAnalyzer {
                                filename: "NO FILE LOADED".to_string(),
                                format: "---".to_string(),
                                telemetry: None,
                                bpm: 0.0,
                                journey_stage,
                            }
                        }
                    },
                    CockpitMode::FileLoaded { name, format, path: _ } => {
                        rsx! {
                            PrimarySignalAnalyzer {
                                filename: name.clone(),
                                format: format.clone(),
                                telemetry: None,
                                bpm: 0.0,
                                journey_stage,
                            }
                        }
                    },
                    CockpitMode::PresetSelected { path, name, preset_id } => {
                        rsx! {
                            PrimarySignalAnalyzer {
                                filename: name.clone(),
                                format: String::new(),
                                telemetry: None,
                                bpm: 0.0,
                                journey_stage,
                            }
                            SelectedPreset { preset_id: preset_id.clone() }
                            MasterButton {
                                mode, session_state, viz_data, path, name, preset_id, wizard_findings,
                                tone_angle, dyn_angle, space_angle, loud_angle, jini_persona
                            }
                        }
                    },
                    CockpitMode::Mastering { path: _, preset_id } => {
                        let name = preset_id.clone();
                        rsx! {
                            PrimarySignalAnalyzer {
                                filename: name,
                                format: String::new(),
                                telemetry: None,
                                bpm: 0.0,
                                journey_stage,
                            }
                        }
                    },
                    CockpitMode::CoachReady { .. } | CockpitMode::Exporting { .. } => rsx! {
                        PrimarySignalAnalyzer {
                            filename: "CERTIFIED MASTER".to_string(), // Keep consistent with previous logic or use empty string
                            format: String::new(),
                            telemetry: None,
                            bpm: 0.0,
                            journey_stage,
                        }
                    },
                    CockpitMode::Fault { code, message } => rsx! {
                        FaultView { code, message }
                    },
                }
            }
        }
    }
}

#[component]
fn SelectedPreset(preset_id: String) -> Element {
    let label = PLATFORMS
        .iter()
        .find(|p| p.id == preset_id.as_str())
        .map(|p| p.label)
        .unwrap_or(&preset_id);
    rsx! {
        div {
            style: "padding:0.75rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
            div {
                style: "color:var(--text-secondary); font-size:0.65rem;
                        letter-spacing:0.15em; text-transform:uppercase;
                        margin-bottom:0.25rem;",
                "TARGET PLATFORM"
            }
            div {
                style: "color:var(--accent-session); font-size:0.85rem; font-weight:600;",
                "{label}"
            }
        }
    }
}

#[component]
fn MasterButton(
    mode: Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    viz_data: Signal<Option<VisualizationDataJson>>,
    path: String,
    name: String,
    preset_id: String,
    mut wizard_findings: Signal<Vec<crate::wizard::WizardFinding>>,
    tone_angle: Signal<f32>,
    dyn_angle: Signal<f32>,
    space_angle: Signal<f32>,
    loud_angle: Signal<f32>,
    jini_persona: Signal<crate::types::JiniPersonaState>,
) -> Element {
    let on_master = move |_| {
        #[cfg(debug_assertions)]
        web_sys::console::log_1(&JsValue::from_str("[session] MASTER CLICKED"));
        let mode_val = format!("{:?}", mode.read().clone());
        #[cfg(debug_assertions)]
        web_sys::console::log_1(&JsValue::from_str(&format!(
            "[session] current mode: {}",
            mode_val
        )));

        let p = path.clone();
        let pr = preset_id.clone();

        // Use spawn_local — the correct WASM async primitive.
        // Dioxus spawn() may not drive JsFuture correctly in web target.
        spawn_local(async move {
            #[cfg(debug_assertions)]
            web_sys::console::log_1(&JsValue::from_str("[session] spawn_local started"));
            dispatch(mode, CockpitEvent::MasterTriggered);

            #[cfg(debug_assertions)]
            web_sys::console::log_1(&JsValue::from_str("[session] calling trigger_mastering..."));
            let blob_id = match invoke::<String, _>(
                "trigger_mastering",
                json!({
                    "audioPath":      p,
                    "presetId":       pr,
                    "intentTone":     ((*tone_angle.read()  / 135.0) + 1.0) / 2.0,
                    "intentDynamics": ((*dyn_angle.read()   / 135.0) + 1.0) / 2.0,
                }),
            )
            .await
            {
                Ok(id) => id,
                Err(e) => {
                    #[cfg(debug_assertions)]
                    web_sys::console::log_1(&JsValue::from_str(&format!(
                        "[session] trigger_mastering FAILED: {e}"
                    )));
                    dispatch(
                        mode,
                        CockpitEvent::MasteringFailed {
                            message: format!("Mastering failed: {e}"),
                        },
                    );
                    return;
                }
            };
            #[cfg(debug_assertions)]
            web_sys::console::log_1(&JsValue::from_str(&format!(
                "[session] mastering done, blob_id={blob_id}"
            )));

            // Single IPC call: all session data in one shot (P9-008)
            #[cfg(debug_assertions)]
            web_sys::console::log_1(&JsValue::from_str("[session] calling get_session_state..."));

            let persona_str = match *jini_persona.read() {
                crate::types::JiniPersonaState::Beginner => "beginner",
                crate::types::JiniPersonaState::Intermediate => "intermediate",
                crate::types::JiniPersonaState::Pro => "pro",
            };

            let state = match invoke::<crate::types::SessionStateJson, _>(
                "get_session_state",
                json!({ "blobId": blob_id, "persona": persona_str }),
            )
            .await
            {
                Ok(s) => s,
                Err(e) => {
                    #[cfg(debug_assertions)]
                    web_sys::console::log_1(&JsValue::from_str(&format!(
                        "[session] get_session_state FAILED: {e}"
                    )));
                    dispatch(
                        mode,
                        CockpitEvent::MasteringFailed {
                            message: format!("Session state failed: {e}"),
                        },
                    );
                    return;
                }
            };
            #[cfg(debug_assertions)]
            web_sys::console::log_1(&JsValue::from_str(
                "[session] session state ok, transitioning to FM5",
            ));

            // P14-003: Fetch viz immediately after session (UI Agent Context §3)
            let bid2 = blob_id.clone();
            if let Ok(viz) = invoke::<VisualizationDataJson, _>(
                "get_visualization_data",
                json!({ "blobId": bid2 }),
            )
            .await
            {
                viz_data.set(Some(viz));
            }

            let findings = crate::wizard::detect_findings(&state);
            wizard_findings.set(findings);

            // FM5: set session state + fetch visualization data
            session_state.set(Some(state));

            dispatch(mode, CockpitEvent::MasteringComplete { blob_id });
        });
    };

    rsx! {
        div {
            style: "padding:1rem 1.5rem;",
            button {
                id: "btn-master",
                onclick: on_master,
                style: "background:var(--accent-master); color:#fff;
                        border:none; border-radius:4px; padding:0.75rem 1.5rem;
                        font-size:0.75rem; letter-spacing:0.2em;
                        text-transform:uppercase; cursor:pointer;
                        font-weight:700; width:100%;
                        box-shadow:0 0 12px rgba(255,107,53,0.3);
                        transition:box-shadow 0.2s ease;",
                "▶  MASTER"
            }
        }
    }
}

#[component]
pub fn GoldenBlobBadge() -> Element {
    rsx! {
        div {
            style: "padding:1rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
            div {
                style: "display:flex; align-items:center; gap:0.5rem;",
                div {
                    style: "width:8px; height:8px; border-radius:50%;
                            background:var(--status-ok); flex-shrink:0;
                            box-shadow:0 0 6px var(--status-ok);",
                }
                div {
                    style: "color:var(--text-secondary); font-size:0.65rem;
                            letter-spacing:0.15em; text-transform:uppercase;",
                    "GOLDEN BLOB READY"
                }
            }
        }
    }
}

#[component]
pub fn ExportControls(mode: Signal<CockpitMode>, blob_id: String) -> Element {
    let mut export_format = use_signal(|| "flac".to_string());
    let mut pdf_preview_ctx = use_context::<Signal<Option<String>>>();

    let on_export = {
        let bid = blob_id.clone();
        move |_| {
            let b = bid.clone();
            let fmt = export_format.read().clone();
            spawn_local(async move {
                dispatch(
                    mode,
                    CockpitEvent::ExportTriggered {
                        format: fmt.clone(),
                    },
                );

                match invoke::<crate::types::ExportResult, _>(
                    "export_audio",
                    json!({ "blobId": b, "format": fmt }),
                )
                .await
                {
                    Ok(r) => {
                        eprintln!("[export] written: {} ({})", r.written_path, r.format);
                        dispatch(mode, CockpitEvent::ExportComplete);
                    }
                    Err(e) if e.contains("cancelled") => {
                        dispatch(mode, CockpitEvent::ExportComplete);
                    }
                    Err(e) => {
                        dispatch(
                            mode,
                            CockpitEvent::ExportFailed {
                                message: format!("Export failed: {e}"),
                            },
                        );
                    }
                }
            });
        }
    };

    rsx! {
        div {
            style: "padding:1rem 1.5rem;",

            div {
                style: "color:var(--text-secondary); font-size:0.65rem;
                        letter-spacing:0.15em; text-transform:uppercase;
                        margin-bottom:0.5rem;",
                "EXPORT FORMAT"
            }

            // Format selector — FLAC / WAV / MP3 / AIFF (Phase 13A + 13-003b)
            div {
                style: "display:flex; gap:0.4rem; margin-bottom:1rem; flex-wrap:wrap;",
                for fmt in ["flac", "wav", "mp3", "aiff"] {
                    {
                        let f   = fmt.to_string();
                        let f2  = f.clone();
                        let sel = export_format.read().clone() == fmt;
                        rsx! {
                            button {
                                key: "{f}",
                                id:  "fmt-{f}",
                                onclick: move |_| export_format.set(f2.clone()),
                                style: if sel {
                                    "background:var(--accent-session); color:#fff;
                                     border:none; border-radius:3px; padding:0.35rem 0.75rem;
                                     font-size:0.7rem; letter-spacing:0.1em; cursor:pointer;
                                     text-transform:uppercase; font-weight:600;"
                                } else {
                                    "background:var(--surface-panel); color:var(--text-muted);
                                     border:1px solid var(--border-subtle); border-radius:3px;
                                     padding:0.35rem 0.75rem; font-size:0.7rem;
                                     letter-spacing:0.1em; cursor:pointer;
                                     text-transform:uppercase;"
                                },
                                "{fmt.to_uppercase()}"
                            }
                        }
                    }
                }
            }

            // Export audio button
            button {
                id: "btn-export",
                onclick: on_export,
                style: "background:var(--accent-insights); color:#fff;
                        border:none; border-radius:4px; padding:0.6rem 1.5rem;
                        font-size:0.75rem; letter-spacing:0.15em;
                        text-transform:uppercase; cursor:pointer;
                        font-weight:600; width:100%; margin-bottom:0.5rem;",
                "EXPORT"
            }

            // PDF Report button (Phase 13B: BMR-128 PDF)
            {
                let _bid = blob_id.clone();
                rsx! {
                    button {
                        id: "btn-pdf-report",
                        onclick: move |_| {
                            pdf_preview_ctx.set(Some(blob_id.clone()));
                        },
                        style: "background:var(--surface-panel); color:var(--text-muted);
                                border:1px solid var(--border-subtle); border-radius:4px;
                                padding:0.5rem 1.5rem; font-size:0.7rem;
                                letter-spacing:0.15em; text-transform:uppercase;
                                cursor:pointer; width:100%;",
                        "PDF REPORT"
                    }
                }
            }
        }
    }
}

#[component]
fn FaultView(code: AscCode, message: String) -> Element {
    rsx! {
        div {
            style: "padding:1.5rem; display:flex; flex-direction:column;
                    align-items:center; gap:1rem;",
            div {
                style: "color:var(--status-err); font-size:0.75rem;
                        letter-spacing:0.2em; text-transform:uppercase;
                        font-weight:700;",
                "FM-ERR  ·  {code.as_str()}"
            }
            div {
                style: "color:var(--text-muted); font-size:0.75rem;
                        text-align:center;",
                "{message}"
            }
        }
    }
}
