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
use crate::types::{SessionStateJson, VisualizationDataJson};
use crate::panels::{
    coach::CoachPanel,
    mastered::MasteredView,
};
use crate::components::sampling_siamese::SamplingSiamese;
use crate::components::intent_bay::IntentBay;
use crate::components::transport_bar::TransportBar;



// ── App root ──────────────────────────────────────────────────────────────────

#[allow(non_snake_case)]
pub fn App() -> Element {
    // ── Signals ──────────────────────────────────────────────────────────────
    let mode           = use_signal(|| CockpitMode::Idle);
    let session_state  = use_signal(|| None::<SessionStateJson>);
    let viz_data: Signal<Option<VisualizationDataJson>> = use_signal(|| None);
    // MasteredView overlay visibility (Signal only — no IPC per §5.3)
    let mut show_mastered: Signal<bool> = use_signal(|| false);
    let mut pdf_preview_ctx = use_context_provider(|| Signal::new(None::<String>));
    let mut intent_open:    Signal<bool> = use_signal(|| false);
    let mut intent_closing: Signal<bool> = use_signal(|| false);
    let tone_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let dyn_angle: Signal<f32>  = use_signal(|| 0.0_f32);
    let space_angle: Signal<f32> = use_signal(|| 0.0_f32);
    let loud_angle: Signal<f32> = use_signal(|| 0.0_f32);

    rsx! {

        div { id: "app-shell", class: "app-shell",

            // ── Transport bar (bottom strip) ──────────────────────────────────
            // playback_state + 500ms poll live inside TransportBar (F4-INT-01):
            // only the footer re-renders on position updates, not the full tree.
            TransportBar {
                mode,
                session_state,
                intent_open,
                intent_closing,
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
                div { class: "coach-panel chassis-bezel",
                    CoachPanel { mode, session_state }
                }
            }
        }
    }
}
