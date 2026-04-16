//! app.rs — Cockpit root component. P11-004 / P11-008
//! Authority: Phase 11 task-decomposition · state-machine.md §2
//!
//! Layout:
//!   ┌─────────────────────────────────────────────┐
//!   │  HEADER: STILL AIR wordmark + mode badge    │  44px
//!   ├─────────────┬──────────────┬────────────────┤
//!   │  SESSION    │   INSIGHTS   │   COACH        │  flex:1
//!   │  (left MFD) │  (center MFD)│  (right MFD)   │
//!   ├─────────────┴──────────────┴────────────────┤
//!   │  TRANSPORT BAR: mode display + buttons      │  52px
//!   └─────────────────────────────────────────────┘
//!
//! Amendment A-002 §2: no business logic here — IPC only.
//! Amendment A-002 §3: no core imports.

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::SessionStateJson;
use crate::panels::{
    coach::CoachPanel,
    insights::InsightsPanel,
    session::SessionPanel,
};

// Inline the CSS design system at compile time
const STYLES: &str = include_str!("../assets/styles.css");

pub fn App() -> Element {
    // ── Signals ──────────────────────────────────────────────────────────────
    // Dioxus 0.6 signals are Copy — they can be freely passed to child components.
    // session_state is written by SessionPanel inside spawn(async move { ... }),
    // where Signal::set() is available via the Writable trait.
    let mode          = use_signal(|| CockpitMode::Idle);
    let session_state = use_signal(|| None::<SessionStateJson>);

    let mode_label = mode.read().label().to_string();

    // Mode-based style for the badge
    let badge_style = {
        let m = mode.read();
        match &*m {
            CockpitMode::Mastering { .. }  =>
                "color:var(--state-running); border-color:var(--state-running);",
            CockpitMode::CoachReady { .. } =>
                "color:var(--state-complete); border-color:var(--state-complete);",
            CockpitMode::Fault { .. }      =>
                "color:var(--state-fault); border-color:var(--state-fault);",
            CockpitMode::Exporting { .. }  =>
                "color:var(--state-locked); border-color:var(--state-locked);",
            _                              => "",
        }
    };

    rsx! {
        // Inject design system CSS
        style { "{STYLES}" }

        // Chassis shell
        div {
            id:    "app-shell",
            class: "app-shell",

            // ── Header ────────────────────────────────────────────────────
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

            // ── MFD bay — 3 equal panels ──────────────────────────────────
            main {
                id:    "mfd-bay",
                class: "mfd-bay",

                SessionPanel {
                    mode,
                    session_state,
                }
                InsightsPanel {
                    mode,
                    session_state,
                }
                CoachPanel {
                    mode,
                    session_state,
                }
            }

            // ── Transport bar (bottom strip) ──────────────────────────────
            // Phase 11: transport controls are Phase 12 (A-003 §3 — no live audio).
            footer {
                id:    "transport-bar",
                class: "transport-bar",

                div {
                    class: "transport-mode",
                    "{mode_label}"
                }

                div { class: "transport-spacer" }

                div {
                    class: "transport-group",
                    span {
                        style: "font-family:var(--font-mono); font-size:var(--text-xs);
                                color:var(--text-muted); letter-spacing:var(--tracking-wider);",
                        "TRANSPORT ·  PHASE 12"
                    }
                }
            }
        }
    }
}
