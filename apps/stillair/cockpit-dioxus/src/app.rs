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
use crate::types::{PlaybackStateJson, SessionStateJson};
use crate::panels::{
    coach::CoachPanel,
    insights::InsightsPanel,
    session::SessionPanel,
};

// Inline the CSS design system at compile time
const STYLES: &str = include_str!("../assets/styles.css");

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
    let mode          = use_signal(|| CockpitMode::Idle);
    let session_state = use_signal(|| None::<SessionStateJson>);
    let playback_state: Signal<Option<PlaybackStateJson>> = use_signal(|| None);

    // ── Mode badge style  ─────────────────────────────────────────────────────
    let mode_label = mode.read().label().to_string();

    let badge_style = {
        let m = mode.read();
        match &*m {
            CockpitMode::Mastering { .. }  =>
                "color:var(--state-running); border-color:var(--state-running);\
                 box-shadow:0 0 6px rgba(0,209,255,0.25);",
            CockpitMode::CoachReady { .. } =>
                "color:var(--state-complete); border-color:var(--state-complete);\
                 box-shadow:0 0 6px rgba(255,214,10,0.2);",
            CockpitMode::Fault { .. }      =>
                "color:var(--state-fault); border-color:var(--state-fault);\
                 box-shadow:0 0 6px rgba(239,68,68,0.25);",
            CockpitMode::Exporting { .. }  =>
                "color:var(--state-locked); border-color:var(--state-locked);",
            _                              => "",
        }
    };

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
        style { "{STYLES}" }

        div {
            id:    "app-shell",
            class: "app-shell",

            // ── Header ────────────────────────────────────────────────────────
            header {
                id:    "cockpit-header",
                class: "cockpit-header",

                span { class: "wordmark", "STILL AIR" }
                span {
                    class: "mode-badge",
                    style: "{badge_style}",
                    "{mode_label}"
                }
            }

            // ── MFD bay — 3 equal panels ──────────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "mfd-bay",

                SessionPanel  { mode, session_state }
                InsightsPanel { mode, session_state, playback_state }
                CoachPanel    { mode, session_state }
            }

            // ── Transport bar (bottom strip) — Phase 12B ──────────────────────
            footer {
                id:    "transport-bar",
                class: "transport-bar",

                // Left: mode indicator + timecode
                div { class: "transport-mode", "{mode_label}" }

                div { class: "transport-divider" }

                // Position display MM:SS
                div {
                    class: "transport-time",
                    id:    "transport-position",
                    { format_ms(position_ms) }
                }

                // Center: controls cluster
                div {
                    class: "transport-controls",

                    // Skip back −5s
                    button {
                        class:   "transport-skip",
                        id:      "btn-skip-back",
                        title:   "Skip back 5 seconds",
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        onclick: move |_| {
                            let new_ms = position_ms.saturating_sub(5_000);
                            let ps     = playback_state.clone();
                            spawn_local(async move {
                                invoke_playback("seek",
                                    Some(new_ms), ps).await;
                            });
                        },
                        "◄◄ 5s"
                    }

                    // Play / Pause toggle
                    button {
                        class: if is_playing { "transport-play is-playing" } else { "transport-play" },
                        id:    "btn-play-pause",
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        onclick: move |_| {
                            let action = if is_playing { "pause" } else { "play" };
                            let ps     = playback_state.clone();
                            spawn_local(async move {
                                invoke_playback(action, None, ps).await;
                            });
                        },
                        {
                            if is_playing { "▐▐  PAUSE" } else { "▶  PLAY" }
                        }
                    }

                    // Skip forward +5s
                    button {
                        class:   "transport-skip",
                        id:      "btn-skip-fwd",
                        title:   "Skip forward 5 seconds",
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        onclick: move |_| {
                            let new_ms = position_ms.saturating_add(5_000).min(duration_ms);
                            let ps     = playback_state.clone();
                            spawn_local(async move {
                                invoke_playback("seek", Some(new_ms), ps).await;
                            });
                        },
                        "5s ►►"
                    }

                    // Scrub rail — click maps to seek position
                    div {
                        class: "transport-scrub",
                        id:    "transport-scrub",
                        title: "Click to seek",
                        // onclick MUST be before children (Dioxus RSX rule)
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

                        // Track + fill
                        div { class: "transport-scrub-track",
                            div {
                                class: "transport-scrub-fill",
                                style: format!("width:{}%", scrub_len),
                            }
                        }
                        // Scrub head
                        div {
                            class: "transport-scrub-head",
                            style: format!("left:{}%", scrub_len),
                        }
                        div {
                            class: "transport-scrub-label",
                            "●SCRUB●"
                        }
                    }

                    // Stop button
                    button {
                        class:    "transport-skip",
                        id:       "btn-stop",
                        disabled: !matches!(*mode.read(), CockpitMode::CoachReady { .. }),
                        onclick: move |_| {
                            let ps = playback_state.clone();
                            spawn_local(async move {
                                invoke_playback("stop", None, ps).await;
                            });
                        },
                        "■ STOP"
                    }
                }

                // Duration display MM:SS
                div {
                    class: "transport-duration",
                    id:    "transport-duration",
                    { format!("/ {}", format_ms(duration_ms)) }
                }
            }
        }
    }
}
