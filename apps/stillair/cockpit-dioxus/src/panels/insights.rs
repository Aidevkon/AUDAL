//! panels/insights.rs — SPATIAL TELEMETRY panel · Phase 14
//! Layout:
//!   ┌────────────────────────────────────┐
//!   │  LISSAJOUS SCOPE  (full width 35%) │
//!   ├────────────────────────────────────┤
//!   │  MID / SIDE / WIDTH / VECTOR  15%  │
//!   ├────────────────────────────────────┤
//!   │  CORRELATION METER           13%   │
//!   ├────────────────────────────────────┤
//!   │  SPATIAL HEAT MAP         flex:1   │
//!   └────────────────────────────────────┘
//! Laws: ❌ No SVG path computation  ❌ No inline hex  ❌ No std::f32 methods
//! M/S fields: demo values until backend amendment.

use dioxus::prelude::*;
use crate::components::module_frame::ModuleFrame;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};

// ── Demo Lissajous paths (FM0 idle) ───────────────────────────────────────────
const DEMO_LISS_OUTER: &str =
    "M 82,60 L 89,74 L 100,94 L 104,110 L 98,116 L 86,110 L 74,94 \
     L 66,74 L 60,60 L 54,46 L 46,26 L 34,10 L 22,4 L 16,10 \
     L 20,26 L 34,46 L 48,60 L 54,74 L 54,94 L 48,110 \
     L 38,116 L 28,108 L 22,92 L 26,74 L 38,60 L 52,46 \
     L 66,26 L 78,10 L 86,4 L 92,10 L 94,26 L 88,46 L 82,60";

const DEMO_LISS_INNER: &str =
    "M 60,60 L 72,73 L 84,78 L 84,66 L 72,47 L 60,38 \
     L 48,47 L 36,66 L 36,78 L 48,73 L 60,60 \
     L 72,47 L 84,42 L 84,54 L 72,73 L 60,82 \
     L 48,73 L 36,54 L 36,42 L 48,47 L 60,60";

const DEMO_LISS_D1: &str =
    "M 60,60 L 81,73 L 90,60 L 81,47 L 60,60 \
     L 39,73 L 30,60 L 39,47 L 60,60 L 75,78 L 60,60 L 45,78 L 60,60";

const DEMO_LISS_D2: &str =
    "M 60,60 L 74,80 L 80,60 L 74,40 L 60,60 \
     L 46,80 L 40,60 L 46,40 L 60,60 L 70,88 L 60,60 L 50,88 L 60,60";

// Demo M/S values — pending backend amendment
const DEMO_MID_PCT:  f32 = 62.0;
const DEMO_SIDE_PCT: f32 = 38.0;

// ── InsightsPanel ─────────────────────────────────────────────────────────────

#[component]
pub fn InsightsPanel(
    mode:           Signal<CockpitMode>,
    session_state:  Signal<Option<SessionStateJson>>,
    playback_state: Signal<Option<PlaybackStateJson>>,
    viz_data:       Signal<Option<VisualizationDataJson>>,
) -> Element {
    let state = session_state.read();
    let viz   = viz_data.read();

    let liss_outer   = viz.as_ref().map(|v| v.lissajous_path_outer.as_str()).unwrap_or(DEMO_LISS_OUTER).to_string();
    let liss_inner   = viz.as_ref().map(|v| v.lissajous_path_inner.as_str()).unwrap_or(DEMO_LISS_INNER).to_string();
    let liss_detail1 = viz.as_ref().map(|v| v.lissajous_path_detail1.as_str()).unwrap_or(DEMO_LISS_D1).to_string();
    let liss_detail2 = viz.as_ref().map(|v| v.lissajous_path_detail2.as_str()).unwrap_or(DEMO_LISS_D2).to_string();

    // Real data where available, demo fallback
    let (correlation, width, phase_coh) = match state.as_ref() {
        Some(s) => (s.quality.stereo_correlation, s.quality.stereo_width, s.quality.phase_coherence),
        None    => (0.21_f32, 0.62_f32, 0.31_f32),
    };

    rsx! {
        ModuleFrame {
            title:        "SPATIAL TELEMETRY".to_string(),
            panel_class:  "panel-insights".to_string(),
            header_style: "color:var(--accent-insights);".to_string(),
            is_scrollable: false,

            div { class: "spatial-body",

                // 1. Lissajous scope
                div { class: "spatial-scope-cell oled-screen",
                    StereoScope {
                        path_outer:   liss_outer,
                        path_inner:   liss_inner,
                        path_detail1: liss_detail1,
                        path_detail2: liss_detail2,
                    }
                }

                // 2. Mid / Side / Width / Vector strip
                div { class: "spatial-ms-cell oled-screen",
                    MidSideReadout {
                        mid_pct:       DEMO_MID_PCT,
                        side_pct:      DEMO_SIDE_PCT,
                        width,
                        phase_coh,
                    }
                }

                // 3. Correlation meter
                div { class: "spatial-corr-cell oled-screen",
                    CorrelationMeter { correlation }
                }

                // 4. Spatial heat map (demo until backend amendment)
                div { class: "spatial-heatmap-cell oled-screen",
                    SpatialHeatMap {}
                }
            }
        }
    }
}

// ── StereoScope ───────────────────────────────────────────────────────────────

#[component]
fn StereoScope(
    path_outer:   String,
    path_inner:   String,
    path_detail1: String,
    path_detail2: String,
) -> Element {
    rsx! {
        div { class: "scope-wrap",
            svg {
                view_box: "0 0 120 120",
                xmlns: "http://www.w3.org/2000/svg",
                style: "filter: drop-shadow(0 0 3px rgba(0,209,255,0.4));",

                StereoGrid {}

                if !path_detail2.is_empty() {
                    path { d: "{path_detail2}", fill: "none",
                           stroke: "var(--accent-magenta)", stroke_width: "0.7", opacity: "0.15" }
                }
                if !path_detail1.is_empty() {
                    path { d: "{path_detail1}", fill: "none",
                           stroke: "var(--accent-insights)", stroke_width: "0.7", opacity: "0.25" }
                }
                if !path_inner.is_empty() {
                    path { d: "{path_inner}", fill: "none",
                           stroke: "var(--accent-magenta)", stroke_width: "1.2", opacity: "0.8" }
                }
                if !path_outer.is_empty() {
                    path { d: "{path_outer}", fill: "none",
                           stroke: "var(--accent-insights)", stroke_width: "1.6", opacity: "0.9" }
                }
                circle { cx: "60", cy: "60", r: "2", fill: "var(--accent-insights)", opacity: "1.0" }
            }
        }
    }
}

#[component]
fn StereoGrid() -> Element {
    rsx! {
        g { stroke: "var(--accent-insights)", stroke_width: "0.8", opacity: "0.22", fill: "none",
            circle { cx: "60", cy: "60", r: "50" }
            circle { cx: "60", cy: "60", r: "33" }
            circle { cx: "60", cy: "60", r: "16" }
            line { x1: "60", y1: "5",   x2: "60",  y2: "115" }
            line { x1: "5",  y1: "60",  x2: "115", y2: "60"  }
            line { x1: "13", y1: "13",  x2: "107", y2: "107" }
            line { x1: "107",y1: "13",  x2: "13",  y2: "107" }
        }
    }
}

// ── MidSideReadout ────────────────────────────────────────────────────────────

#[component]
fn MidSideReadout(mid_pct: f32, side_pct: f32, width: f32, phase_coh: f32) -> Element {
    let width_str  = format!("{:.2}", width);
    // Approximate vector angle: phase_coherence 0..1 → 0..45°
    let angle_deg  = phase_coh * 45.0;
    let angle_str  = format!("+{:.0}°", angle_deg);

    rsx! {
        div { class: "spatial-ms-row",
            div { class: "spatial-ms-item",
                div { class: "spatial-ms-label", "MID" }
                div { class: "spatial-ms-value", "{mid_pct:.0}%" }
            }
            div { class: "spatial-ms-sep" }
            div { class: "spatial-ms-item",
                div { class: "spatial-ms-label", "SIDE" }
                div { class: "spatial-ms-value spatial-ms-value--side", "{side_pct:.0}%" }
            }
            div { class: "spatial-ms-sep" }
            div { class: "spatial-ms-item",
                div { class: "spatial-ms-label", "WIDTH" }
                div { class: "spatial-ms-value", "{width_str}" }
            }
            div { class: "spatial-ms-sep" }
            div { class: "spatial-ms-item",
                div { class: "spatial-ms-label", "VECTOR" }
                div { class: "spatial-ms-value spatial-ms-value--angle", "{angle_str}" }
            }
        }
    }
}

// ── CorrelationMeter ──────────────────────────────────────────────────────────

#[component]
fn CorrelationMeter(correlation: f32) -> Element {
    // Map -1..+1 → 0..100%
    let fill_pct = ((correlation + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0);
    let corr_str = if correlation >= 0.0 {
        format!("+{:.2}", correlation)
    } else {
        format!("{:.2}", correlation)
    };
    // Color: positive = teal, near-zero = amber, negative = red
    let pip_color = if correlation > 0.4 {
        "var(--accent-insights)"
    } else if correlation > 0.0 {
        "var(--accent-amber)"
    } else {
        "var(--status-err)"
    };

    rsx! {
        div { class: "corr-wrap",
            div { class: "corr-header",
                span { class: "corr-label", "CORRELATION" }
                span { class: "corr-value", style: "color: {pip_color};", "{corr_str}" }
            }
            div { class: "corr-track-wrap",
                // Scale labels
                div { class: "corr-scale",
                    span { class: "corr-scale-mark", "-1" }
                    span { class: "corr-scale-mark", "0" }
                    span { class: "corr-scale-mark", "+1" }
                }
                // Bar track
                div { class: "corr-track",
                    div { class: "corr-fill",
                          style: "width: {fill_pct:.1}%; background: {pip_color};" }
                    div { class: "corr-center-line" }
                    div { class: "corr-pip",
                          style: "left: {fill_pct:.1}%; background: {pip_color};" }
                }
            }
        }
    }
}

// ── SpatialHeatMap ────────────────────────────────────────────────────────────

/// Demo heat map — static SVG until backend amendment provides
/// per-band L/R energy data via VisualizationDataJson.
#[component]
fn SpatialHeatMap() -> Element {
    rsx! {
        div { class: "heatmap-wrap",
            div { class: "heatmap-header",
                span { class: "heatmap-ch-label", "L" }
                span { class: "heatmap-title", "SPATIAL HEAT MAP" }
                span { class: "heatmap-ch-label heatmap-ch-label--r", "R" }
            }
            div { class: "heatmap-canvas",
                svg {
                    view_box: "0 0 400 54",
                    xmlns: "http://www.w3.org/2000/svg",
                    preserve_aspect_ratio: "none",
                    style: "width:100%; height:100%; display:block;",

                    defs {
                        linearGradient {
                            id: "hm-grad", x1: "0", y1: "0", x2: "1", y2: "0",
                            stop { offset: "0%",   stop_color: "var(--accent-insights)", stop_opacity: "0.85" }
                            stop { offset: "20%",  stop_color: "var(--accent-insights)", stop_opacity: "0.55" }
                            stop { offset: "45%",  stop_color: "var(--accent-amber)",    stop_opacity: "0.35" }
                            stop { offset: "65%",  stop_color: "var(--accent-amber)",    stop_opacity: "0.20" }
                            stop { offset: "82%",  stop_color: "var(--accent-magenta)",  stop_opacity: "0.35" }
                            stop { offset: "100%", stop_color: "var(--accent-magenta)",  stop_opacity: "0.75" }
                        }
                    }

                    // Heat bars (3 rows — decreasing intensity)
                    rect { x: "0", y: "4",  width: "400", height: "16", fill: "url(#hm-grad)" }
                    rect { x: "0", y: "22", width: "400", height: "11", fill: "url(#hm-grad)", opacity: "0.55" }
                    rect { x: "0", y: "35", width: "400", height: "7",  fill: "url(#hm-grad)", opacity: "0.28" }

                    // L hotspot: 3.2kHz ≈ x=265
                    line { x1: "265", y1: "2", x2: "265", y2: "48",
                           stroke: "var(--accent-amber)", stroke_width: "1.2",
                           stroke_dasharray: "2,2", opacity: "0.75" }

                    // R hotspot: 180Hz ≈ x=330
                    line { x1: "330", y1: "2", x2: "330", y2: "48",
                           stroke: "var(--accent-magenta)", stroke_width: "1.2",
                           stroke_dasharray: "2,2", opacity: "0.75" }
                }
            }
            div { class: "heatmap-footer",
                span { class: "heatmap-hotspot heatmap-hotspot--l", "▲ L  3.2 kHz  harsh" }
                span { class: "heatmap-hotspot heatmap-hotspot--r", "▲ R  180 Hz  mud" }
            }
        }
    }
}
