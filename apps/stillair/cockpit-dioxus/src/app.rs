//! app.rs — Cockpit root component. Phase 12B transport bar.
//! Authority: Phase 12B task-decomposition · P12B-003 · P12B-004
//!            state-machine.md §2 · Amendment A-003 §5
//!
//! Layout:
//!   ┌───────────────────────────────────────────────────────┐
//!   │  HEADER: STILL AIR wordmark + mode badge               │  44px
//!   ├───────────────┬───────────────┬───────────────────────┤
//!   │  SESSION      │   INSIGHTS    │   COACH               │  flex:1
//!   │  (left MFD)   │  (center MFD) │  (right MFD)          │
//!   ├───────────────┴───────────────┴───────────────────────┤
//!   │  [00:00]  [◄◄-5s] [▶PLAY▐▐] [+5s►]  [●SCRUB●]  [04:32] │  64px
//!   └───────────────────────────────────────────────────────┘
//!
//! Amendment A-002 §2: no business logic here — IPC only.
//! Amendment A-002 §3: no core imports.
//! Amendment A-003 §5: no PCM — PlaybackStateJson only.


use crate::ipc::invoke;
use crate::components::module_frame::ModuleFrame;
use crate::components::sampling_siamese::SamplingSiamese;
use crate::components::transport_bar::TransportBar;
use crate::components::jini::drop_zone::JiniDropZone;
use crate::components::jini::detection::JiniDetection;
use crate::components::jini::platform_selector::JiniPlatformSelector;
use crate::components::jini::flavour_selector::JiniFlavourSelector;
use crate::components::jini::analysing::JiniAnalysing;
use crate::components::jini::awaiting_more::JiniAwaitingMore;
use crate::panels::{jini_panel::JiniPanel, mastered::MasteredView};
use crate::state::cockpit_mode::CockpitMode;

use crate::state::hangar_interview::HangarInterviewState;
use crate::state::hangar_reducer::dispatch_hangar;
use crate::state::hangar_event::HangarEvent;
use crate::state::reducer::dispatch;
use crate::state::cockpit_event::CockpitEvent;
use crate::types::{
    CockpitTier, JiniPersonaState, JiniSuggestionJson, SessionStateJson, VisualizationDataJson,
};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

// ── App root ──────────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
pub fn App() -> Element {
    // ── Signals ──────────────────────────────────────────────────────────────
    let mode = use_signal(|| CockpitMode::Idle);
    let session_state = use_signal(|| None::<SessionStateJson>);
    let tier = use_signal(|| CockpitTier::Tier3_Pro);
    let presentation =
        crate::state::cockpit_presentation::CockpitPresentation::from_tier(*tier.read());
    let wizard_findings = use_signal(Vec::<crate::wizard::WizardFinding>::new);
    let viz_data: Signal<Option<VisualizationDataJson>> = use_signal(|| None);
    // MasteredView overlay visibility (Signal only — no IPC per §5.3)
    let mut show_mastered: Signal<bool> = use_signal(|| false);
    let mut pdf_preview_ctx = use_context_provider(|| Signal::new(None::<String>));
    let mut intent_open: Signal<bool> = use_signal(|| false);
    let mut intent_closing: Signal<bool> = use_signal(|| false);
    let tone_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let dyn_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let space_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let loud_angle: Signal<f32> = use_signal(|| 0.0_f32);
    // ── JINI signals (J-P5) ──────────────────────────────────────────────────
    let jini_suggestion: Signal<Option<JiniSuggestionJson>> = use_signal(|| None);
    let jini_persona: Signal<JiniPersonaState> = use_signal(|| JiniPersonaState::Intermediate);
    let mut is_journey_active: Signal<bool> = use_signal(|| false);
    let mut journey_stage: Signal<String> = use_signal(|| "INITIALIZING".to_string());
    let mut stage_queue: Signal<std::collections::VecDeque<String>> =
        use_signal(|| std::collections::VecDeque::new());
    let mut journey_elapsed_ms: Signal<u64> = use_signal(|| 0);
    let mut bpm_signal: Signal<f32> = use_signal(|| 0.0);
    let mut dropped_path: Signal<Option<String>> = use_signal(|| None);
    let last_platform: Signal<Option<String>> = use_signal(|| None);
    let last_flavour: Signal<Option<String>>  = use_signal(|| None);
    let mut pending_master_req: Signal<Option<(String, String, String, f32, f32)>> = use_signal(|| None);
    // TODO: When file drop is implemented (OB-P2 full),
    // wire Ignition → CockpitMode::FileLoaded { path, name, format }
    // and Analysing → drive AnalysisStage LEDs in Left MFD
    let hangar_state = use_signal(|| HangarInterviewState::AwaitingDrop);

    // ── Mastering Orchestration Task (Root Scope) ─────────────────────────────
    // Dioxus false-positive warning notice: this effect both reads and writes
    // pending_master_req in the same reactive scope (read() on entry, write().take()
    // to consume the one-shot request). This triggers Dioxus's "infinite loop"
    // warning, but is NOT an actual loop — the if let Some(...) guard means the
    // effect's second, self-triggered run sees None and exits immediately without
    // writing again. Verified harmless: each UI trigger produces exactly one
    // /master POST (no duplicate/runaway requests observed in testing).
    // Investigated and confirmed 2026-06-24, see commit history for analysis.
    // Deliberately left as-is rather than restructured, to avoid introducing a
    // behavior change (e.g. moving the clear into an async task) purely to
    // silence a cosmetic warning.
    use_effect(move || {
        let req = pending_master_req.read().clone();
        if let Some((path, pr, fl, tone, dynval)) = req {
            pending_master_req.write().take(); // clear before spawn — prevents double-fire
            
            let mut m_mode = mode;
            let mut hs = hangar_state;
            let mut lp = last_platform;
            let mut lf = last_flavour;
            let mut viz_data_sig = viz_data;
            let mut session_state_sig = session_state;
            let mut wizard_findings_sig = wizard_findings;
            let jini_persona_sig = jini_persona;
            let mut is_journey_active_sig = is_journey_active;
            
            spawn_local(async move {
                lp.set(Some(pr.clone()));
                lf.set(Some(fl.clone()));

                web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                    "[NEW-SESSION] queue_len={}, journey_active={}, mode={:?}",
                    stage_queue.read().len(),
                    *is_journey_active.read(),
                    *m_mode.read()
                )));

                // Step 1: load file metadata
                let meta = match crate::ipc::invoke::<crate::types::AudioMeta, _>(
                    "load_audio_file",
                    serde_json::json!({ "path": path.clone() })
                ).await {
                    Ok(m) => m,
                    Err(e) => {
                        dispatch(m_mode, CockpitEvent::MasteringFailed {
                            message: format!("Load failed: {e}")
                        });
                        dispatch_hangar(hs, HangarEvent::Reset);
                        return;
                    }
                };
                if !matches!(*m_mode.read(), CockpitMode::Idle) {
                    dispatch(m_mode, CockpitEvent::BackToIdle);
                }
                is_journey_active_sig.set(false);
                let mut stage_queue_guard = stage_queue.write();
                stage_queue_guard.clear();
                drop(stage_queue_guard);
                let mut journey_stage_sig = journey_stage;
                journey_stage_sig.set("INITIALIZING".into());

                dispatch(m_mode, CockpitEvent::FileDropped {
                    path: path.clone(),
                    name: meta.name.clone(),
                    format: meta.format.clone(),
                });
                dispatch(m_mode, CockpitEvent::PresetSelected {
                    preset_id: pr.clone(),
                });
                dispatch(m_mode, CockpitEvent::MasterTriggered);
                is_journey_active_sig.set(true);
                dispatch_hangar(hs, HangarEvent::AnalysisStarted);

                // Step 2: trigger_mastering
                let blob_id = match crate::ipc::invoke::<String, _>(
                    "trigger_mastering",
                    serde_json::json!({
                        "audioPath":      path.clone(),
                        "presetId":       pr,
                        "flavourId":      fl,
                        "intentTone":     tone,
                        "intentDynamics": dynval,
                    })
                ).await {
                    Ok(id) => id,
                    Err(e) => {
                        dispatch(m_mode, CockpitEvent::MasteringFailed {
                            message: format!("Mastering failed: {e}")
                        });
                        dispatch_hangar(hs, HangarEvent::Reset);
                        return;
                    }
                };

                // Step 3: get_session_state
                let persona_str = match *jini_persona_sig.read() {
                    crate::types::JiniPersonaState::Beginner => "beginner",
                    crate::types::JiniPersonaState::Intermediate => "intermediate",
                    crate::types::JiniPersonaState::Pro => "pro",
                };
                let state = match crate::ipc::invoke::<crate::types::SessionStateJson, _>(
                    "get_session_state",
                    serde_json::json!({ "blobId": blob_id.clone(), "persona": persona_str, "flavour": fl.clone(), "platform": pr.clone() })
                ).await {
                    Ok(s) => s,
                    Err(e) => {
                        dispatch(m_mode, CockpitEvent::MasteringFailed {
                            message: format!("Session state failed: {e}")
                        });
                        dispatch_hangar(hs, HangarEvent::Reset);
                        return;
                    }
                };

                // Step 4: get_visualization_data
                if let Ok(viz) = crate::ipc::invoke::<crate::types::VisualizationDataJson, _>(
                    "get_visualization_data",
                    serde_json::json!({ "blobId": blob_id.clone() })
                ).await {
                    viz_data_sig.set(Some(viz));
                }

                // Step 5: wizard findings
                let findings = crate::wizard::detect_findings(&state);
                wizard_findings_sig.set(findings);

                // Step 6: session state
                session_state_sig.set(Some(state));

                // wait for visual queue to drain
                web_sys::console::error_1(&format!("[TRAP] ENTERING DRAIN LOOP. is_journey_active={}", *is_journey_active_sig.read()).into());
                
                loop {
                    let empty = stage_queue.read().is_empty();
                    let active = *is_journey_active_sig.read();
                    
                    web_sys::console::error_1(&format!(
                        "[TRAP] WAIT LOOP PING: queue_empty={}, journey_active={}", 
                        empty, active
                    ).into());

                    if empty && !active {
                        web_sys::console::error_1(&format!("[TRAP] LOOP BREAK CONDITION MET!").into());
                        break;
                    }
                    gloo_timers::future::TimeoutFuture::new(500).await;
                }

                // Step 7: complete
                // Step 7: complete
                web_sys::console::log_1(&format!(
                    "[FSM-TRAP] queue drained, journey_active={}, mode={:?}",
                    *is_journey_active_sig.read(), *m_mode.read()
                ).into());

                dispatch(m_mode, CockpitEvent::MasteringComplete { blob_id: blob_id.clone() });

                web_sys::console::log_1(&format!(
                    "[FSM-TRAP] after MasteringComplete dispatch, mode={:?}",
                    *m_mode.read()
                ).into());

                dispatch_hangar(hs, HangarEvent::AnalysisComplete);

                web_sys::console::error_1(&format!(
                    "[TRAP] FSM: after AnalysisComplete. hangar should be Ready now"
                ).into());
            });
        }
    });


    // ── Tauri Event Listener for mastering://progress ────────────────────────
    use_effect(move || {
        spawn_local(async move {
            let window = web_sys::window().expect("no window");
            let window_val: JsValue = window.into();

            // Try to get window.__TAURI__.event.listen
            if let Ok(tauri) = js_sys::Reflect::get(&window_val, &JsValue::from_str("__TAURI__")) {
                if !tauri.is_undefined() {
                    if let Ok(event_api) = js_sys::Reflect::get(&tauri, &JsValue::from_str("event"))
                    {
                        if let Ok(listen_val) =
                            js_sys::Reflect::get(&event_api, &JsValue::from_str("listen"))
                        {
                            if let Ok(listen_fn) = listen_val.dyn_into::<js_sys::Function>() {
                                let cb = wasm_bindgen::closure::Closure::wrap(Box::new(
                                    move |ev: JsValue| {
                                        if let Ok(payload) =
                                            js_sys::Reflect::get(&ev, &JsValue::from_str("payload"))
                                        {
                                            if let Ok(stage_val) = js_sys::Reflect::get(
                                                &payload,
                                                &JsValue::from_str("stage"),
                                            ) {
                                                if let Some(s) = stage_val.as_string() {
                                                    if s == "DISPATCHED" { return; }
                                                    let mut q = stage_queue.write();
                                                    if q.back() != Some(&s) {
                                                        q.push_back(s.clone());
                                                    }
                                                }
                                            }
                                            if let Ok(elapsed_val) = js_sys::Reflect::get(
                                                &payload,
                                                &JsValue::from_str("elapsed_ms"),
                                            ) {
                                                if let Some(ms) = elapsed_val.as_f64() {
                                                    journey_elapsed_ms.set(ms as u64);
                                                }
                                            }
                                        }
                                    },
                                )
                                    as Box<dyn FnMut(JsValue)>);

                                let _ = listen_fn.call2(
                                    &event_api,
                                    &JsValue::from_str("mastering://progress"),
                                    cb.as_ref().unchecked_ref(),
                                );
                                cb.forget();

                                let cb_album = wasm_bindgen::closure::Closure::wrap(Box::new(
                                    move |ev: JsValue| {
                                        if let Ok(payload) =
                                            js_sys::Reflect::get(&ev, &JsValue::from_str("payload"))
                                        {
                                            if let Ok(bpm_val) = js_sys::Reflect::get(
                                                &payload,
                                                &JsValue::from_str("bpm"),
                                            ) {
                                                if let Some(b) = bpm_val.as_f64() {
                                                    bpm_signal.set(b as f32);
                                                    web_sys::console::log_1(&JsValue::from_str(&format!("BPM: {}", b)));
                                                }
                                            }
                                        }
                                    },
                                ) as Box<dyn FnMut(JsValue)>);

                                let _ = listen_fn.call2(
                                    &event_api,
                                    &JsValue::from_str("album://pre_analysis"),
                                    cb_album.as_ref().unchecked_ref(),
                                );
                                cb_album.forget();

                                let cb_drop = wasm_bindgen::closure::Closure::wrap(Box::new(
                                    move |ev: JsValue| {
                                        if let Ok(payload) =
                                            js_sys::Reflect::get(&ev, &JsValue::from_str("payload"))
                                        {
                                            if let Ok(ev_type) = js_sys::Reflect::get(
                                                &payload,
                                                &JsValue::from_str("type"),
                                            ) {
                                                if ev_type.as_string().unwrap_or_default() == "drop" {
                                                    if let Ok(paths) = js_sys::Reflect::get(
                                                        &payload,
                                                        &JsValue::from_str("paths"),
                                                    ) {
                                                        let paths_array = js_sys::Array::from(&paths);
                                                        let count = paths_array.length() as u32;

                                                        if count > 0 {
                                                            let first_path = paths_array
                                                                .get(0)
                                                                .as_string()
                                                                .unwrap_or_default();

                                                            dropped_path.set(Some(first_path));

                                                            dispatch_hangar(hangar_state, HangarEvent::FilesDropped { count: count as usize });
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    },
                                ) as Box<dyn FnMut(JsValue)>);

                                let _ = listen_fn.call2(
                                    &event_api,
                                    &JsValue::from_str("tauri://drag-drop"),
                                    cb_drop.as_ref().unchecked_ref(),
                                );
                                cb_drop.forget();
                            }
                        }
                    }
                }
            }
        });
    });

    // ── Queue Playback Timer ──────────────────────────────────────────────────
    use_effect(move || {
        spawn_local(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(500).await;
                let next = stage_queue.write().pop_front();
                if let Some(stage) = next {
                    web_sys::console::error_1(&format!("[TRAP] TIMER POPPED STAGE: {}", stage).into());
                    journey_stage.set(stage.clone());
                    if stage == "CERTIFIED" || stage == "ERROR" {
                        // let it display, then tear down journey
                        web_sys::console::error_1(&format!("[TRAP] TIMER: reached CERTIFIED/ERROR. Waiting 800ms to teardown").into());
                        gloo_timers::future::TimeoutFuture::new(800).await;
                        is_journey_active.set(false);
                        web_sys::console::error_1(&format!("[TRAP] TIMER: is_journey_active SET TO FALSE").into());
                    }
                }
            }
        });
    });

    rsx! {
        div { id: "app-shell", class: "app-shell",

            // ── Transport bar (bottom strip) ──────────────────────────────────
            // playback_state + 500ms poll live inside TransportBar (F4-INT-01):
            // only the footer re-renders on position updates, not the full tree.
            TransportBar {
                mode,
                tier,
                session_state,
                intent_open,
                intent_closing,
                presentation,
                hangar_state,
                jini_persona,
                dropped_path,
                last_platform,
                last_flavour,
            }

            // ── Work Layer — 65% Middle ──────────────────────────────────────
            main {
                id: "mfd-bay",
                class: if *intent_open.read() { "mfd-bay work-layer cockpit-work-layer intent-active" } else { "mfd-bay work-layer cockpit-work-layer" },

                SamplingSiamese {
                    mode,
                    session_state,
                    playback_state: use_signal(|| None),
                    viz_data,
                    wizard_findings,
                    show_mastered: show_mastered,
                    tone_angle,
                    dyn_angle,
                    space_angle,
                    loud_angle,
                    on_down_tone: move |_| {
                        if *intent_open.read() {
                            // Close: remove open immediately, play seal animation for 1600ms
                            intent_open.set(false);
                            intent_closing.set(true);
                            spawn_local(async move {
                                TimeoutFuture::new(1_600).await;
                                intent_closing.set(false);
                            });
                        } else if !*intent_closing.read() {
                            intent_open.set(true);
                        }
                    },
                    on_down_dyn: move |_| {},
                    on_down_space: move |_| {},
                    on_down_loud: move |_| {},
                    jini_persona,
                    is_journey_active,
                    journey_stage,
                    journey_elapsed_ms,
                }
            }

            // ── MasteredView overlay (P14-011) — conditional on show_mastered ──
            if *show_mastered.read() {
                MasteredView {
                    session_state,
                    viz_data,
                    on_close: move |_| {
                        show_mastered.set(false);
                    },
                }
            }

            if let Some(blob_id) = pdf_preview_ctx.read().as_ref() {
                crate::components::PdfPreviewModal {
                    blob_id: blob_id.clone(),
                    on_close: move |_| pdf_preview_ctx.set(None),
                }
            }

            // ── Hangar — 25% Bottom ──────────────────────────────────────────
            div {
                class: {
                    if *intent_open.read()         { "hangar-layer intent-open" }
                    else if *intent_closing.read() { "hangar-layer intent-closing" }
                    else                           { "hangar-layer" }
                },
                ModuleFrame {
                    title: "JINI".to_string(),
                    show_screws: false,
                    panel_class: "hangar-module".to_string(),
                    match hangar_state.read().clone() {
                        HangarInterviewState::Ready => {
                            let m = mode.read().clone();
                            web_sys::console::error_1(&format!("[READY] mode={:?}", m).into());

                            let blob = match m {
                                crate::state::cockpit_mode::CockpitMode::CoachReady { blob_id } => Some(blob_id),
                                crate::state::cockpit_mode::CockpitMode::Exporting { blob_id, .. } => Some(blob_id),
                                _ => None,
                            };
                            rsx! {
                                div { class: "jini-ready-surface",
                                    p { class: "jini-text", "Certified. Your master is ready." }
                                    if let Some(blob_id) = blob {
                                        crate::panels::session::GoldenBlobBadge {}
                                        crate::panels::session::ExportControls {
                                            mode,
                                            blob_id,
                                        }
                                    }
                                }
                            }
                        },
                        HangarInterviewState::AwaitingDrop => {
                            let hs = hangar_state;
                            let mut dp = dropped_path;
                            rsx! {
                                JiniDropZone {
                                    on_browse: move |_| {
                                        spawn_local(async move {
                                            if let Ok(Some(meta)) = invoke::<Option<crate::types::AudioMeta>, _>(
                                                "open_audio_file",
                                                serde_json::json!({})
                                            ).await {
                                                dp.set(Some(meta.path.clone()));
                                                dispatch_hangar(hs, HangarEvent::FilesDropped { count: 1 });
                                            }
                                        });
                                    }
                                }
                            }
                        },
                        HangarInterviewState::Detection { track_count } => {
                            let dp = dropped_path.read().clone().unwrap_or_default();
                            rsx! {
                                JiniDetection {
                                    key: "{dp}",
                                    track_count,
                                    on_timeout: move |_| {
                                        dispatch_hangar(hangar_state, HangarEvent::DetectionTimeout);
                                    }
                                }
                            }
                        },
                        HangarInterviewState::AwaitingMore { .. } => rsx! {
                            JiniAwaitingMore {
                                on_proceed: move |_| {
                                    dispatch_hangar(hangar_state, HangarEvent::DetectionTimeout);
                                }
                            }
                        },
                        HangarInterviewState::PlatformCard { .. } => rsx! {
                            JiniPlatformSelector {
                                on_select: move |platform| {
                                    dispatch_hangar(hangar_state, HangarEvent::PlatformChosen { platform });
                                }
                            }
                        },
                        HangarInterviewState::FlavourCard { platform, track_count: _ } => {
                            let platform_clone = platform.clone();
                            let current_path = dropped_path.read().clone();
                            
                            let m_mode = mode;
                            let hs = hangar_state;
                            let mut lp = last_platform;
                            let mut lf = last_flavour;
                            
                            let tone = (*tone_angle.read() / 135.0 + 1.0) / 2.0;
                            let dynval = (*dyn_angle.read() / 135.0 + 1.0) / 2.0;
                            
                            let mut viz_data_sig = viz_data;
                            let mut session_state_sig = session_state;
                            let mut wizard_findings_sig = wizard_findings;
                            let jini_persona_sig = jini_persona;

                            rsx! {
                                JiniFlavourSelector {
                                    on_select: move |flavour: String| {
                                        web_sys::console::log_1(&format!(
                                            "[NEW-SESSION] starting, current mode={:?}", *m_mode.read()
                                        ).into());
                                        dispatch_hangar(hs, HangarEvent::FlavourChosen { flavour: flavour.clone() });
                                        
                                        if let Some(path) = current_path.clone() {
                                            let pr = platform_clone.clone();
                                            let fl = flavour;
                                            pending_master_req.set(Some((path, pr, fl, tone, dynval)));
                                        }
                                    }
                                }
                            }
                        },
                        HangarInterviewState::Ignition { .. } => rsx! {
                            JiniAnalysing { stage: journey_stage.read().clone() }
                        },
                        HangarInterviewState::Analysing => rsx! {
                            JiniAnalysing { stage: journey_stage.read().clone() }
                        },
                    }
                }
            }
        }
    }
}
