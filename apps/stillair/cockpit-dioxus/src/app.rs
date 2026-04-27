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

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};
use crate::panels::{
    coach::CoachPanel,
    insights::InsightsPanel,
    mastered::MasteredView,
};
use crate::components::{
    screw::Screw,
    soft_key::{SoftKey, SoftKeyVariant},
    annunciator::Annunciator,
};



// ── Helpers ───────────────────────────────────────────────────────────────────

/// Format milliseconds as MM:SS
fn format_ms(ms: u64) -> String {
    let secs = ms / 1_000;
    let mins = secs / 60;
    format!("{:02}:{:02}", mins, secs % 60)
}

/// Scrub fill % (0.0–100.0)
fn scrub_pct(position_ms: u64, duration_ms: u64) -> f64 {
    if duration_ms == 0 { 0.0 }
    else { (position_ms as f64 / duration_ms as f64 * 100.0).clamp(0.0, 100.0) }
}

// ── IPC (fire-and-forget) ─────────────────────────────────────────────────────

async fn invoke_playback(
    action:         &'static str,
    position_ms:    Option<u64>,
    playback_state: Signal<Option<PlaybackStateJson>>,
) {
    let args = serde_json::json!({ "action": action, "positionMs": position_ms });
    match crate::ipc::invoke::<Option<PlaybackStateJson>, _>("playback_control", args).await {
        Ok(Some(state)) => { playback_state.clone().set(Some(state)); }
        Ok(None)        => {}
        Err(e)          => { web_sys::console::log_1(&format!("[transport] {action} error: {e}").into()); }
    }
}

// ── App root ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub enum DraggingKnob { None, Tone, Dynamics, Space, Loudness }

pub fn App() -> Element {
    let mode           = use_signal(|| CockpitMode::Idle);
    let session_state  = use_signal(|| None::<SessionStateJson>);
    let viz_data: Signal<Option<VisualizationDataJson>> = use_signal(|| None);
    let playback_state: Signal<Option<PlaybackStateJson>> = use_signal(|| None);
    let mut show_mastered: Signal<bool> = use_signal(|| false);

    let mut dragging = use_signal(|| DraggingKnob::None);
    let mut start_y = use_signal(|| 0.0_f32);
    let mut tone_angle = use_signal(|| 0.0_f32);
    let mut dyn_angle = use_signal(|| 0.0_f32);
    let mut space_angle = use_signal(|| 0.0_f32);
    let mut loud_angle = use_signal(|| 0.0_f32);
    let mut intent_open = use_signal(|| false); // Test toggling later or rely on shortcut/severity

    {
        let playback_state = playback_state.clone();
        let mode           = mode.clone();
        use_effect(move || {
            let playback_state = playback_state.clone();
            let mode           = mode.clone();
            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(500).await;
                    if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) { continue; }
                    match crate::ipc::invoke_no_args::<Option<PlaybackStateJson>>("get_playback_state").await {
                        Ok(Some(state)) => { playback_state.clone().set(Some(state)); }
                        _               => {}
                    }
                }
            });
        });
    }

    // Temporary keyboard shortcut to test opening IntentBay
    use_effect(move || {
        let mut intent_open = intent_open.clone();
        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        
        let cb = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            if e.key() == "i" && !e.ctrl_key() && !e.alt_key() && !e.meta_key() {
                let current = *intent_open.read();
                intent_open.set(!current);
            }
        }) as Box<dyn FnMut(_)>);
        
        use wasm_bindgen::JsCast;
        document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref()).unwrap();
        cb.forget();
    });

    rsx! {
        div { 
            class: "main-chassis",
            onmousemove: move |e| {
                let knob = *dragging.read();
                if knob != DraggingKnob::None {
                    let current_y = e.client_coordinates().y as f32;
                    let delta = (*start_y.read() - current_y) * 1.2;
                    let mut update = |mut angle: Signal<f32>| {
                        let new_val = (*angle.read() + delta).clamp(-135.0, 135.0);
                        angle.set(new_val);
                    };
                    match knob {
                        DraggingKnob::Tone => update(tone_angle),
                        DraggingKnob::Dynamics => update(dyn_angle),
                        DraggingKnob::Space => update(space_angle),
                        DraggingKnob::Loudness => update(loud_angle),
                        DraggingKnob::None => {}
                    }
                    start_y.set(current_y);
                }
            },
            onmouseup: move |_| { dragging.set(DraggingKnob::None); },
            onmouseleave: move |_| { dragging.set(DraggingKnob::None); },

            PFRTransportBar { mode, playback_state }
            HUDOverlay {}
            if *show_mastered.read() {
                MasteredView {
                    session_state,
                    viz_data,
                    on_close: move |_| { show_mastered.set(false); },
                }
            }
            div { 
                class: if *intent_open.read() { "cockpit-work-layer intent-active" } else { "cockpit-work-layer" },
                SiamesePanels { mode, session_state, playback_state, viz_data }
                crate::components::active_processing_chain::ActiveProcessingChain {
                    eq: crate::components::active_processing_chain::types::EQState {
                        low_db: 2.5, mid_db: -1.0, presence_db: 0.0, air_db: 1.5,
                        curve_points: vec![(0.0, 0.5), (0.1, 0.4), (0.5, 0.6), (0.8, 0.5), (1.0, 0.3)],
                    },
                    compressor: crate::components::active_processing_chain::types::CompressorState {
                        threshold_db: -18.0, ratio: 4.0, gain_reduction_db: -3.2, makeup_db: 2.0,
                        curve_points: vec![(0.0, 1.0), (0.5, 0.5), (1.0, 0.2)],
                    },
                    limiter: crate::components::active_processing_chain::types::LimiterState {
                        ceiling_dbtp: -1.0, release_auto: true, isp_factor: 4,
                        curve_points: vec![(0.0, 1.0), (0.5, 0.3), (1.0, 0.2)],
                    }
                }
            }
            crate::components::intent_bay::IntentBay { 
                open: *intent_open.read(),
                tone_angle: *tone_angle.read(),
                dyn_angle: *dyn_angle.read(),
                space_angle: *space_angle.read(),
                loud_angle: *loud_angle.read(),
                on_down_tone: move |e: MouseEvent| { dragging.set(DraggingKnob::Tone); start_y.set(e.client_coordinates().y as f32); },
                on_down_dyn: move |e: MouseEvent| { dragging.set(DraggingKnob::Dynamics); start_y.set(e.client_coordinates().y as f32); },
                on_down_space: move |e: MouseEvent| { dragging.set(DraggingKnob::Space); start_y.set(e.client_coordinates().y as f32); },
                on_down_loud: move |e: MouseEvent| { dragging.set(DraggingKnob::Loudness); start_y.set(e.client_coordinates().y as f32); },
            }
            Hangar { mode, session_state }
        }
    }
}

// ── New Component Stubs ────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct PFRTransportBarProps {
    pub mode: Signal<CockpitMode>,
    pub playback_state: Signal<Option<PlaybackStateJson>>,
}

#[component]
pub fn PFRTransportBar(props: PFRTransportBarProps) -> Element {
    let mode = props.mode;
    let playback_state = props.playback_state;
    let is_playing   = playback_state.read().as_ref().map(|s| s.is_playing).unwrap_or(false);
    let position_ms  = playback_state.read().as_ref().map(|s| s.position_ms).unwrap_or(0);
    let duration_ms  = playback_state.read().as_ref().map(|s| s.duration_ms).unwrap_or(0);
    let scrub_len    = scrub_pct(position_ms, duration_ms);

    rsx! {
        footer { id: "transport-bar", class: "transport-bar",
            Screw { top: 10, left: 10 }
            Screw { top: 10, right: 10 }
            Screw { bottom: 10, left: 10 }
            Screw { bottom: 10, right: 10 }
            div { class: "transport-left dsp-annunciators",
                Annunciator { label: "FM0".to_string(), is_master: true, active: true }
                Annunciator { label: "EQ".to_string(), active: false }
                Annunciator { label: "COMP".to_string(), active: false }
                Annunciator { label: "SAT".to_string(), active: false }
                Annunciator { label: "LIMIT".to_string(), active: false }
            }
            div { class: "transport-center",
                div { class: "transport-center-top",
                    SoftKey {
                        label: "◄◄".to_string(), title: "Skip back 5 seconds".to_string(),
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        variant: SoftKeyVariant::Standard,
                        onclick: move |_| {
                            let new_ms = position_ms.saturating_sub(5_000);
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback("seek", Some(new_ms), ps).await; });
                        }
                    }
                    div { class: "transport-vfd-display transport-time", id: "transport-position", { format_ms(position_ms) } }
                    SoftKey {
                        label: "►►".to_string(), title: "Skip forward 5 seconds".to_string(),
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        variant: SoftKeyVariant::Standard,
                        onclick: move |_| {
                            let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback("seek", Some(new_ms), ps).await; });
                        }
                    }
                    SoftKey {
                        label: "PLAY".to_string(), active: is_playing,
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        variant: SoftKeyVariant::Active,
                        onclick: move |_| {
                            let action = if is_playing { "pause" } else { "play" };
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback(action, None, ps).await; });
                        }
                    }
                    SoftKey {
                        label: "STOP".to_string(),
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        variant: SoftKeyVariant::Standard,
                        onclick: move |_| {
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback("stop", None, ps).await; });
                        }
                    }
                }
                div { class: "transport-scrub", id: "transport-scrub", title: "Click to seek",
                    onclick: move |evt| {
                        if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) || duration_ms == 0 { return; }
                        let client_x = evt.client_coordinates().x;
                        let window   = web_sys::window().unwrap();
                        let doc      = window.document().unwrap();
                        if let Some(el) = doc.get_element_by_id("transport-scrub") {
                            let rect = el.get_bounding_client_rect();
                            let frac = ((client_x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            let seek_ms = (frac * duration_ms as f64) as u64;
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback("seek", Some(seek_ms), ps).await; });
                        }
                    },
                    div { class: "transport-scrub-track",
                        div { class: "transport-scrub-fill", style: format!("width:{}%", scrub_len) }
                    }
                    div { class: "transport-scrub-head", style: format!("left:{}%", scrub_len) }
                    div { class: "transport-scrub-label", "●SCRUB●" }
                }
            }
            div { class: "transport-right abort-zone",
                SoftKey {
                    label: "ABORT".to_string(), variant: SoftKeyVariant::Danger, is_guarded: true,
                    onclick: move |_| { web_sys::console::warn_1(&"Avionics ABORT trigger activated".into()); }
                }
            }
        }
    }
}

#[component]
pub fn HUDOverlay() -> Element {
    rsx! {
        div { class: "hud-overlay", style: "z-index: 60;" }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct SiamesePanelsProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
    pub playback_state: Signal<Option<PlaybackStateJson>>,
    pub viz_data: Signal<Option<VisualizationDataJson>>,
}

#[component]
pub fn SiamesePanels(props: SiamesePanelsProps) -> Element {
    rsx! {
        InsightsPanel {
            mode: props.mode,
            session_state: props.session_state,
            playback_state: props.playback_state,
            viz_data: props.viz_data,
        }
    }
}





#[derive(Props, Clone, PartialEq)]
pub struct HangarProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
}

#[component]
pub fn Hangar(props: HangarProps) -> Element {
    rsx! {
        CoachPanel {
            mode: props.mode,
            session_state: props.session_state,
        }
    }
}
