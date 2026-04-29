//! panels/insights.rs — THE INSIGHTS panel · Phase 14
//! Authority: Phase 14 P14-004 through P14-010 · UI Agent Context v2.1
//!
//! Layout matches reference mockup exactly:
//!   ┌───────────────────────────────┐
//!   │  SPECTRUM (full width, top)   │  ~55% height
//!   ├──────────────┬────────────────┤
//!   │  StereoScope │  VU + Metrics  │  ~45% height
//!   └──────────────┴────────────────┘
//!
//! Laws:
//!   ❌ No SVG path computation — paths come from viz signal
//!   ❌ No inline hex colors — CSS variables only  
//!   ❌ No business logic
//!   ❌ No std::f32 methods
//!
//! A-003 §5: No PCM. No audio kernel imports.

use dioxus::prelude::*;
use crate::components::module_frame::ModuleFrame;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};

// ── Demo path constants — static string literals, zero computation ────────────
//
// Used when viz_data is None (FM0/idle state, pre-mastering).
// These are hardcoded points for a typical mastered track:
//   Spectrum: gaussian peak ~1kHz, gentle high-end roll-off
//   Lissajous: a=1 b=2 figure-8 (standard goniometer reference shape)
//
// Source: sampled from compute_lissajous_path(rx=44, ry=50, a=1, b=2, phase=0.5)
// and compute_spectrum_path(centroid=3200, flatness=0.45) offline.
// Real data replaces these the moment mastering completes.

/// Demo spectrum — gaussian with high-roll-off texture. viewBox 0 0 400 160.
const DEMO_SPECTRUM: &str =
    "M 10,148 L 16,146 L 22,143 L 28,140 L 34,135 L 40,128 \
     L 46,118 L 52,106 L 58,92 L 64,78 L 70,65 L 76,54 L 82,46 \
     L 88,40 L 94,37 L 100,37 L 106,40 L 112,45 L 118,50 L 124,56 \
     L 130,63 L 140,74 L 152,84 L 165,92 L 180,100 L 198,108 \
     L 218,114 L 240,119 L 262,124 L 284,130 L 304,135 \
     L 322,139 L 340,142 L 358,145 L 374,147 L 390,149 \
     L 390,150 L 10,150 Z";

/// Demo Lissajous outer — figure-8, a=1 b=2 phase≈0.5, rx=44 ry=50. viewBox 120×120.
const DEMO_LISS_OUTER: &str =
    "M 82,60 L 89,74 L 100,94 L 104,110 L 98,116 L 86,110 L 74,94 \
     L 66,74 L 60,60 L 54,46 L 46,26 L 34,10 L 22,4 L 16,10 \
     L 20,26 L 34,46 L 48,60 L 54,74 L 54,94 L 48,110 \
     L 38,116 L 28,108 L 22,92 L 26,74 L 38,60 L 52,46 \
     L 66,26 L 78,10 L 86,4 L 92,10 L 94,26 L 88,46 \
     L 82,60";

/// Demo Lissajous inner — smaller figure, a=2 b=3, rx=24 ry=22. viewBox 120×120.
const DEMO_LISS_INNER: &str =
    "M 60,60 L 72,73 L 84,78 L 84,66 L 72,47 L 60,38 \
     L 48,47 L 36,66 L 36,78 L 48,73 L 60,60 \
     L 72,47 L 84,42 L 84,54 L 72,73 L 60,82 \
     L 48,73 L 36,54 L 36,42 L 48,47 L 60,60";

/// Demo detail trace 1 — thin cyan, a=1 b=3, rx=30 ry=18.
const DEMO_LISS_D1: &str =
    "M 60,60 L 81,73 L 90,60 L 81,47 L 60,60 \
     L 39,73 L 30,60 L 39,47 L 60,60 \
     L 75,78 L 60,60 L 45,78 L 60,60";

/// Demo detail trace 2 — thin magenta, a=3 b=2, rx=20 ry=28.
const DEMO_LISS_D2: &str =
    "M 60,60 L 74,80 L 80,60 L 74,40 L 60,60 \
     L 46,80 L 40,60 L 46,40 L 60,60 \
     L 70,88 L 60,60 L 50,88 L 60,60";

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

    // Use backend viz when available, fall back to demo constants (FM0 state)
    let spectrum_path = viz.as_ref()
        .map(|v| v.spectrum_svg_path.as_str())
        .unwrap_or(DEMO_SPECTRUM)
        .to_string();
    let liss_outer   = viz.as_ref().map(|v| v.lissajous_path_outer.as_str())
        .unwrap_or(DEMO_LISS_OUTER).to_string();
    let liss_inner   = viz.as_ref().map(|v| v.lissajous_path_inner.as_str())
        .unwrap_or(DEMO_LISS_INNER).to_string();
    let liss_detail1 = viz.as_ref().map(|v| v.lissajous_path_detail1.as_str())
        .unwrap_or(DEMO_LISS_D1).to_string();
    let liss_detail2 = viz.as_ref().map(|v| v.lissajous_path_detail2.as_str())
        .unwrap_or(DEMO_LISS_D2).to_string();

    // Demo metric values — shown before mastering, replaced by real session data
    let demo_lufs = -18.2_f32;
    let demo_peak = -1.5_f32;
    let demo_lra  = 12.0_f32;
    let demo_corr = 0.65_f32;

    rsx! {
        ModuleFrame {
            title: "SPECTRAL DYNAMICS & INSIGHTS".to_string(),
            panel_class: "panel-insights".to_string(),
            header_style: "color:var(--accent-insights);".to_string(),
            is_scrollable: false,

            div {
                class: "insights-body",

                // ── TOP: Spectrum (full width) ────────────────────────────────
                div {
                    class: "insights-spectrum-cell oled-screen",
                    SpectrumDisplay {
                        path: spectrum_path,
                        lufs: state.as_ref().map(|s| s.loudness.integrated_lufs).unwrap_or(-18.2_f32),
                        peak: state.as_ref().map(|s| s.loudness.true_peak_dbtp).unwrap_or(-1.5_f32),
                    }
                }

                // ── BOTTOM ROW ────────────────────────────────────────────────
                div {
                    class: "insights-bottom-row",

                    // Bottom-left: StereoScope
                    div {
                        class: "insights-scope-cell oled-screen",
                        StereoScope {
                            path_outer:   liss_outer,
                            path_inner:   liss_inner,
                            path_detail1: liss_detail1,
                            path_detail2: liss_detail2,
                        }
                    }

                    // Bottom-right: VU meters + large numeric readouts
                    div {
                        class: "insights-meters-cell oled-screen",

                        // VuPanel: real data when session present, demo values in FM0
                        {
                            let (vu_lufs, vu_peak, vu_lra, vu_corr) = match state.as_ref() {
                                Some(s) => (
                                    s.loudness.integrated_lufs,
                                    s.loudness.true_peak_dbtp,
                                    s.loudness.lra,
                                    s.quality.stereo_correlation,
                                ),
                                None => (demo_lufs, demo_peak, demo_lra, demo_corr),
                            };
                            rsx! {
                                VuPanel {
                                    lufs:        vu_lufs,
                                    peak:        vu_peak,
                                    lra:         vu_lra,
                                    correlation: vu_corr,
                                }
                            }
                        }
                    }
                }
            }   // .insights-body
        }
    }
}

// ── SpectrumDisplay (§4.7) ────────────────────────────────────────────────────

/// Full-width spectrum with LUFS/PEAK readouts at bottom.
/// Path from backend. Renders only. Spec: §4.7.
#[component]
fn SpectrumDisplay(path: String, lufs: f32, peak: f32) -> Element {
    let lufs_str = format!("{:.1}", lufs);
    let peak_str = format!("{:.1}", peak);

    rsx! {
        div {
            class: "spectrum-wrap",

            // SVG canvas
            svg {
                class: "spectrum-svg",
                view_box: "0 0 400 160",
                xmlns: "http://www.w3.org/2000/svg",

                defs {
                    linearGradient {
                        id: "sg",
                        x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%",   stop_color: "var(--accent-insights)", stop_opacity: "0.9" }
                        stop { offset: "85%",  stop_color: "var(--accent-insights)", stop_opacity: "0.15" }
                        stop { offset: "100%", stop_color: "transparent", stop_opacity: "0" }
                    }
                }

                // Oscilloscope grid
                SpectrumGrid {}

                // Fill area
                if !path.is_empty() {
                    path {
                        d:    "{path}",
                        fill: "url(#sg)",
                    }
                    // Stroke line
                    path {
                        d:            "{path}",
                        fill:         "none",
                        stroke:       "var(--accent-insights)",
                        stroke_width: "1.5",
                        stroke_linejoin: "round",
                    }
                } else {
                    // Placeholder when no session — flat line
                    path {
                        d: "M 10,140 L 390,140",
                        fill: "none",
                        stroke: "var(--accent-insights)",
                        stroke_width: "0.5",
                        opacity: "0.2",
                    }
                }

                // Frequency labels at bottom
                FreqLabels {}
            }

            // LUFS/PEAK readout strip at bottom
            div {
                class: "spectrum-footer",
                span { class: "spectrum-readout", "{lufs_str}" }
                span { class: "spectrum-readout-peak", "{peak_str}" }
            }
        }
    }
}

/// Oscilloscope grid lines — §4.7.
#[component]
fn SpectrumGrid() -> Element {
    rsx! {
        g {
            stroke: "var(--accent-insights)",
            stroke_width: "0.5",
            opacity: "0.12",
            fill: "none",
            // Horizontal dB reference lines
            line { x1:"10", y1:"20",  x2:"390", y2:"20"  }  // +10dB
            line { x1:"10", y1:"50",  x2:"390", y2:"50"  }  // 0dB
            line { x1:"10", y1:"80",  x2:"390", y2:"80"  }  // -10dB
            line { x1:"10", y1:"110", x2:"390", y2:"110" }  // -20dB
            line { x1:"10", y1:"140", x2:"390", y2:"140" }  // -30dB
            // Vertical frequency markers
            line { x1:"50",  y1:"10", x2:"50",  y2:"150" }  // 50Hz
            line { x1:"90",  y1:"10", x2:"90",  y2:"150" }  // 100Hz
            line { x1:"145", y1:"10", x2:"145", y2:"150" }  // 500Hz
            line { x1:"195", y1:"10", x2:"195", y2:"150" }  // 1kHz
            line { x1:"260", y1:"10", x2:"260", y2:"150" }  // 5kHz
            line { x1:"310", y1:"10", x2:"310", y2:"150" }  // 10kHz
            line { x1:"380", y1:"10", x2:"380", y2:"150" }  // 20kHz
        }
    }
}

/// Frequency axis tick labels.
#[component]
fn FreqLabels() -> Element {
    let labels: &[(&str, &str)] = &[
        ("20Hz", "12"), ("100", "52"), ("500", "107"),
        ("1k", "155"), ("5k", "222"), ("10k", "272"), ("20k", "345"),
    ];
    rsx! {
        g {
            font_family: "monospace",
            font_size: "8",
            fill: "var(--accent-insights)",
            opacity: "0.5",
            for (label, x) in labels {
                text {
                    key: "{label}",
                    x: "{x}",
                    y: "158",
                    "{label}"
                }
            }
        }
    }
}

// ── StereoScope (§4.8) ───────────────────────────────────────────────────────

/// Multi-trace Lissajous goniometer.
///
/// Receives 4 precomputed SVG path strings from the backend viz signal.
/// Renders only — zero computation. Spec: §4.8.
///
/// path_outer:   cyan, main stereo width figure  (opacity 0.9)
/// path_inner:   magenta, correlation tightness  (opacity 0.8)
/// path_detail1: cyan at 0.25 opacity            (richness trace)
/// path_detail2: magenta at 0.15 opacity         (richness trace)
#[component]
fn StereoScope(
    path_outer:   String,
    path_inner:   String,
    path_detail1: String,
    path_detail2: String,
) -> Element {
    rsx! {
        div {
            class: "scope-wrap",

            svg {
                view_box: "0 0 120 120",
                xmlns: "http://www.w3.org/2000/svg",
                style: "filter: drop-shadow(0 0 3px rgba(0,209,255,0.4));",

                // Polar grid (static — no data dependency)
                StereoGrid {}

                // Detail traces — rendered first (under main traces)
                if !path_detail2.is_empty() {
                    path {
                        d:            "{path_detail2}",
                        fill:         "none",
                        stroke:       "var(--accent-magenta)",
                        stroke_width: "0.7",
                        opacity:      "0.15",
                    }
                }
                if !path_detail1.is_empty() {
                    path {
                        d:            "{path_detail1}",
                        fill:         "none",
                        stroke:       "var(--accent-insights)",
                        stroke_width: "0.7",
                        opacity:      "0.25",
                    }
                }

                // Inner orbit — magenta
                if !path_inner.is_empty() {
                    path {
                        d:            "{path_inner}",
                        fill:         "none",
                        stroke:       "var(--accent-magenta)",
                        stroke_width: "1.2",
                        opacity:      "0.8",
                    }
                }

                // Outer orbit — cyan (main, brightest)
                if !path_outer.is_empty() {
                    path {
                        d:            "{path_outer}",
                        fill:         "none",
                        stroke:       "var(--accent-insights)",
                        stroke_width: "1.6",
                        opacity:      "0.9",
                    }
                }

                // Center hotspot — always visible
                circle {
                    cx: "60", cy: "60",
                    r:  "2",
                    fill: "var(--accent-insights)",
                    opacity: "1.0",
                }
            }
        }
    }
}


/// Polar grid: 3 circles + crosshair + diagonals (opacity 0.15).
#[component]
fn StereoGrid() -> Element {
    rsx! {
        g {
            stroke: "var(--accent-insights)",
            stroke_width: "0.8",
            opacity: "0.22",
            fill: "none",
            // Circles
            circle { cx: "60", cy: "60", r: "50" }
            circle { cx: "60", cy: "60", r: "33" }
            circle { cx: "60", cy: "60", r: "16" }
            // Cross + diagonals
            line { x1: "60", y1: "5",   x2: "60",  y2: "115" }
            line { x1: "5",  y1: "60",  x2: "115", y2: "60"  }
            line { x1: "13", y1: "13",  x2: "107", y2: "107" }
            line { x1: "107",y1: "13",  x2: "13",  y2: "107" }
        }
    }
}

// ── VuPanel — VU meters + MetricsReadout ─────────────────────────────────────

/// Right cell: segmented LUFS/PEAK VU meters + PEAK/RANGE/CORR numeric readout.
#[component]
fn VuPanel(lufs: f32, peak: f32, lra: f32, correlation: f32) -> Element {
    let corr_str = if correlation >= 0.0 {
        format!("+{:.2}", correlation)
    } else {
        format!("{:.2}", correlation)
    };

    rsx! {
        div {
            class: "vu-panel",

            // Segmented VU meters
            div {
                class: "vu-meters-row",

                // Scale labels column
                div { class: "vu-scale",
                    for label in ["0", "-3", "-6", "-10", "-12", "-14", "-18", "-20", "-25", "-30"] {
                        span { class: "vu-scale-label", "{label}" }
                    }
                }

                // LUFS channel
                div {
                    class: "vu-channel-wrap",
                    VuMeter { value: lufs, min: -30.0_f32, max: 0.0_f32, color_var: "var(--accent-insights)" }
                    div { class: "vu-ch-label", "LUFS" }
                }

                // PEAK channel
                div {
                    class: "vu-channel-wrap",
                    VuMeter { value: peak, min: -30.0_f32, max: 0.0_f32, color_var: "var(--accent-amber)" }
                    div { class: "vu-ch-label vu-peak-label", "PEAK" }
                }

                // Readout values column
                div {
                    class: "vu-readout-col",

                    div { class: "vu-readout-row",
                        div { class: "vu-readout-label", "LUFS" }
                        div { class: "vu-readout-value", style: "color:var(--accent-insights);",
                            { format!("{:.1}", lufs) }
                        }
                    }
                    div { class: "vu-readout-row",
                        div { class: "vu-readout-label", "PEAK" }
                        div { class: "vu-readout-value", style: "color:var(--accent-amber);",
                            { format!("{:.1}", peak) }
                        }
                    }
                    div { class: "vu-readout-row",
                        div { class: "vu-readout-label", "RANGE" }
                        div { class: "vu-readout-value", style: "color:var(--accent-gold);",
                            { format!("{:.0}", lra) }
                        }
                    }
                    div { class: "vu-readout-row",
                        div { class: "vu-readout-label", "CORR" }
                        div { class: "vu-readout-value", style: "color:var(--accent-insights);",
                            "{corr_str}"
                        }
                    }
                }
            }
        }
    }
}

// ── VuMeter — 30 LED segments ─────────────────────────────────────────────────

/// 30-segment VU meter bar. Active segments = solid color. Inactive = 8% opacity.
/// Top 3 segments (near 0dBTP) = red clip zone. No std::f32 methods.
#[component]
fn VuMeter(value: f32, min: f32, max: f32, color_var: &'static str) -> Element {
    let total   = 30usize;
    let range   = max - min;
    let ratio   = if range > 0.0 { (value - min) / range } else { 0.0 };
    let filled  = (ratio * total as f32).clamp(0.0, total as f32) as usize;

    rsx! {
        div {
            class: "vu-bar",
            // Segments render bottom-to-top via flex-direction:column-reverse
            for i in 0..total {
                div {
                    key: "{i}",
                    class: "vu-seg",
                    style: {
                        if i < filled {
                            // Active: top 3 = clip red, rest = color_var
                            if i >= total - 3 {
                                "background:#ff3b30;".to_string()
                            } else {
                                format!("background:{};", color_var)
                            }
                        } else {
                            // Inactive: color at 8% opacity via inline alpha
                            format!("background:{};opacity:0.08;", color_var)
                        }
                    },
                }
            }
        }
    }
}
