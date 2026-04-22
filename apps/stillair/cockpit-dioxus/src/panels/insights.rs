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
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};

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

    // Viz defaults — empty strings show nothing until backend responds
    let spectrum_path = viz.as_ref()
        .map(|v| v.spectrum_svg_path.clone())
        .unwrap_or_default();
    let liss_outer   = viz.as_ref().map(|v| v.lissajous_path_outer.clone()).unwrap_or_default();
    let liss_inner   = viz.as_ref().map(|v| v.lissajous_path_inner.clone()).unwrap_or_default();
    let liss_detail1 = viz.as_ref().map(|v| v.lissajous_path_detail1.clone()).unwrap_or_default();
    let liss_detail2 = viz.as_ref().map(|v| v.lissajous_path_detail2.clone()).unwrap_or_default();

    rsx! {
        div {
            class: "mfd-panel panel-insights",

            // Corner screws (§4.5)
            div { class: "screw screw-tl" }
            div { class: "screw screw-tr" }
            div { class: "screw screw-bl" }
            div { class: "screw screw-br" }

            // Panel title bar
            div {
                class: "panel-title",
                style: "color:var(--accent-insights);",
                "THE INSIGHTS"
            }

            // Panel body — avionics layout
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

                        match state.as_ref() {
                            None => rsx! {
                                div { class: "awaiting", style: "font-size:0.55rem;", "—" }
                            },
                            Some(s) => rsx! {
                                VuPanel {
                                    lufs: s.loudness.integrated_lufs,
                                    peak: s.loudness.true_peak_dbtp,
                                    lra:  s.loudness.lra,
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
            stroke_width: "0.7",
            opacity: "0.15",
            fill: "none",
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
