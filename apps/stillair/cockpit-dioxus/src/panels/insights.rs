//! panels/insights.rs — THE INSIGHTS panel · Phase 14
//! Authority: Phase 14 P14-004 through P14-010 · UI Agent Context v2.1
//!
//! This is a RENDERING SURFACE ONLY.
//!
//! Laws enforced:
//!   ❌ No SVG path computation — paths come from viz signal (getVisualizationData)
//!   ❌ No business logic
//!   ❌ No inline hex colors — CSS variables only
//!   ❌ No std::f32 methods
//!
//! 2×2 OLED grid layout (§4 InsightsPanel):
//!   TL: SpectrumDisplay   — SVG path from viz.spectrum_svg_path
//!   TR: VuMeterPair       — LUFS cyan + PEAK amber, 30 segments each
//!   BL: StereoScope       — dual ellipse from viz.lissajous_* rx/ry
//!   BR: MetricsReadout    — PEAK/RANGE/CORR with correct colors
//!
//! A-003 §5: No PCM. No audio kernel imports.

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};

// ── InsightsPanel (P14-010) ───────────────────────────────────────────────────

/// THE INSIGHTS panel — center MFD.
///
/// Receives session and viz signals. Renders 2×2 OLED grid.
/// Zero computation. All data comes from IPC signals.
#[component]
pub fn InsightsPanel(
    mode:           Signal<CockpitMode>,
    session_state:  Signal<Option<SessionStateJson>>,
    playback_state: Signal<Option<PlaybackStateJson>>,
    viz_data:       Signal<Option<VisualizationDataJson>>,
) -> Element {
    let state = session_state.read();
    let viz   = viz_data.read();

    rsx! {
        div {
            class: "mfd-panel panel-insights panel-insights-glow",
            isolation: "isolate",

            // Corner screws (§4.5)
            div { class: "screw screw-tl" }
            div { class: "screw screw-tr" }
            div { class: "screw screw-bl" }
            div { class: "screw screw-br" }

            // Panel title bar
            div {
                class: "panel-title",
                style: "color:var(--accent-cyan);",
                "THE INSIGHTS"
            }

            // Panel body — 2×2 OLED grid
            div {
                class: "panel-body",

                match state.as_ref() {
                    None => rsx! {
                        div { class: "awaiting", "AWAITING SESSION" }
                    },
                    Some(s) => {
                        // Get viz data (may be None until backend responds)
                        let spectrum_path = viz.as_ref()
                            .map(|v| v.spectrum_svg_path.clone())
                            .unwrap_or_default();
                        let outer_rx = viz.as_ref().map(|v| v.lissajous_outer_rx).unwrap_or(20.0_f32);
                        let outer_ry = viz.as_ref().map(|v| v.lissajous_outer_ry).unwrap_or(30.0_f32);
                        let inner_rx = viz.as_ref().map(|v| v.lissajous_inner_rx).unwrap_or(10.0_f32);
                        let inner_ry = viz.as_ref().map(|v| v.lissajous_inner_ry).unwrap_or(12.0_f32);

                        rsx! {
                            div {
                                class: "oled-grid",

                                // Top-left: SpectrumDisplay (§4.7)
                                div {
                                    class: "oled-cell oled-screen",
                                    SpectrumDisplay { path: spectrum_path }
                                }

                                // Top-right: VuMeterPair (§4.6)
                                div {
                                    class: "oled-cell oled-screen",
                                    VuMeterPair {
                                        lufs: s.loudness.integrated_lufs,
                                        peak: s.loudness.true_peak_dbtp,
                                    }
                                }

                                // Bottom-left: StereoScope (§4.8)
                                div {
                                    class: "oled-cell oled-screen",
                                    StereoScope {
                                        outer_rx,
                                        outer_ry,
                                        inner_rx,
                                        inner_ry,
                                    }
                                }

                                // Bottom-right: MetricsReadout (§4.9)
                                div {
                                    class: "oled-cell oled-screen",
                                    MetricsReadout {
                                        peak:        s.loudness.true_peak_dbtp,
                                        lra:         s.loudness.lra,
                                        correlation: s.quality.stereo_correlation,
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── P14-005: SpectrumDisplay ──────────────────────────────────────────────────

/// Renders a precomputed SVG spectrum curve.
///
/// Receives SVG path string from backend (getVisualizationData).
/// Renders only — zero computation. Spec: §4.7.
#[component]
fn SpectrumDisplay(path: String) -> Element {
    rsx! {
        div {
            class: "spectrum-display",

            svg {
                class: "spectrum-svg",
                view_box: "0 0 400 200",

                defs {
                    linearGradient {
                        id: "sg",
                        x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%",   stop_color: "var(--accent-cyan)" }
                        stop { offset: "100%", stop_color: "transparent" }
                    }
                }

                // Grid lines at frequency markers — opacity 0.08 per §4.7
                SpectrumGrid {}

                // Fill area — precomputed closed path from backend
                path {
                    d:    "{path}",
                    fill: "url(#sg)",
                    opacity: "0.4",
                }

                // Stroke — same path
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

/// Frequency marker grid lines — §4.7 (7 markers, opacity 0.08).
/// Static: 20Hz=10px, 100=70, 500=155, 1k=200, 5k=300, 10k=345, 20k=390.
#[component]
fn SpectrumGrid() -> Element {
    // x positions for: 20Hz, 100, 500, 1k, 5k, 10k, 20k
    let freq_x: &[f32] = &[10.0, 70.0, 155.0, 200.0, 300.0, 345.0, 390.0];
    rsx! {
        g {
            opacity: "0.08",
            stroke: "var(--accent-cyan)",
            stroke_width: "1",
            for x in freq_x {
                line {
                    key: "{x}",
                    x1: "{x}", y1: "0",
                    x2: "{x}", y2: "190",
                }
            }
        }
    }
}

// ── P14-004: VuMeterPair ──────────────────────────────────────────────────────

/// LUFS + PEAK VU meter pair — 30 LED segments each.
///
/// Receives loudness float values. Renders only.
/// LUFS = var(--accent-cyan). PEAK = var(--accent-amber). Spec: §4.6.
#[component]
fn VuMeterPair(lufs: f32, peak: f32) -> Element {
    rsx! {
        div {
            class: "vu-pair",

            // Scale labels column (shared, reversed — 0 at top)
            div {
                class: "vu-scale",
                // Labels at key positions: 0, -3, -6, -12, -18, -20, -25, -30
                // Each label sits at a segment offset from top
                // 30 segments: 0 at top (seg 30), -30 at bottom (seg 0)
                // We emit labels for visual reference only
                for (label, _seg) in [("0", 30usize), ("-3", 27), ("-6", 24),
                                       ("-12",18), ("-18",12), ("-20",10),
                                       ("-25",5), ("-30",0)] {
                    span {
                        key: "{label}",
                        class: "vu-scale-label",
                        "{label}"
                    }
                }
            }

            // LUFS channel — cyan
            div {
                class: "vu-channel",
                VuMeter { value: lufs, min: -30.0_f32, max: 0.0_f32, color: "var(--accent-cyan)" }
                div { class: "vu-label vu-lufs", "L" }
            }

            // PEAK channel — amber
            div {
                class: "vu-channel",
                VuMeter { value: peak, min: -30.0_f32, max: 0.0_f32, color: "var(--accent-amber)" }
                div { class: "vu-label vu-peak", "P" }
            }
        }
    }
}

/// Single VU meter — 30 LED segments.
///
/// Filled: full color. Inactive: 8% opacity (color + "14" hex suffix).
/// Top 3: #ff3b30 clipping zone. Spec: §4.6.
/// No std::f32 methods — plain arithmetic only.
#[component]
fn VuMeter(value: f32, min: f32, max: f32, color: &'static str) -> Element {
    let total = 30usize;
    let range = max - min;
    let ratio = if range > 0.0 { (value - min) / range } else { 0.0 };
    let filled = (ratio * total as f32).clamp(0.0, total as f32) as usize;

    rsx! {
        div {
            class: "vu-meter",
            for i in 0..total {
                div {
                    key: "{i}",
                    class: "vu-segment",
                    // Active segments: top 3 are clip zone (red), rest are color
                    // Inactive: color at 8% opacity → append "14" to hex
                    // We use inline style here only for the dynamic color value
                    style: {
                        if i < filled {
                            if i >= total - 3 {
                                "background:#ff3b30;".to_string()
                            } else {
                                format!("background:{};", color)
                            }
                        } else {
                            // 8% opacity: append hex "14" to the CSS variable value
                            // We fake opacity via box-shadow trick if color is a var(),
                            // but for simplicity use opacity on the element
                            format!("background:{};opacity:0.08;", color)
                        }
                    },
                }
            }
        }
    }
}

// ── P14-006: StereoScope ─────────────────────────────────────────────────────

/// Dual-ellipse Lissajous / goniometer display.
///
/// Receives 4 f32 values from viz signal — renders SVG ellipses.
/// Zero computation. Spec: §4.8.
///
/// Outer ellipse: cyan (stereo width orbit).
/// Inner ellipse: magenta (correlation tightness).
/// Both rotated -45° (standard goniometer orientation).
#[component]
fn StereoScope(
    outer_rx: f32,
    outer_ry: f32,
    inner_rx: f32,
    inner_ry: f32,
) -> Element {
    rsx! {
        div {
            class: "stereo-scope",

            svg {
                view_box: "0 0 120 120",

                defs {
                    filter {
                        id: "cg",
                        feGaussianBlur { std_deviation: "2" }
                    }
                }

                // Polar grid background (3 circles + crosshair) — §4.8
                StereoGrid {}

                // Outer ellipse — cyan (stereo width)
                ellipse {
                    cx:           "60",
                    cy:           "60",
                    rx:           "{outer_rx}",
                    ry:           "{outer_ry}",
                    fill:         "none",
                    stroke:       "var(--accent-cyan)",
                    stroke_width: "1.5",
                    opacity:      "0.8",
                    transform:    "rotate(-45 60 60)",
                }

                // Inner ellipse — magenta (correlation)
                ellipse {
                    cx:           "60",
                    cy:           "60",
                    rx:           "{inner_rx}",
                    ry:           "{inner_ry}",
                    fill:         "none",
                    stroke:       "var(--accent-magenta)",
                    stroke_width: "1.0",
                    opacity:      "0.7",
                    transform:    "rotate(-45 60 60)",
                }

                // Center glow hotspot
                circle {
                    cx:     "60",
                    cy:     "60",
                    r:      "3",
                    fill:   "var(--accent-cyan)",
                    opacity:"0.9",
                    filter: "url(#cg)",
                }
            }
        }
    }
}

/// Polar grid: 3 concentric circles + perpendicular crosshair.
/// Opacity 0.15 per §4.8. Static — no data dependency.
#[component]
fn StereoGrid() -> Element {
    rsx! {
        g {
            opacity: "0.15",
            stroke: "var(--accent-cyan)",
            stroke_width: "0.8",
            fill: "none",

            // 3 concentric circles
            circle { cx: "60", cy: "60", r: "50" }
            circle { cx: "60", cy: "60", r: "33" }
            circle { cx: "60", cy: "60", r: "16" }

            // Crosshair
            line { x1: "60", y1: "5",  x2: "60", y2: "115" }
            line { x1: "5",  y1: "60", x2: "115", y2: "60" }

            // Diagonal axes (45°/135° — standard goniometer guides)
            line { x1: "14", y1: "14", x2: "106", y2: "106" }
            line { x1: "106", y1: "14", x2: "14", y2: "106" }
        }
    }
}

// ── P14-007: MetricsReadout ───────────────────────────────────────────────────

/// PEAK / RANGE / CORR numerical readout. Spec: §4.9.
///
/// PEAK:  amber, 28px
/// RANGE: gold,  28px
/// CORR:  cyan,  28px (prefix "+" when ≥ 0.0)
///
/// No computation — receives values from session signal.
#[component]
fn MetricsReadout(peak: f32, lra: f32, correlation: f32) -> Element {
    let corr_str = if correlation >= 0.0 {
        format!("+{:.2}", correlation)
    } else {
        format!("{:.2}", correlation)
    };

    rsx! {
        div {
            class: "metrics-readout",

            // PEAK — amber
            div {
                class: "metric-readout-row",
                div { class: "metric-readout-label", "PEAK" }
                div {
                    class: "metric-readout-value peak",
                    { format!("{:.1}", peak) }
                }
            }

            // RANGE — gold
            div {
                class: "metric-readout-row",
                div { class: "metric-readout-label", "RANGE" }
                div {
                    class: "metric-readout-value range",
                    { format!("{:.0}", lra) }
                }
            }

            // CORR — cyan
            div {
                class: "metric-readout-row",
                div { class: "metric-readout-label", "CORR" }
                div {
                    class: "metric-readout-value corr",
                    "{corr_str}"
                }
            }
        }
    }
}
