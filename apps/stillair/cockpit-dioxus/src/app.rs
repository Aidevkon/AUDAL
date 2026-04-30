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
    mastered::MasteredView,
};
use crate::components::sampling_siamese::SamplingSiamese;
use crate::components::intent_bay::IntentBay;
use crate::components::{
    screw::Screw,
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

#[allow(non_snake_case)]
pub fn App() -> Element {
    // ── Signals ──────────────────────────────────────────────────────────────
    let mut mode           = use_signal(|| CockpitMode::Idle);
    let mut session_state  = use_signal(|| None::<SessionStateJson>);
    let viz_data: Signal<Option<VisualizationDataJson>> = use_signal(|| None);
    let playback_state: Signal<Option<PlaybackStateJson>> = use_signal(|| None);
    // MasteredView overlay visibility (Signal only — no IPC per §5.3)
    let mut show_mastered: Signal<bool> = use_signal(|| false);
    let mut intent_open: Signal<bool> = use_signal(|| false);
    let tone_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let dyn_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let space_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let loud_angle: Signal<f32> = use_signal(|| 0.0_f32);

    // ── Mode badge style  ─────────────────────────────────────────────────────



    // ── P12B-004: Position polling — 500ms when in FM5 ───────────────────────
    {
        let playback_state = playback_state.clone();
        let mode           = mode.clone();

        use_effect(move || {
            let playback_state = playback_state.clone();
            let mode           = mode.clone();

            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(500).await;

                    // Only poll when in FM5 (CoachReady)
                    if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) {
                        // If not in FM5, wait and check again
                        continue;
                    }

                    match crate::ipc::invoke_no_args::<Option<PlaybackStateJson>>(
                        "get_playback_state"
                    ).await {
                        Ok(Some(state)) => { playback_state.clone().set(Some(state)); }
                        _               => {}
                    }
                }
            });
        });
    }

    // ── Transport bar state ───────────────────────────────────────────────────
    // NOTE: has_audio is NOT stored as a let-binding here — Dioxus 0.6 does not
    // re-subscribe to signals read as plain let-bindings outside rsx!.
    // Use  matches!(*mode.read(), CockpitMode::CoachReady { .. })  inline.
    let is_playing   = playback_state.read().as_ref().map(|s| s.is_playing).unwrap_or(false);
    let position_ms  = playback_state.read().as_ref().map(|s| s.position_ms).unwrap_or(0);
    let duration_ms  = playback_state.read().as_ref().map(|s| s.duration_ms).unwrap_or(0);
    let scrub_len    = scrub_pct(position_ms, duration_ms);

    rsx! {

        div {
            id:    "app-shell",
            class: "app-shell",

            // ── Transport bar (bottom strip) — Phase 12B ──────────────────────
            footer {
                id:    "transport-bar",
                class: "transport-bar",

                Screw { top: 10, left: 10 }
                Screw { top: 10, right: 10 }
                Screw { bottom: 10, left: 10 }
                Screw { bottom: 10, right: 10 }

                // Left: Annunciator Zone
                div {
                    class: "transport-left dsp-annunciators",
                    Annunciator { label: "FM0".to_string(), is_master: true, active: true }
                    div { class: "dsp-separator" }
                    Annunciator { label: "EQ".to_string(), active: true, sub_labels: Some(vec!["A1".to_string(), "A2".to_string(), "A3".to_string()]) }
                    Annunciator { label: "COMP".to_string(), active: true, sub_labels: Some(vec!["B1".to_string(), "B2".to_string(), "B3".to_string()]) }
                    Annunciator { label: "SAT".to_string(), active: true, sub_labels: Some(vec!["C1".to_string(), "C2".to_string(), "C3".to_string()]) }
                    Annunciator { label: "LIMIT".to_string(), active: true, sub_labels: Some(vec!["D1".to_string(), "D2".to_string(), "D3".to_string()]) }
                }

                div { class: "transport-module-divider" }

                // Center: Transport & Time
                div {
                    class: "transport-center",

                    div {
                        class: "transport-mfd-pit",
                        
                        // Top row of controls and timecode
                        div { class: "mfd-controls-row",
                            // SKIP BACK
                            button {
                                class: "btn-keycap",
                                onclick: move |_| {
                                    let new_ms = position_ms.saturating_sub(5_000);
                                    let ps = playback_state.clone();
                                    spawn_local(async move { invoke_playback("seek", Some(new_ms), ps).await; });
                                },
                                "◄◄"
                            }

                            // Middle: OLED Timecode
                            div {
                                class: "transport-vfd-display transport-time",
                                id:    "transport-position",
                                { format!("00:{:02}:{:02}:{:03}", position_ms / 60000, (position_ms / 1000) % 60, position_ms % 1000) }
                            }

                            // SKIP FORWARD
                            button {
                                class: "btn-keycap",
                                onclick: move |_| {
                                    let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                                    let ps = playback_state.clone();
                                    spawn_local(async move { invoke_playback("seek", Some(new_ms), ps).await; });
                                },
                                "►►"
                            }

                            // PLAY
                            button {
                                class: if is_playing { "btn-keycap btn-keycap-orange active" } else { "btn-keycap btn-keycap-orange" },
                                onclick: move |_| {
                                    let action = if is_playing { "pause" } else { "play" };
                                    let ps = playback_state.clone();
                                    spawn_local(async move { invoke_playback(action, None, ps).await; });
                                },
                                "PLAY"
                            }

                            // STOP
                            button {
                                class: "btn-keycap btn-keycap-stop",
                                onclick: move |_| {
                                    let ps = playback_state.clone();
                                    spawn_local(async move { invoke_playback("stop", None, ps).await; });
                                },
                                "STOP"
                            }
                        }

                        // Bottom: Glowing Scrub Trench
                        div {
                            class: "transport-scrub-trench",
                            id:    "transport-scrub",
                            onclick: move |evt| {
                                if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) || duration_ms == 0 { return; }
                                let client_x = evt.client_coordinates().x;
                                let window   = web_sys::window().unwrap();
                                let doc      = window.document().unwrap();
                                if let Some(el) = doc.get_element_by_id("transport-scrub") {
                                    let rect  = el.get_bounding_client_rect();
                                    let frac  = ((client_x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                                    let seek_ms = (frac * duration_ms as f64) as u64;
                                    let ps      = playback_state.clone();
                                    spawn_local(async move { invoke_playback("seek", Some(seek_ms), ps).await; });
                                }
                            },
                            div { class: "transport-scrub-trench-fill", style: format!("width:{}%", scrub_len) }
                        }
                    }
                }

                div { class: "transport-module-divider" }

                // Right: Critical Zone
                div {
                    class: "transport-right abort-zone",
                    // Small STOP pill next to ABORT
                    button {
                        class: "btn-pill btn-pill-red",
                        onclick: move |_| {
                            let ps = playback_state.clone();
                            spawn_local(async move { invoke_playback("stop", None, ps).await; });
                        },
                        "STOP"
                    }

                    div { class: "abort-guard-wrapper",
                        div { class: "abort-guard-left" }
                        button {
                            class: "btn-abort-guarded",
                            onclick: move |_| {
                                let mut s = session_state.write();
                                *s = None;
                                let mut m = mode.write();
                                *m = CockpitMode::Idle;
                            },
                            "ABORT"
                        }
                        div { class: "abort-guard-right" }
                    }
                }
            }

            // ── Work Layer — 65% Middle ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: if *intent_open.read() { "mfd-bay work-layer cockpit-work-layer intent-active" } else { "mfd-bay work-layer cockpit-work-layer" },

                SamplingSiamese {
                    mode,
                    session_state,
                    playback_state,
                    viz_data,
                    show_mastered: show_mastered.clone(),
                }
            }

            // ── MasteredView overlay (P14-011) — conditional on show_mastered ──
            if *show_mastered.read() {
                MasteredView {
                    session_state,
                    viz_data,
                    on_close: move |_| { show_mastered.set(false); },
                }
            }

            // ── Hangar — 25% Bottom ──────────────────────────────────────────
            div {
                class: if *intent_open.read() { "hangar-layer intent-open" } else { "hangar-layer" },
                div { class: "intent-knob-bay",
                    IntentBay {
                        open: *intent_open.read(),
                        tone_angle: *tone_angle.read(),
                        dyn_angle: *dyn_angle.read(),
                        space_angle: *space_angle.read(),
                        loud_angle: *loud_angle.read(),
                        on_down_tone: move |_| {
                            let current = *intent_open.read();
                            intent_open.set(!current);
                        },
                        on_down_dyn: move |_| {},
                        on_down_space: move |_| {},
                        on_down_loud: move |_| {},
                    }
                }
                div { class: "coach-panel",
                    CoachPanel { mode, session_state }
                }
            }
        }
    }
}
