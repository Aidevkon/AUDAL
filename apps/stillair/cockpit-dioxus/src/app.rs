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


use crate::components::sampling_siamese::SamplingSiamese;
use crate::components::transport_bar::TransportBar;
use crate::panels::{coach::CoachPanel, mastered::MasteredView};
use crate::state::cockpit_mode::CockpitMode;
use crate::state::presets::{FLAVOURS, PLATFORMS};
use crate::state::hangar_interview::HangarInterviewState;
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
    let mut journey_elapsed_ms: Signal<u64> = use_signal(|| 0);
    let mut bpm_signal: Signal<f32> = use_signal(|| 0.0);
    // TODO: When file drop is implemented (OB-P2 full),
    // wire Ignition → CockpitMode::FileLoaded { path, name, format }
    // and Analysing → drive AnalysisStage LEDs in Left MFD
    let mut hangar_state = use_signal(|| HangarInterviewState::AwaitingDrop);

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
                                                    journey_stage.set(s.clone());
                                                    // INV-JV-2: always returns to standard layout after CERTIFIED
                                                    if s == "CERTIFIED" || s == "ERROR" {
                                                        spawn_local(async move {
                                                            gloo_timers::future::TimeoutFuture::new(1000).await;
                                                            is_journey_active.set(false);
                                                        });
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
                            }
                        }
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
                match hangar_state.read().clone() {
                    HangarInterviewState::AwaitingDrop => rsx! {
                        div { class: "hangar-drop-zone",
                            p { "Drop your audio here." }
                        }
                    },
                    HangarInterviewState::Detection { .. } => rsx! {
                        p { "Single track." }
                    },
                    HangarInterviewState::AwaitingMore { .. } => rsx! {
                        p { "Waiting for more tracks..." }
                        button { onclick: move |_| {}, "[+] Add" }
                    },
                    HangarInterviewState::PlatformCard { .. } => rsx! {
                        p { "Where is this going?" }
                        for p in PLATFORMS {
                            {
                                let id = p.id;
                                rsx! {
                                    button {
                                        onclick: move |_| {
                                            hangar_state.set(
                                                HangarInterviewState::FlavourCard {
                                                    platform: id.to_string(),
                                                    track_count: 1,
                                                }
                                            );
                                        },
                                        "{p.label}"
                                    }
                                }
                            }
                        }
                    },
                    HangarInterviewState::FlavourCard { platform, track_count: _track_count } => {
                        // Progressive gate: if sessions >= 5, show memory prompt
                        // STUBS: Set to 5 and "Warm Analog" to force render the UI
                        let sessions: u32 = 5; 
                        let last_flavour: Option<String> = Some("Warm Analog".to_string());
                        
                        if sessions >= 5 && last_flavour.is_some() {
                            let flav_text = last_flavour.clone().unwrap();
                            let flav_action = last_flavour.unwrap();
                            
                            let p1 = platform.clone();
                            rsx! {
                                p { "Last time: {flav_text}. Same this time?" }
                                button { onclick: move |_| {
                                    hangar_state.set(HangarInterviewState::Ignition {
                                        platform: p1.clone(),
                                        flavour: flav_action.clone(),
                                    });
                                }, "Yes" }
                                button { onclick: move |_| {
                                    // TODO: clear memory and show full flavour card
                                }, "Change it" }
                            }
                        } else {
                            rsx! {
                                p { "How do you want it to sound?" }
                                for f in FLAVOURS {
                                    {
                                        let id = f.id;
                                        let platform_clone = platform.clone();
                                        rsx! {
                                            button {
                                                onclick: move |_| {
                                                    hangar_state.set(HangarInterviewState::Ignition {
                                                        platform: platform_clone.clone(),
                                                        flavour: id.to_string(),
                                                    });
                                                },
                                                "{f.label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    HangarInterviewState::Ignition { platform, flavour } => {
                        let _ = (platform, flavour); // stub — real path from drop event
                        rsx! { p { "Analysing." } }
                    },
                    HangarInterviewState::Analysing => rsx! {
                        p { "Analysing." }
                    },
                    HangarInterviewState::Ready => {
                        // Stub — full CockpitMode wiring when file drop implemented
                        rsx! {
                            div { class: "coach-panel chassis-bezel",
                                CoachPanel { 
                                    mode, session_state, wizard_findings,
                                    jini_suggestion, jini_persona 
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
