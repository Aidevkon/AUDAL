//! panels/mastered.rs — MasteredView overlay · Phase 14 P14-011
//! Authority: UI Agent Context v2.1 §5 · Amendment A-002 §3
//!
//! Post-session analysis room. Read-only. Engineer reviews BEFORE/AFTER.
//!
//! Laws:
//!   - No business logic
//!   - No SVG path computation — waveform paths from viz signal (§5.4)
//!   - No inline hex colors — CSS variables only
//!   - ViewMode signal = UI-only state, no IPC (§5.3)
//!
//! Phase 14: Waveform paths are placeholders from viz.waveform_*_svg.
//! Phase 15: Real before/after waveforms from PCM cache (A-003 §11).

use dioxus::prelude::*;
use serde_json::json;
use wasm_bindgen_futures::spawn_local;

use crate::ipc::invoke;
use crate::types::{SessionStateJson, VisualizationDataJson};

/// View mode — UI-only signal, no IPC. (§5.3)
#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    Overlay,
    Split,
    Dual,
}

/// MasteredView — post-session read-only overlay.
///
/// Displays BEFORE/AFTER waveforms, quality gate readout, and compliance LEDs.
/// Export bar at bottom for invoking exportAudio.
/// Shown as a full-screen overlay (position: fixed).
#[component]
pub fn MasteredView(
    session_state: Signal<Option<SessionStateJson>>,
    viz_data: Signal<Option<VisualizationDataJson>>,
    on_close: EventHandler<()>,
) -> Element {
    // ViewMode — UI signal only, no IPC (§5.3)
    let mut view_mode: Signal<ViewMode> = use_signal(|| ViewMode::Split);

    let session = session_state.read();
    let viz = viz_data.read();

    let before_wave = viz
        .as_ref()
        .map(|v| v.waveform_before_svg.clone())
        .unwrap_or_default();
    let after_wave = viz
        .as_ref()
        .map(|v| v.waveform_after_svg.clone())
        .unwrap_or_default();

    let before_path = before_wave;
    let after_path = after_wave;

    rsx! {
        div {
            class: "mastered-view",

            // ── Sidebar ─────────────────────────────────────────────────────────
            div {
                class: "mastered-sidebar",

                div {
                    class: "mastered-sidebar-title",
                    "MASTERED VIEW"
                }

                div {
                    class: "mastered-nav-item active",
                    "WAVEFORM"
                }
                div {
                    class: "mastered-nav-item",
                    "LOUDNESS"
                }
                div {
                    class: "mastered-nav-item",
                    "COMPLIANCE"
                }
                div {
                    class: "mastered-nav-item",
                    "EXPORT"
                }

                // Back / Close button
                div {
                    style: "margin-top:auto;",
                    button {
                        id: "btn-close-mastered",
                        onclick: move |_| on_close.call(()),
                        style: "background:transparent; border:1px solid var(--border-panel);\
                                color:var(--text-secondary); font-family:var(--font-mono);\
                                font-size:0.5rem; letter-spacing:0.15em; text-transform:uppercase;\
                                padding:6px 12px; border-radius:3px; cursor:pointer;\
                                width:100%;",
                        "← BACK"
                    }
                }
            }

            // ── Main content ─────────────────────────────────────────────────────
            div {
                class: "mastered-main",

                // View mode selector (§5.3)
                div {
                    class: "mastered-view-modes",
                    for (label, vm) in [
                        ("OVERLAY", ViewMode::Overlay),
                        ("SPLIT",   ViewMode::Split),
                        ("DUAL",    ViewMode::Dual),
                    ] {
                        {
                            let vm2 = vm.clone();
                            let active_class = if *view_mode.read() == vm {
                                "view-mode-btn active"
                            } else {
                                "view-mode-btn"
                            };
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: "{active_class}",
                                    onclick: move |_| { view_mode.set(vm2.clone()); },
                                    "{label}"
                                }
                            }
                        }
                    }
                }

                // ── Waveform displays (§5.4) ──────────────────────────────────
                WaveformDisplay { path: before_path.clone(), label: "BEFORE" }
                WaveformDisplay { path: after_path.clone(),  label: "AFTER" }

                // ── Quality Gate readout (§5.5) ──────────────────────────────
                match session.as_ref() {
                    None => rsx! { div { class: "awaiting", "NO SESSION" } },
                    Some(s) => rsx! {
                        QualityGatePanel { session: s.clone() }
                        CompliancePanel  { session: s.clone() }
                    }
                }
            }

            // ── Export bar ───────────────────────────────────────────────────────
            div {
                class: "mastered-export-bar",

                // Export buttons — invoke exportAudio
                for (fmt, label) in [("wav","WAV"),("flac","FLAC"),("mp3","MP3"),("aiff","AIFF")] {
                    {
                        let blob_id = session.as_ref()
                            .map(|s| s.blob_id.clone())
                            .unwrap_or_default();
                        rsx! {
                            button {
                                key:     "{fmt}",
                                id:      "btn-export-{fmt}",
                                class:   "transport-btn export-btn",
                                disabled: blob_id.is_empty(),
                                onclick: move |_| {
                                    let b = blob_id.clone();
                                    spawn_local(async move {
                                        let _ = invoke::<String, _>(
                                            "export_audio",
                                            json!({ "blobId": b, "format": fmt }),
                                        ).await;
                                    });
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── WaveformDisplay (§5.4) ────────────────────────────────────────────────────

/// Renders a precomputed waveform SVG path. Spec: §5.4.
///
/// Receives SVG path string from viz signal — renders only.
/// No computation. 800×120 viewBox.
#[component]
fn WaveformDisplay(path: String, label: &'static str) -> Element {
    rsx! {
        div {
            class: "waveform-container",

            div {
                class: "waveform-label",
                "{label}"
            }

            svg {
                class:    "waveform-svg",
                view_box: "0 0 800 120",

                // Grid lines
                WaveformGrid {}

                path {
                    d:            "{path}",
                    fill:         "none",
                    stroke:       "var(--accent-cyan)",
                    stroke_width: "1.5",
                }
            }
        }
    }
}

/// Waveform horizontal grid lines.
#[component]
fn WaveformGrid() -> Element {
    rsx! {
        g {
            opacity: "0.06",
            stroke:  "var(--accent-cyan)",
            stroke_width: "0.8",
            // Center line + quarter lines
            line { x1: "0", y1: "60",  x2: "800", y2: "60"  }
            line { x1: "0", y1: "30",  x2: "800", y2: "30"  }
            line { x1: "0", y1: "90",  x2: "800", y2: "90"  }
            line { x1: "0", y1: "15",  x2: "800", y2: "15"  }
            line { x1: "0", y1: "105", x2: "800", y2: "105" }
        }
    }
}

// ── QualityGatePanel (§5.5) ───────────────────────────────────────────────────

/// Large monospace readout of key loudness metrics. Spec: §5.5.
#[component]
fn QualityGatePanel(session: SessionStateJson) -> Element {
    rsx! {
        div {
            class: "quality-gate-panel",

            div {
                class: "quality-gate-metric",
                div { class: "quality-gate-label", "INTEGRATED LUFS" }
                div { class: "quality-gate-value",
                    { format!("{:.1}", session.loudness.integrated_lufs) }
                }
            }
            div {
                class: "quality-gate-metric",
                div { class: "quality-gate-label", "TRUE PEAK" }
                div { class: "quality-gate-value",
                    { format!("{:.1}", session.loudness.true_peak_dbtp) }
                }
            }
            div {
                class: "quality-gate-metric",
                div { class: "quality-gate-label", "LOUDNESS RANGE" }
                div { class: "quality-gate-value",
                    { format!("{:.1}", session.loudness.lra) }
                }
            }
            div {
                class: "quality-gate-metric",
                div { class: "quality-gate-label", "SHORT TERM" }
                div { class: "quality-gate-value",
                    { format!("{:.1}", session.loudness.short_term_lufs) }
                }
            }
        }
    }
}

// ── CompliancePanel (§5.5) ────────────────────────────────────────────────────

/// Platform compliance LED dots. Spec: §5.5.
///
/// Cyan active = compliant. Dim inactive = not compliant.
#[component]
fn CompliancePanel(session: SessionStateJson) -> Element {
    let platforms = [
        ("SPOTIFY", session.compliance.spotify),
        ("YOUTUBE", session.compliance.youtube),
        ("APPLE", session.compliance.apple),
        ("TIDAL", session.compliance.tidal),
        ("BROADCAST", session.compliance.broadcast),
        ("EBU R128", session.compliance.ebu_r128),
    ];

    rsx! {
        div {
            class: "compliance-panel",
            for (name, compliant) in platforms {
                div {
                    key:   "{name}",
                    class: "compliance-led",
                    div {
                        class: if compliant { "compliance-led-dot active" }
                               else { "compliance-led-dot inactive" },
                    }
                    div {
                        class: "compliance-led-label",
                        "{name}"
                    }
                }
            }
        }
    }
}
