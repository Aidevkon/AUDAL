//! panels/insights.rs — SPATIAL TELEMETRY panel · Phase 14
//! Layout:
//!   ┌──────────────────┬─────────────────────┐
//!   │  LISSAJOUS       │  MID   62%          │
//!   │  (square, 45%w)  │  SIDE  38%          │  ~50% height
//!   │                  │  WIDTH 0.62         │
//!   │                  │  VECTOR +14°        │
//!   ├──────────────────┴─────────────────────┤
//!   │  CORRELATION ────────●──── +0.21       │  ~12% height
//!   ├────────────────────────────────────────┤
//!   │  L ████████░░░░░░░░░░░░░░░░░░░░░░░ R  │  flex:1
//!   └────────────────────────────────────────┘
//! Laws: ❌ No SVG path computation  ❌ No inline hex  ❌ No std::f32 methods
//! M/S fields: demo until backend amendment.

use crate::components::module_frame::ModuleFrame;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};
use dioxus::prelude::*;

// ── Demo Lissajous paths (FM0 idle) ───────────────────────────────────────────
const DEMO_LISS_OUTER: &str = "M 82,60 L 89,74 L 100,94 L 104,110 L 98,116 L 86,110 L 74,94 \
     L 66,74 L 60,60 L 54,46 L 46,26 L 34,10 L 22,4 L 16,10 \
     L 20,26 L 34,46 L 48,60 L 54,74 L 54,94 L 48,110 \
     L 38,116 L 28,108 L 22,92 L 26,74 L 38,60 L 52,46 \
     L 66,26 L 78,10 L 86,4 L 92,10 L 94,26 L 88,46 L 82,60";

const DEMO_LISS_INNER: &str = "M 60,60 L 72,73 L 84,78 L 84,66 L 72,47 L 60,38 \
     L 48,47 L 36,66 L 36,78 L 48,73 L 60,60 \
     L 72,47 L 84,42 L 84,54 L 72,73 L 60,82 \
     L 48,73 L 36,54 L 36,42 L 48,47 L 60,60";

const DEMO_LISS_D1: &str = "M 60,60 L 81,73 L 90,60 L 81,47 L 60,60 \
     L 39,73 L 30,60 L 39,47 L 60,60 L 75,78 L 60,60 L 45,78 L 60,60";

const DEMO_LISS_D2: &str = "M 60,60 L 74,80 L 80,60 L 74,40 L 60,60 \
     L 46,80 L 40,60 L 46,40 L 60,60 L 70,88 L 60,60 L 50,88 L 60,60";

// Demo M/S — pending backend amendment
const DEMO_MID_PCT: f32 = 62.0;
const DEMO_SIDE_PCT: f32 = 38.0;

// ── InsightsPanel ─────────────────────────────────────────────────────────────

#[component]
pub fn InsightsPanel(
    mode: Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    playback_state: Signal<Option<PlaybackStateJson>>,
    viz_data: Signal<Option<VisualizationDataJson>>,
) -> Element {
    let state = session_state.read();
    let viz = viz_data.read();

    let liss_outer = viz
        .as_ref()
        .map(|v| v.lissajous_path_outer.as_str())
        .unwrap_or(DEMO_LISS_OUTER)
        .to_string();
    let liss_inner = viz
        .as_ref()
        .map(|v| v.lissajous_path_inner.as_str())
        .unwrap_or(DEMO_LISS_INNER)
        .to_string();
    let liss_detail1 = viz
        .as_ref()
        .map(|v| v.lissajous_path_detail1.as_str())
        .unwrap_or(DEMO_LISS_D1)
        .to_string();
    let liss_detail2 = viz
        .as_ref()
        .map(|v| v.lissajous_path_detail2.as_str())
        .unwrap_or(DEMO_LISS_D2)
        .to_string();

    let (correlation, width, phase_coh) = match state.as_ref() {
        Some(s) => (
            s.quality.stereo_correlation,
            s.quality.stereo_width,
            s.quality.phase_coherence,
        ),
        None => (0.21_f32, 0.62_f32, 0.31_f32),
    };

    let width_str = format!("{:.2}", width);
    let angle_str = format!("+{:.0}°", phase_coh * 45.0);

    rsx! {
        ModuleFrame {
            title:        "SPATIAL TELEMETRY".to_string(),
            panel_class:  "panel-insights".to_string(),
            header_style: "color:var(--accent-insights);".to_string(),
            is_scrollable: false,

            div { class: "spatial-body",

                // ── TOP ROW: Lissajous (left) + Metrics stack (right) ─────────
                div { class: "spatial-top-row",

                    // Left: square goniometer
                    div { class: "spatial-scope-cell oled-screen",
                        StereoScope {
                            path_outer:   liss_outer,
                            path_inner:   liss_inner,
                            path_detail1: liss_detail1,
                            path_detail2: liss_detail2,
                        }
                    }

                    // Right: 4 stacked metric readouts
                    div { class: "spatial-metrics-col",
                        MetricRow {
                            label: "MID",
                            value: format!("{:.0}%", DEMO_MID_PCT),
                            color: "var(--accent-insights)",
                            bar_pct: DEMO_MID_PCT,
                        }
                        MetricRow {
                            label: "SIDE",
                            value: format!("{:.0}%", DEMO_SIDE_PCT),
                            color: "var(--accent-magenta)",
                            bar_pct: DEMO_SIDE_PCT,
                        }
                        MetricRow {
                            label: "WIDTH",
                            value: width_str,
                            color: "var(--accent-insights)",
                            bar_pct: width * 100.0,
                        }
                        MetricRow {
                            label: "VECTOR",
                            value: angle_str,
                            color: "var(--accent-amber)",
                            bar_pct: (phase_coh * 45.0 / 90.0 * 100.0).clamp(0.0, 100.0),
                        }
                    }
                }

                // ── CORRELATION: full-width horizontal meter ──────────────────
                div { class: "spatial-corr-cell oled-screen",
                    CorrelationMeter { correlation }
                }

                // ── SPATIAL HEAT MAP: full-width, flex:1 ─────────────────────
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
    path_outer: String,
    path_inner: String,
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
                circle { cx: "60", cy: "60", r: "2",
                          fill: "var(--accent-insights)", opacity: "1.0" }
            }
        }
    }
}

#[component]
fn StereoGrid() -> Element {
    rsx! {
        g { stroke: "var(--accent-insights)", stroke_width: "0.8",
            opacity: "0.22", fill: "none",
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

// ── MetricRow — stacked readout tile with mini progress bar ───────────────────

#[component]
fn MetricRow(label: &'static str, value: String, color: &'static str, bar_pct: f32) -> Element {
    rsx! {
        div { class: "metric-row oled-screen",
            div { class: "metric-row-header",
                span { class: "metric-label", "{label}" }
                span { class: "metric-value", style: "color: {color};", "{value}" }
            }
            // Mini bar
            div { class: "metric-bar-track",
                div { class: "metric-bar-fill",
                      style: "width: {bar_pct:.1}%; background: {color};" }
            }
        }
    }
}

// ── CorrelationMeter ──────────────────────────────────────────────────────────

#[component]
fn CorrelationMeter(correlation: f32) -> Element {
    let fill_pct = ((correlation + 1.0) / 2.0 * 100.0).clamp(0.0, 100.0);
    let corr_str = if correlation >= 0.0 {
        format!("+{:.2}", correlation)
    } else {
        format!("{:.2}", correlation)
    };
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
                div { class: "corr-scale",
                    span { class: "corr-scale-mark", "-1" }
                    span { class: "corr-scale-mark", "0" }
                    span { class: "corr-scale-mark", "+1" }
                }
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
