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
    session::SessionPanel,
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

pub fn App() -> Element {
    // ── Signals ──────────────────────────────────────────────────────────────
    let mode           = use_signal(|| CockpitMode::Idle);
    let session_state  = use_signal(|| None::<SessionStateJson>);
    let viz_data: Signal<Option<VisualizationDataJson>> = use_signal(|| None);
    let playback_state: Signal<Option<PlaybackStateJson>> = use_signal(|| None);
    // MasteredView overlay visibility (Signal only — no IPC per §5.3)
    let mut show_mastered: Signal<bool> = use_signal(|| false);

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

            // ── MFD bay — 3 equal panels ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "mfd-bay",

                SessionPanel  { mode, session_state, viz_data, show_mastered }
                InsightsPanel { mode, session_state, playback_state, viz_data }
                CoachPanel    { mode, session_state }
            }

            // ── MasteredView overlay (P14-011) — conditional on show_mastered ──
            if *show_mastered.read() {
                MasteredView {
                    session_state,
                    viz_data,
                    on_close: move |_| { show_mastered.set(false); },
                }
            }

            // ── Transport bar (bottom strip) — Phase 12B ──────────────────────
            footer {
                id:    "transport-bar",
                class: "transport-bar",

                Screw { top: 10, left: 10 }
                Screw { top: 10, right: 10 }
                Screw { bottom: 10, left: 10 }
                Screw { bottom: 10, right: 10 }

                // Left: Annunciator Zone (25% width)
                div {
                    class: "transport-left dsp-annunciators",
                    Annunciator { label: "FM0".to_string(), is_master: true, active: true } // Master is active for now
                    Annunciator { label: "EQ".to_string(), active: false }
                    Annunciator { label: "COMP".to_string(), active: false }
                    Annunciator { label: "SAT".to_string(), active: false }
                    Annunciator { label: "LIMIT".to_string(), active: false }
                }

                // Center: Chrono-Control Zone (50% width)
                div {
                    class: "transport-center",

                    // Top: Controls & Time
                    div {
                        class: "transport-center-top",

                        // Skip back −5s
                        SoftKey {
                            label: "◄◄".to_string(),
                            title: "Skip back 5 seconds".to_string(),
                            disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                            variant: SoftKeyVariant::Standard,
                            onclick: move |_| {
                                let new_ms = position_ms.saturating_sub(5_000);
                                let ps     = playback_state.clone();
                                spawn_local(async move {
                                    invoke_playback("seek", Some(new_ms), ps).await;
                                });
                            }
                        }

                        // Time Counter (Now VFD Styled and in the center)
                        div {
                            class: "transport-vfd-display transport-time",
                            id:    "transport-position",
                            { format_ms(position_ms) }
                        }

                        // Skip forward +5s
                        SoftKey {
                            label: "►►".to_string(),
                            title: "Skip forward 5 seconds".to_string(),
                            disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                            variant: SoftKeyVariant::Standard,
                            onclick: move |_| {
                                let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                                let ps     = playback_state.clone();
                                spawn_local(async move {
                                    invoke_playback("seek", Some(new_ms), ps).await;
                                });
                            }
                        }

                        // Play / Pause toggle
                        SoftKey {
                            label: "PLAY".to_string(),
                            active: is_playing,
                            disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                            variant: SoftKeyVariant::Active,
                            onclick: move |_| {
                                let action = if is_playing { "pause" } else { "play" };
                                let ps     = playback_state.clone();
                                spawn_local(async move {
                                    invoke_playback(action, None, ps).await;
                                });
                            }
                        }

                        // Stop button
                        SoftKey {
                            label: "STOP".to_string(),
                            disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                            variant: SoftKeyVariant::Standard,
                            onclick: move |_| {
                                let ps = playback_state.clone();
                                spawn_local(async move {
                                    invoke_playback("stop", None, ps).await;
                                });
                            }
                        }
                    }

                    // Bottom: Scrub rail
                    div {
                        class: "transport-scrub",
                        id:    "transport-scrub",
                        title: "Click to seek",
                        onclick: move |evt| {
                            if !matches!(*mode.read(), CockpitMode::CoachReady { .. })
                                || duration_ms == 0 { return; }
                            let client_x = evt.client_coordinates().x;
                            let window   = web_sys::window().unwrap();
                            let doc      = window.document().unwrap();
                            let el       = doc.get_element_by_id("transport-scrub");
                            if let Some(el) = el {
                                let rect  = el.get_bounding_client_rect();
                                let frac  = ((client_x - rect.left()) / rect.width())
                                               .clamp(0.0, 1.0);
                                let seek_ms = (frac * duration_ms as f64) as u64;
                                let ps      = playback_state.clone();
                                spawn_local(async move {
                                    invoke_playback("seek", Some(seek_ms), ps).await;
                                });
                            }
                        },

                        div { class: "transport-scrub-track",
                            div {
                                class: "transport-scrub-fill",
                                style: format!("width:{}%", scrub_len),
                            }
                        }
                        div {
                            class: "transport-scrub-head",
                            style: format!("left:{}%", scrub_len),
                        }
                        div {
                            class: "transport-scrub-label",
                            "●SCRUB●"
                        }
                    }
                }

                // Right: Critical Zone (25% width)
                div {
                    class: "transport-right abort-zone",
                    SoftKey {
                        label: "ABORT".to_string(),
                        variant: SoftKeyVariant::Danger,
                        is_guarded: true,
                        onclick: move |_| {
                            web_sys::console::warn_1(&"Avionics ABORT trigger activated".into());
                        }
                    }
                }
            }
        }
    }
}
