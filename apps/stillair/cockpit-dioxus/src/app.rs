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
use gloo_timers::future::TimeoutFuture;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};
use crate::panels::{
    coach::CoachPanel,
    mastered::MasteredView,
};
use crate::components::sampling_siamese::SamplingSiamese;
use crate::components::intent_bay::IntentBay;
use crate::components::{
    annunciator::Annunciator,
    transport_button::{TransportActuator, LedColor, SkipActuator},
    screw::Screw,
    ab_toggle::{AbToggle, AbToggleState},
    timecode::TimecodeDisplay,
    oled_tile::{OledTile, OledTileState},
};



// ── Helpers ───────────────────────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum TransportState {
    Playing,
    Stopped,
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

#[derive(Clone, Copy, PartialEq)]
enum AbortState {
    IdleClosed,
    Armed,
    Triggered,
    Cooldown,
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
    let mut intent_open:    Signal<bool> = use_signal(|| false);
    let mut intent_closing: Signal<bool> = use_signal(|| false);
    let mut abort_state: Signal<AbortState> = use_signal(|| AbortState::IdleClosed);
    let mut current_state = use_signal(|| TransportState::Stopped);
    let tone_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let dyn_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let space_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let loud_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let mut ab_state = use_signal(|| AbToggleState::A);
    let mut ab_press_time = use_signal(|| 0u64);

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
    // let is_playing   = playback_state.read().as_ref().map(|s| s.is_playing).unwrap_or(false);
    let position_ms  = playback_state.read().as_ref().map(|s| s.position_ms).unwrap_or(0);
    let duration_ms  = playback_state.read().as_ref().map(|s| s.duration_ms).unwrap_or(0);
    let scrub_len    = scrub_pct(position_ms, duration_ms);

    // Define labels outside rsx! to prevent editor syntax highlighter errors with '<' and '>'
    let lbl_skip_back = "<<".to_string();
    let lbl_skip_fwd  = ">>".to_string();

    rsx! {

        div { id: "app-shell", class: "app-shell",

            // ── Transport bar (bottom strip) — Phase 12B ──────────────────────
            footer { id: "transport-bar", class: "transport-bar",
                div { class: "transport-rim",

                    div { class: "transport-panel",
                        // Left: Annunciator Zone
                        div { class: "transport-left dsp-annunciators",
                            div { class: "fm0-zone",
                                OledTile { state: OledTileState::Lock }
                                div { class: "oled-tile__screw-bl" }
                                div { class: "oled-tile__screw-br" }
                            }
                            div { class: "dsp-chassis",
                        div { class: "dsp-slots-wrapper",
                            Annunciator {
                                label: "EQ".to_string(),
                                active: true,
                                pipeline_stages: 3,
                            }
                            Annunciator {
                                label: "COMP".to_string(),
                                active: true,
                                pipeline_stages: 3,
                            }
                            Annunciator {
                                label: "SAT".to_string(),
                                active: true,
                                pipeline_stages: 3,
                            }
                            Annunciator {
                                label: "LIMIT".to_string(),
                                active: true,
                                pipeline_stages: 3,
                            }
                        }
                    }
                }


                // Center: Transport & Time
                div { class: "transport-center",
                    div { class: "insert-panel",
                        Screw { top: 8, left: 8 }
                        Screw { top: 8, right: 8 }
                        Screw { bottom: 8, left: 8 }
                        Screw { bottom: 8, right: 8 }
                        
                        div { class: "transport-mfd-pit",
                                // Top row of controls and timecode
                                div { class: "control-housing",
                                    div { class: "mfd-controls-row",
                                        // SKIP BACK
                                        div { class: "transport-btn-col",
                                            div { class: "transport-top-label", "–5" }
                                            div { class: "button-base-seat-narrow",
                                                SkipActuator {
                                                    label: "{lbl_skip_back}",
                                                    on_click: move |_| {
                                                        let new_ms = position_ms.saturating_sub(5_000);
                                                        let ps = playback_state.clone();
                                                        spawn_local(async move {
                                                            invoke_playback("seek", Some(new_ms), ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }

                                        // Scrub Knob — inline with transport buttons
                                        div {
                                            class: "scrub-knob-assembly",
                                            id: "transport-scrub",
                                            onclick: move |evt| {
                                                if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) || duration_ms == 0 {
                                                    return;
                                                }
                                                let client_x = evt.client_coordinates().x;
                                                let window = web_sys::window().unwrap();
                                                let doc = window.document().unwrap();
                                                if let Some(el) = doc.get_element_by_id("transport-scrub") {
                                                    let rect = el.get_bounding_client_rect();
                                                    let frac = ((client_x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                                                    let seek_ms = (frac * duration_ms as f64) as u64;
                                                    let ps = playback_state.clone();
                                                    spawn_local(async move {
                                                        invoke_playback("seek", Some(seek_ms), ps).await;
                                                    });
                                                }
                                            },
                                            div { class: "transport-top-label", style: "visibility: hidden;", "SCRUB" }
                                            div {
                                                class: "scrub-knob-wrapper",
                                                style: {
                                                    let active_angle_deg = (scrub_len / 100.0) * 180.0 - 90.0;
                                                    format!("--active-angle: {:.2}deg;", active_angle_deg)
                                                },
                                                div { class: "scrub-knob__trench" }
                                                div { class: "scrub-knob__oled-ring",
                                                    svg {
                                                        class: "scrub-knob__arc",
                                                        view_box: "0 0 72 72",
                                                        xmlns: "http://www.w3.org/2000/svg",
                                                        {
                                                            let total_ticks = 17_u32;
                                                            let active_count = ((scrub_len / 100.0) * (total_ticks - 1) as f64).round() as u32;
                                                            (0..total_ticks).map(move |i| {
                                                                // BOTTOM ARC: 0° to 180° (smile)
                                                                let angle_deg = 0.0 + (i as f64 / (total_ticks - 1) as f64) * 180.0;
                                                                let angle_rad = angle_deg * std::f64::consts::PI / 180.0;
                                                                let cx = 36.0_f64;
                                                                let cy = 36.0_f64;
                                                                // Moat is between 22px (cap) and 36px (trench). Center is 29px.
                                                                // A 6px long tick centered at 29px means 26px to 32px.
                                                                let r_inner = 26.0_f64;
                                                                let r_outer = 32.0_f64;
                                                                let x1 = cx + r_inner * angle_rad.cos();
                                                                let y1 = cy + r_inner * angle_rad.sin();
                                                                let x2 = cx + r_outer * angle_rad.cos();
                                                                let y2 = cy + r_outer * angle_rad.sin();
                                                                let is_active = i <= active_count;
                                                                // 3-tier: super-bright current, active trail, dim inactive
                                                                let is_current = i == active_count;
                                                                let color = if is_current {
                                                                    "#ffe699"          // Warm bright yellow-amber (not pure white)
                                                                } else if is_active {
                                                                    "#ffb703"          // active trail
                                                                } else {
                                                                    "#8a6311"          // solid dark amber
                                                                };
                                                                let width = if is_current { "5" } else { "4" };
                                                                let line_class = if is_current { "scrub-knob__tick scrub-knob__tick--active" } else { "scrub-knob__tick" };
                                                                rsx! {
                                                                    line {
                                                                        key: "{i}",
                                                                        class: "{line_class}",
                                                                        x1: "{x1:.2}", y1: "{y1:.2}",
                                                                        x2: "{x2:.2}", y2: "{y2:.2}",
                                                                        stroke: "{color}",
                                                                        stroke_width: "{width}",
                                                                        stroke_linecap: "round",
                                                                    }
                                                                }
                                                            })
                                                        }
                                                    }
                                                }
                                                div { class: "scrub-knob__glass" }
                                                div { class: "scrub-knob__spill-glow" }
                                                div { class: "scrub-knob__occlusion-mask" }
                                                div { class: "scrub-knob__rotor",
                                                    div { class: "scrub-knob__rim-highlight" }
                                                    div { class: "scrub-knob__skirt" }
                                                    div { class: "scrub-knob__skirt-reflection" }
                                                    div { class: "scrub-knob__faceplate" }
                                                    div { class: "scrub-knob__specular-highlight" }
                                                }
                                            }
                                        }
// SKIP FORWARD
                                        div { class: "transport-btn-col",
                                            div { class: "transport-top-label", "+5" }
                                            div { class: "button-base-seat-narrow",
                                                SkipActuator {
                                                    label: "{lbl_skip_fwd}",
                                                    on_click: move |_| {
                                                        let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                                                        let ps = playback_state.clone();
                                                        spawn_local(async move {
                                                            invoke_playback("seek", Some(new_ms), ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }

                                        // Middle: OLED Timecode
                                        div { class: "lcd-bezel",
                                            div {
                                                class: "transport-vfd-display transport-time",
                                                id: "transport-position",
                                                TimecodeDisplay {
                                                    value: format!(
                                                        "00:{:02}:{:02}:{:03}",
                                                        position_ms / 60000,
                                                        (position_ms / 1000) % 60,
                                                        position_ms % 1000,
                                                    )
                                                }
                                            }
                                        }

                                        // STOP
                                        div { class: "transport-btn-col",
                                            div { class: if current_state() == TransportState::Stopped { "transport-led-pill red-active" } else { "transport-led-pill" } }
                                            div { class: "button-base-seat",
                                                TransportActuator {
                                                    label: "STOP".to_string(),
                                                    color: LedColor::Red,
                                                    active: current_state() == TransportState::Stopped,
                                                    on_click: move |_| {
                                                        current_state.set(TransportState::Stopped);
                                                        let ps = playback_state.clone();
                                                        spawn_local(async move {
                                                            invoke_playback("stop", None, ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }
                                        // PLAY
                                        div { class: "transport-btn-col",
                                            div { class: if current_state() == TransportState::Playing { "transport-led-pill amber-active" } else { "transport-led-pill" } }
                                            div { class: "button-base-seat",
                                                TransportActuator {
                                                    label: "PLAY".to_string(),
                                                    color: LedColor::Amber,
                                                    active: current_state() == TransportState::Playing,
                                                    on_click: move |_| {
                                                        current_state.set(TransportState::Playing);
                                                        let ps = playback_state.clone();
                                                        spawn_local(async move {
                                                            invoke_playback("play", None, ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }

                                                                                AbToggle {
                                            state: (*ab_state.read()).clone(),
                                            on_mousedown: move |_| {
                                                let now = js_sys::Date::now() as u64;
                                                ab_press_time.set(now);
                                            },
                                            on_mouseup: move |_| {
                                                let press_duration = js_sys::Date::now() as u64
                                                    - *ab_press_time.read();
                                                if press_duration >= 300 {
                                                    // long press — return to A
                                                    ab_state.set(AbToggleState::A);
                                                }
                                            },
                                            on_click: move |_| {
                                                let next = match *ab_state.read() {
                                                    AbToggleState::A => AbToggleState::B,
                                                    AbToggleState::B => AbToggleState::A,
                                                    AbToggleState::Toggled => AbToggleState::A,
                                                };
                                                ab_state.set(next);
                                            },
                                        }
                                    }
                                }
                            }
                        }
                }


                // Right: Critical Zone
                div { class: "transport-right abort-zone",
                    // Small STOP pill next to ABORT (Repurposed for Intent Bay reveal)
                    button {
                        class: "btn-pill btn-pill-red",
                        onclick: move |_| {
                            let current = *intent_open.read();
                            intent_open.set(!current);
                        },
                        "HA"
                    }

                    // ABORT — Flip-Guard Cap + Deep Cavity + PA-Family Red Actuator
                    div { class: "abort-housing",
                      div {
                        class: match *abort_state.read() {
                            AbortState::IdleClosed => "abort-column",
                            AbortState::Armed => "abort-column armed",
                            AbortState::Triggered => "abort-column triggered",
                            AbortState::Cooldown => "abort-column cooldown",
                        },

                        // Layer 1: Flip cap (satin-matte milled aluminum + CNC letters)
                        div {
                            class: "abort-cover",
                            onclick: move |_| {
                                let current = *abort_state.read();
                                match current {
                                    AbortState::IdleClosed => abort_state.set(AbortState::Armed),
                                    AbortState::Armed | AbortState::Triggered => abort_state.set(AbortState::IdleClosed),
                                    AbortState::Cooldown => {}, // Block manual close during blackout
                                }
                            },
                            div { class: "abort-cover-frame" }
                        }

                        // Layer 2: Deep cavity — PA-family red actuator inside
                        div { class: "abort-cavity",
                            div { class: "abort-inner-socket",
                                button {
                                    class: "abort-inner-button",
                                    style: if *abort_state.read() != AbortState::Armed { "pointer-events: none;" } else { "" },
                                    onclick: move |_| {
                                        if *abort_state.read() != AbortState::Armed { return; }
                                        let mut s = session_state.write();
                                        *s = None;
                                        let mut m = mode.write();
                                        *m = CockpitMode::Idle;
                                        abort_state.set(AbortState::Triggered);
                                        spawn_local(async move {
                                            gloo_timers::future::TimeoutFuture::new(4_000).await;
                                            abort_state.set(AbortState::Cooldown);
                                            gloo_timers::future::TimeoutFuture::new(200).await;
                                            abort_state.set(AbortState::IdleClosed);
                                        });
                                    },
                                    div { class: "abort-jewel-glow" }
                                    span { class: "abort-inner-label", "ABORT" }
                                }
                            }
                        }
                      }
                    }
                        }
                    }
                }
            }

            // ── Work Layer — 65% Middle ──────────────────────────────────────
            main {
                id: "mfd-bay",
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
                    on_close: move |_| {
                        show_mastered.set(false);
                    },
                }
            }

            // ── Hangar — 25% Bottom ──────────────────────────────────────────
            div {
                class: {
                    if *intent_open.read()    { "hangar-layer intent-open" }
                    else if *intent_closing.read() { "hangar-layer intent-closing" }
                    else                      { "hangar-layer" }
                },
                div { class: "intent-knob-bay",
                    IntentBay {
                        tone_angle: *tone_angle.read(),
                        dyn_angle: *dyn_angle.read(),
                        space_angle: *space_angle.read(),
                        loud_angle: *loud_angle.read(),
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
                    }
                }
                div { class: "coach-panel",
                    CoachPanel { mode, session_state }
                }
            }
        }
    }
}
