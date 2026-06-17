//! transport_bar.rs — TransportBar component. Extracted from app.rs (F4-INT-01).
//!
//! Owns playback_state and the 500ms polling loop internally so that
//! position updates only re-render TransportBar, not the full App tree.
//!
//! Props received from App(): mode, session_state, intent_open, intent_closing

use crate::components::{
    ab_toggle::{AbToggle, AbToggleState},
    timecode::TimecodeDisplay,
    transport_button::{LedColor, SkipActuator, TransportActuator},
};
use crate::state::cockpit_event::CockpitEvent;
use crate::state::reducer::dispatch;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, JiniPersonaState};
use crate::state::hangar_interview::HangarInterviewState;
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

// ── Types (moved from app.rs) ─────────────────────────────────────────────────

#[derive(PartialEq, Clone, Copy)]
enum TransportState {
    Playing,
    Stopped,
}

#[derive(Clone, Copy, PartialEq)]
enum AbortState {
    IdleClosed,
    Armed,
    Triggered,
    Cooldown,
}

// ── Helpers (moved from app.rs) ───────────────────────────────────────────────

/// Scrub fill % (0.0–100.0)
fn scrub_pct(position_ms: u64, duration_ms: u64) -> f64 {
    if duration_ms == 0 {
        0.0
    } else {
        (position_ms as f64 / duration_ms as f64 * 100.0).clamp(0.0, 100.0)
    }
}

// ── IPC (fire-and-forget, moved from app.rs) ──────────────────────────────────

async fn invoke_playback(
    action: &'static str,
    position_ms: Option<u64>,
    playback_state: Signal<Option<PlaybackStateJson>>,
) {
    let args = serde_json::json!({ "action": action, "positionMs": position_ms });
    match crate::ipc::invoke::<Option<PlaybackStateJson>, _>("playback_control", args).await {
        Ok(Some(state)) => {
            playback_state.clone().set(Some(state));
        }
        Ok(None) => {}
        Err(e) => {
            web_sys::console::log_1(&format!("[transport] {action} error: {e}").into());
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct TransportBarProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
    pub tier: Signal<crate::types::CockpitTier>,
    pub intent_open: Signal<bool>,
    pub intent_closing: Signal<bool>,
    pub presentation: crate::state::cockpit_presentation::CockpitPresentation,
    pub hangar_state: Signal<HangarInterviewState>,
    pub jini_persona: Signal<JiniPersonaState>,
    pub dropped_path: Signal<Option<String>>,
    pub last_platform: Signal<Option<String>>,
    pub last_flavour: Signal<Option<String>>,
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn TransportBar(mut props: TransportBarProps) -> Element {
    let mut mode = props.mode;
    let mut session_state = props.session_state;
    let tier = props.tier;
    let mut intent_open = props.intent_open;
    let presentation = &props.presentation;

    // ── Internal signals ─────────────────────────────────────────────────────
    let playback_state: Signal<Option<PlaybackStateJson>> = use_signal(|| None);
    let mut current_state = use_signal(|| TransportState::Stopped);
    let mut abort_state = use_signal(|| AbortState::IdleClosed);
    let mut ab_press_time = use_signal(|| 0u64);

    // ── P12B-004: Position polling — 500ms when in CoachReady ────────────────
    // Scoped here: only TransportBar re-renders on poll (fixes F4-INT-01).
    {
        let mode_poll = mode;

        use_effect(move || {
            let playback_state = playback_state;
            let mode_poll = mode_poll;

            spawn_local(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(500).await;

                    // Only poll when in FM5 (CoachReady)
                    if !matches!(*mode_poll.read(), CockpitMode::CoachReady { .. }) {
                        continue;
                    }

                    if let Ok(Some(state)) =
                        crate::ipc::invoke_no_args::<Option<PlaybackStateJson>>(
                            "get_playback_state",
                        )
                        .await
                    {
                        playback_state.clone().set(Some(state));
                    }
                }
            });
        });
    }

    // ── Computed (subscribed to TransportBar only after extraction) ───────────
    // NOTE: These reads are inside TransportBar — not App() — so only this
    // component re-renders on playback_state changes. Fixes F4-INT-01.
    let position_ms = playback_state
        .read()
        .as_ref()
        .map(|s| s.position_ms)
        .unwrap_or(0);
    let duration_ms = playback_state
        .read()
        .as_ref()
        .map(|s| s.duration_ms)
        .unwrap_or(0);
    let scrub_len = scrub_pct(position_ms, duration_ms);

    let ab_state_derived = playback_state
        .read()
        .as_ref()
        .map(|s| match s.active_ab.as_str() {
            "A" => AbToggleState::A,
            _ => AbToggleState::B,
        })
        .unwrap_or(AbToggleState::B);

    // Define labels outside rsx! to prevent editor syntax highlighter errors with '<' and '>'
    let lbl_skip_back = "<<".to_string();
    let lbl_skip_fwd = ">>".to_string();

    let chain = session_state
        .read()
        .as_ref()
        .and_then(|s| s.dsp_chain.clone())
        .unwrap_or_default();

    rsx! {
        footer { id: "transport-bar", class: "transport-bar",
            div { class: "chassis-bezel transport-rim",

                div { class: "transport-panel oled-glass-surface",
                    // Left: Annunciator Zone
                    div { class: "transport-left dsp-annunciators-oled",
                        // Session State Annunciator
                        div { class: "fm0-zone session-annunciator-oled",
                            {
                                let session = session_state.read();
                                let mode_val = mode.read();

                                // Extract filename if available
                                let filename = match &*mode_val {
                                    crate::state::cockpit_mode::CockpitMode::FileLoaded { name, .. } => Some(name.clone()),
                                    crate::state::cockpit_mode::CockpitMode::PresetSelected { name, .. } => Some(name.clone()),
                                    crate::state::cockpit_mode::CockpitMode::Mastering { path, .. } => std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()),
                                    _ => None,
                                };

                                match session.as_ref() {
                                    Some(s) => {
                                        let lufs = s.loudness.integrated_lufs;
                                        let blob_short = if s.blob_id.len() >= 8 { &s.blob_id[..8] } else { &s.blob_id };
                                        let disp_name = filename.unwrap_or_else(|| "SESSION ACTIVE".to_string());
                                        rsx! {
                                            div { class: "session-ann-row",
                                                span { class: "session-ann-dot certified" }
                                                span { class: "session-ann-id", "{blob_short}" }
                                            }
                                            div { class: "session-ann-row",
                                                span { class: "session-ann-lufs",
                                                    { format!("{:.1} LUFS", lufs) }
                                                }
                                                span { class: "session-ann-idle", style: "margin-left: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 60px;", "{disp_name}" }
                                            }
                                        }
                                    }
                                    None => {
                                        let disp_name = filename.unwrap_or_else(|| "NO FILE".to_string());
                                        rsx! {
                                            div { class: "session-ann-row",
                                                span { class: "session-ann-dot idle" }
                                                span { class: "session-ann-idle", "{disp_name}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "dsp-chassis-oled",
                            div { class: "dsp-slots-wrapper-oled",
                                div { class: if chain.eq_active    { "oled-annunciator active" } else { "oled-annunciator" }, "EQ" }
                                div { class: if chain.comp_active  { "oled-annunciator active" } else { "oled-annunciator" }, "COMP" }
                                div { class: if chain.sat_active   { "oled-annunciator active" } else { "oled-annunciator" }, "SAT" }
                                div { class: if chain.limit_active { "oled-annunciator active" } else { "oled-annunciator" }, "LIMIT" }
                            }
                        }
                    }

                    // Center: Transport & Time
                    div { class: "transport-center",
                        div { class: "insert-panel",
                            div { class: "transport-mfd-pit",
                                div { class: "control-housing",
                                    div { class: "mfd-controls-row",
                                        // SKIP BACK — visible at all tiers (ADR-C0.3)
                                        div { class: "transport-btn-col",
                                            div { class: "transport-top-label", "–5" }
                                            div { class: "button-base-seat-narrow",
                                                SkipActuator {
                                                    label: "{lbl_skip_back}",
                                                    on_click: move |_| {
                                                        let new_ms = position_ms.saturating_sub(5_000);
                                                        let ps = playback_state;
                                                        spawn_local(async move {
                                                            invoke_playback("seek", Some(new_ms), ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }

                                        // Scrub Knob — visible at all tiers (ADR-C0.2)
                                        div {
                                            class: "scrub-knob-assembly",
                                            id: "transport-scrub",
                                            onclick: move |evt| {
                                                // ADR-C0.2: scrub input gated at Tier2+ (display-only at Tier1)
                                                if *tier.read() < crate::types::CockpitTier::Tier2_Medium { return; }
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
                                                        let ps = playback_state;
                                                        spawn_local(async move {
                                                            invoke_playback("seek", Some(seek_ms), ps).await;
                                                        });
                                                    }
                                                },
                                                div { class: "transport-top-label", "SCRUB" }
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
                                                                        "#ffe699"
                                                                    } else if is_active {
                                                                        "#ffb703"
                                                                    } else {
                                                                        "#8a6311"
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

                                        // SKIP FORWARD — visible at all tiers (ADR-C0.3)
                                        div { class: "transport-btn-col",
                                            div { class: "transport-top-label", "+5" }
                                            div { class: "button-base-seat-narrow",
                                                SkipActuator {
                                                    label: "{lbl_skip_fwd}",
                                                    on_click: move |_| {
                                                        let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                                                        let ps = playback_state;
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
                                                        let ps = playback_state;
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
                                                        let ps = playback_state;
                                                        spawn_local(async move {
                                                            invoke_playback("play", None, ps).await;
                                                        });
                                                    },
                                                }
                                            }
                                        }

                                        // A/B Toggle — visible at all tiers (ADR-C0.4)
                                        div { class: "transport-btn-col",
                                            div { class: "transport-top-label", "A/B" }
                                            div { class: "ab-toggle-seat",
                                                AbToggle {
                                                    state: ab_state_derived.clone(),
                                                    on_mousedown: move |_| {
                                                        let now = js_sys::Date::now() as u64;
                                                        ab_press_time.set(now);
                                                    },
                                                    on_mouseup: move |_| {
                                                        let press_duration = js_sys::Date::now() as u64
                                                            - *ab_press_time.read();
                                                        if press_duration >= 300 {
                                                            // long press — return to A
                                                            let ps = playback_state;
                                                            spawn_local(async move {
                                                                invoke_playback("ab_a", None, ps).await;
                                                            });
                                                        }
                                                    },
                                                    on_click: move |_| {
                                                        let next = match ab_state_derived {
                                                            AbToggleState::A       => AbToggleState::B,
                                                            AbToggleState::B       => AbToggleState::A,
                                                            AbToggleState::Toggled => AbToggleState::A,
                                                        };
                                                        // Wire to M0 playback
                                                        let action = match &next {
                                                            AbToggleState::A | AbToggleState::Toggled => "ab_a",
                                                            AbToggleState::B => "ab_b",
                                                        };
                                                        let ps = playback_state;
                                                        spawn_local(async move {
                                                            invoke_playback(action, None, ps).await;
                                                        });
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Right: Critical Zone
                    div { class: "transport-right abort-zone",
                        // HA — visible at Tier2+ (ADR-C0.5, via CockpitPresentation)
                        if presentation.show_ha_button {
                            button {
                                class: "btn-pill btn-pill-red",
                                onclick: move |_| {
                                    let current = *intent_open.read();
                                    intent_open.set(!current);
                                },
                                "HA"
                            }
                        }

                        // ABORT — OLED Touchscreen Button
                        button {
                            class: match *abort_state.read() {
                                AbortState::IdleClosed => "oled-abort-btn idle",
                                AbortState::Armed      => "oled-abort-btn armed",
                                AbortState::Triggered  => "oled-abort-btn triggered",
                                AbortState::Cooldown   => "oled-abort-btn cooldown",
                            },
                            onclick: move |_| {
                                let current = *abort_state.read();
                                match current {
                                    AbortState::IdleClosed => {
                                        abort_state.set(AbortState::Armed);
                                        let mut state_clone = abort_state;
                                        spawn_local(async move {
                                            gloo_timers::future::TimeoutFuture::new(3_000).await;
                                            if *state_clone.read() == AbortState::Armed {
                                                state_clone.set(AbortState::IdleClosed);
                                            }
                                        });
                                    },
                                    AbortState::Armed => {
                                        dispatch(props.mode, CockpitEvent::BackToIdle);
                                        props.hangar_state.set(HangarInterviewState::AwaitingDrop);
                                        props.session_state.set(None);
                                        props.dropped_path.set(None);
                                        props.last_platform.set(None);
                                        props.last_flavour.set(None);
                                        
                                        // Reset persona to default
                                        props.jini_persona.set(JiniPersonaState::Intermediate);

                                        abort_state.set(AbortState::Triggered);
                                        spawn_local(async move {
                                            gloo_timers::future::TimeoutFuture::new(4_000).await;
                                            abort_state.set(AbortState::Cooldown);
                                            gloo_timers::future::TimeoutFuture::new(200).await;
                                            abort_state.set(AbortState::IdleClosed);
                                        });
                                    },
                                    _ => {}
                                }
                            },
                            match *abort_state.read() {
                                AbortState::IdleClosed => "ABORT",
                                AbortState::Armed      => "ARMED",
                                AbortState::Triggered  => "ABORTED",
                                AbortState::Cooldown   => "",
                            }
                        }
                    }
                }
            }
        }
    }
}
