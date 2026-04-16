//! panels/session.rs — Session Panel (FM0→FM2) · P11-005
//! Authority: Phase 11 task-decomposition P11-005 · state-machine.md §2
//!
//! THE SESSION panel (left MFD):
//!   FM0: Drop zone + LOAD NEW button
//!   FM1: File info + preset selector
//!   FM1.5: File info + preset confirmed + MASTER button
//!   FM2: Mastering progress indicator
//!   FM5+: Golden Blob badge + EXPORT controls

use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::ipc::invoke;
use crate::state::cockpit_mode::{AscCode, CockpitMode};
use crate::types::{AudioMeta, SessionStateJson};

const PRESETS: &[(&str, &str)] = &[
    ("spotify",       "Spotify  −14 LUFS"),
    ("youtube",       "YouTube  −14 LUFS"),
    ("apple_music",   "Apple Music  −16 LUFS"),
    ("apple_podcast", "Apple Podcasts  −16 LUFS"),
    ("tidal",         "Tidal  −14 LUFS"),
    ("broadcast",     "Broadcast  −23 LUFS"),
    ("amazon",        "Amazon Music  −14 LUFS"),
];

#[component]
pub fn SessionPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    rsx! {
        div {
            class: "mfd-panel panel-session",
            style: "border-right:1px solid var(--border-subtle); display:flex; flex-direction:column; overflow:hidden;",

            // Panel title bar
            div {
                class: "panel-title",
                style: "color:var(--accent-session);
                        border-bottom:2px solid var(--accent-session);
                        padding:1rem 1.5rem 0.5rem;
                        font-size:0.7rem; letter-spacing:0.2em;
                        text-transform:uppercase; font-weight:600;
                        flex-shrink:0;",
                "THE SESSION"
            }

            // Panel body — mode-dependent
            div {
                style: "flex:1; overflow-y:auto; padding:0;",
                match mode.read().clone() {
                    CockpitMode::Idle => rsx! {
                        DropZone { mode }
                    },
                    CockpitMode::FileLoaded { name, format, path } => rsx! {
                        FileInfo { name: name.clone(), format: format.clone() }
                        PresetMenu { mode, path, name }
                    },
                    CockpitMode::PresetSelected { path, name, preset_id } => rsx! {
                        FileInfo { name: name.clone(), format: String::new() }
                        SelectedPreset { preset_id: preset_id.clone() }
                        MasterButton { mode, session_state, path, name, preset_id }
                    },
                    CockpitMode::Mastering { .. } => rsx! {
                        MasteringProgress {}
                    },
                    CockpitMode::CoachReady { blob_id } | CockpitMode::Exporting { blob_id, .. } => rsx! {
                        GoldenBlobBadge {}
                        ExportControls { mode, blob_id }
                    },
                    CockpitMode::Fault { code, message } => rsx! {
                        FaultView { code, message }
                    },
                }
            }
        }
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────────

#[component]
fn DropZone(mode: Signal<CockpitMode>) -> Element {
    let on_load = move |_| {
        web_sys::console::log_1(&JsValue::from_str("[session] LOAD NEW clicked"));
        spawn_local(async move {
            match invoke::<Option<AudioMeta>, _>("open_audio_file", json!({})).await {
                Ok(Some(meta)) => {
                    mode.set(CockpitMode::FileLoaded {
                        path:   meta.path.clone(),
                        name:   meta.name.clone(),
                        format: meta.format.clone(),
                    });
                }
                Ok(None) => {} // user cancelled dialog
                Err(e) => mode.set(CockpitMode::Fault {
                    code:    AscCode::IoErr,
                    message: format!("File open failed: {e}"),
                }),
            }
        });
    };

    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center;
                    justify-content:center; height:100%; padding:2rem;
                    text-align:center;",

            // Drop zone visual
            div {
                style: "border:2px dashed var(--border-medium); border-radius:8px;
                        padding:3rem 2rem; width:100%; box-sizing:border-box;
                        color:var(--text-muted); font-size:0.85rem;
                        letter-spacing:0.05em; margin-bottom:1.5rem;",
                div { style: "font-size:2rem; margin-bottom:1rem; opacity:0.4;", "⬇" }
                "DROP AUDIO FILE"
                div { style: "font-size:0.7rem; margin-top:0.5rem; opacity:0.6;",
                    "WAV  ·  FLAC  ·  MP3  ·  AIFF"
                }
            }

            // Load New button
            button {
                id: "btn-load-new",
                onclick: on_load,
                style: "background:var(--accent-session); color:#fff;
                        border:none; border-radius:4px; padding:0.6rem 1.5rem;
                        font-size:0.75rem; letter-spacing:0.15em;
                        text-transform:uppercase; cursor:pointer;
                        font-weight:600; width:100%;
                        transition:opacity 0.15s ease;",
                "LOAD NEW"
            }
        }
    }
}

#[component]
fn FileInfo(name: String, format: String) -> Element {
    rsx! {
        div {
            style: "padding:1rem 1.5rem; border-bottom:1px solid var(--border-subtle);",
            div {
                style: "color:var(--text-secondary); font-size:0.65rem;
                        letter-spacing:0.15em; text-transform:uppercase;
                        margin-bottom:0.4rem;",
                "AUDIO SOURCE"
            }
            div {
                style: "color:var(--text-primary); font-size:0.9rem;
                        font-weight:500; word-break:break-all;",
                "{name}"
            }
            if !format.is_empty() {
                div {
                    style: "color:var(--text-muted); font-size:0.7rem;
                            margin-top:0.25rem; text-transform:uppercase;",
                    "{format}"
                }
            }
        }
    }
}

#[component]
fn PresetMenu(mode: Signal<CockpitMode>, path: String, name: String) -> Element {
    rsx! {
        div {
            style: "padding:1rem 1.5rem;",
            div {
                style: "color:var(--text-secondary); font-size:0.65rem;
                        letter-spacing:0.15em; text-transform:uppercase;
                        margin-bottom:0.75rem;",
                "SELECT TARGET PLATFORM"
            }
            div {
                style: "display:flex; flex-direction:column; gap:0.35rem;",
                for (preset_id, label) in PRESETS {
                    {
                        let pid  = preset_id.to_string();
                        let lbl  = label.to_string();
                        let p    = path.clone();
                        let n    = name.clone();
                        rsx! {
                            button {
                                key: "{pid}",
                                id:  "preset-{pid}",
                                onclick: move |_| {
                                    mode.set(CockpitMode::PresetSelected {
                                        path:      p.clone(),
                                        name:      n.clone(),
                                        preset_id: pid.clone(),
                                    });
                                },
                                style: "background:var(--surface-panel);
                                        color:var(--text-primary);
                                        border:1px solid var(--border-subtle);
                                        border-radius:4px; padding:0.5rem 0.75rem;
                                        font-size:0.75rem; cursor:pointer;
                                        text-align:left; transition:border-color 0.15s;",
                                "{lbl}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SelectedPreset(preset_id: String) -> Element {
    let label = PRESETS.iter()
        .find(|(id, _)| *id == preset_id.as_str())
        .map(|(_, l)| *l)
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
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    path:          String,
    name:          String,
    preset_id:     String,
) -> Element {
    let on_master = move |_| {
        web_sys::console::log_1(&JsValue::from_str("[session] MASTER CLICKED"));
        let mode_val = format!("{:?}", mode.read().clone());
        web_sys::console::log_1(&JsValue::from_str(
            &format!("[session] current mode: {}", mode_val)
        ));

        let p  = path.clone();
        let pr = preset_id.clone();

        // Use spawn_local — the correct WASM async primitive.
        // Dioxus spawn() may not drive JsFuture correctly in web target.
        spawn_local(async move {
            web_sys::console::log_1(&JsValue::from_str("[session] spawn_local started"));
            mode.set(CockpitMode::Mastering {
                path:      p.clone(),
                preset_id: pr.clone(),
            });

            web_sys::console::log_1(&JsValue::from_str("[session] calling trigger_mastering..."));
            let blob_id = match invoke::<String, _>(
                "trigger_mastering",
                json!({ "audioPath": p, "presetId": pr }),
            ).await {
                Ok(id)  => id,
                Err(e)  => {
                    web_sys::console::log_1(&JsValue::from_str(
                        &format!("[session] trigger_mastering FAILED: {e}")
                    ));
                    mode.set(CockpitMode::Fault {
                        code:    AscCode::IoErr,
                        message: format!("Mastering failed: {e}"),
                    });
                    return;
                }
            };
            web_sys::console::log_1(&JsValue::from_str(
                &format!("[session] mastering done, blob_id={blob_id}")
            ));

            // Single IPC call: all session data in one shot (P9-008)
            web_sys::console::log_1(&JsValue::from_str("[session] calling get_session_state..."));
            let state = match invoke::<crate::types::SessionStateJson, _>(
                "get_session_state",
                json!({ "blobId": blob_id }),
            ).await {
                Ok(s)   => s,
                Err(e)  => {
                    web_sys::console::log_1(&JsValue::from_str(
                        &format!("[session] get_session_state FAILED: {e}")
                    ));
                    mode.set(CockpitMode::Fault {
                        code:    AscCode::IoErr,
                        message: format!("Session state failed: {e}"),
                    });
                    return;
                }
            };
            web_sys::console::log_1(&JsValue::from_str("[session] session state ok, transitioning to FM5"));

            // FM5
            session_state.set(Some(state));
            mode.set(CockpitMode::CoachReady { blob_id });
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
fn MasteringProgress() -> Element {
    rsx! {
        div {
            style: "display:flex; flex-direction:column; align-items:center;
                    justify-content:center; height:100%; padding:2rem;",
            div {
                style: "color:var(--accent-master); font-size:0.75rem;
                        letter-spacing:0.2em; text-transform:uppercase;
                        font-weight:600; margin-bottom:1.5rem;",
                "MASTERING IN PROGRESS"
            }
            // Animated progress bar
            div {
                style: "width:100%; height:3px; background:var(--border-subtle);
                        border-radius:2px; overflow:hidden;",
                div {
                    style: "height:100%; background:var(--accent-master);
                            width:60%; border-radius:2px;
                            animation:pulse 1.5s ease-in-out infinite;",
                }
            }
            div {
                style: "color:var(--text-muted); font-size:0.65rem;
                        margin-top:1rem; letter-spacing:0.1em;",
                "sp314-dsp · 8-stage pipeline"
            }
        }
    }
}

#[component]
fn GoldenBlobBadge() -> Element {
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
fn ExportControls(mode: Signal<CockpitMode>, blob_id: String) -> Element {
    let mut export_format = use_signal(|| "flac".to_string());

    let on_export = {
        let bid = blob_id.clone();
        move |_| {
            let b   = bid.clone();
            let fmt = export_format.read().clone();
            spawn_local(async move {
                mode.set(CockpitMode::Exporting {
                    blob_id: b.clone(),
                    format:  fmt.clone(),
                });

                match invoke::<crate::types::ExportResult, _>(
                    "export_audio",
                    json!({ "blobId": b, "format": fmt }),
                ).await {
                    Ok(r) => {
                        eprintln!("[export] written: {} ({})", r.written_path, r.format);
                        mode.set(CockpitMode::CoachReady { blob_id: b });
                    }
                    Err(e) if e.contains("cancelled") => {
                        mode.set(CockpitMode::CoachReady { blob_id: b });
                    }
                    Err(e) => {
                        mode.set(CockpitMode::Fault {
                            code:    AscCode::IoErr,
                            message: format!("Export failed: {e}"),
                        });
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

            // Format selector
            div {
                style: "display:flex; gap:0.4rem; margin-bottom:1rem;",
                for fmt in ["flac", "wav"] {
                    {
                        let f   = fmt.to_string();
                        let sel = export_format.read().clone() == fmt;
                        rsx! {
                            button {
                                key: "{f}",
                                id:  "fmt-{f}",
                                onclick: move |_| export_format.set(f.clone()),
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
                                "{fmt}"
                            }
                        }
                    }
                }
            }

            button {
                id: "btn-export",
                onclick: on_export,
                style: "background:var(--accent-insights); color:#fff;
                        border:none; border-radius:4px; padding:0.6rem 1.5rem;
                        font-size:0.75rem; letter-spacing:0.15em;
                        text-transform:uppercase; cursor:pointer;
                        font-weight:600; width:100%;",
                "EXPORT"
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
