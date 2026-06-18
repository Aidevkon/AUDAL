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
use crate::components::neon_canvas::NeonCanvas;
use crate::components::intent_bay::IntentBay;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson, RealtimeFrameJson};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

// ── Demo Lissajous paths (FM0 idle) ───────────────────────────────────────────


// Demo M/S — pending backend amendment
const DEMO_MID_PCT: f32 = 62.0;
const DEMO_SIDE_PCT: f32 = 38.0;

// ── InsightsPanel ─────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct InsightsPanelProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
    pub playback_state: Signal<Option<PlaybackStateJson>>,
    pub viz_data: Signal<Option<VisualizationDataJson>>,
    pub tone_angle: Signal<f32>,
    pub dyn_angle: Signal<f32>,
    pub space_angle: Signal<f32>,
    pub loud_angle: Signal<f32>,
    pub on_down_tone: EventHandler<MouseEvent>,
    pub on_down_dyn: EventHandler<MouseEvent>,
    pub on_down_space: EventHandler<MouseEvent>,
    pub on_down_loud: EventHandler<MouseEvent>,
}

#[component]
pub fn InsightsPanel(props: InsightsPanelProps) -> Element {
    let state = props.session_state.read();

    let mut realtime: Signal<Option<RealtimeFrameJson>> = use_signal(|| None);
    let mut intents_active = use_signal(|| false);
    let mut spatial_collapsed = use_signal(|| false);

    let mut tele_generation = use_signal(|| 0u64);

    use_effect(move || {
        let my_gen = *tele_generation.peek() + 1;
        tele_generation.set(my_gen);
        spawn_local(async move {
            loop {
                if *tele_generation.peek() != my_gen { break; }

                let m = props.mode.read().clone();
                let is_active = !matches!(m, CockpitMode::Idle | CockpitMode::CoachReady { .. } | CockpitMode::Exporting { .. });
                
                if is_active {
                    web_sys::console::log_1(&format!(
                        "[TELE-TRAP] polling, mode={:?}", m
                    ).into());
                    if let Ok(Some(frame)) = crate::ipc::invoke_no_args::<Option<RealtimeFrameJson>>("get_live_telemetry_realtime").await {
                        realtime.set(Some(frame));
                    }
                    gloo_timers::future::TimeoutFuture::new(200).await;
                } else {
                    gloo_timers::future::TimeoutFuture::new(200).await;
                }
            }
        });
    });

    // Removed lissajous code to fix unused variable warnings since StereoScope is gone

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

                div { class: "center-mfd-stack",
                    div { class: "canvas-reactive-zone",
                        NeonCanvas {
                            telemetry: Some(realtime),
                            session_state: Some(props.session_state),
                            bpm: 0.0,
                            width: 800,
                            height: if *intents_active.read() { 180 } else { 280 },
                            paused: false,
                            is_delta_mode: false,
                        }
                    }
                    div {
                        class: if *intents_active.read() { "intent-reveal-zone active" }
                               else { "intent-reveal-zone" },
                        onmouseenter: move |_| intents_active.set(true),
                        onmouseleave: move |_| intents_active.set(false),
                        IntentBay {
                            tone_angle: props.tone_angle,
                            dyn_angle: props.dyn_angle,
                            space_angle: props.space_angle,
                            loud_angle: props.loud_angle,
                            on_down_tone: props.on_down_tone.clone(),
                            on_down_dyn: props.on_down_dyn.clone(),
                            on_down_space: props.on_down_space.clone(),
                            on_down_loud: props.on_down_loud.clone(),
                        }
                    }
                }

                button { onclick: move |_| spatial_collapsed.toggle(),
                    if *spatial_collapsed.read() { "▶ SPATIAL" } else { "▼ SPATIAL" }
                }

                if !*spatial_collapsed.read() {
                    // ── TOP ROW: Metrics stack ─────────
                    div { class: "spatial-top-row",

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

// ── SpectrumBars ─────────────────────────────────────────────────────────────

#[component]
fn SpectrumBars(spectrum: Option<Vec<f32>>) -> Element {
    let bars = spectrum.unwrap_or_else(|| vec![-40.0; 64]);
    
    rsx! {
        svg {
            view_box: "0 0 640 120",
            width: "100%", height: "100%",
            preserve_aspect_ratio: "none",
            {
                bars.iter().enumerate().map(|(i, &db)| {
                    let clamped = db.clamp(-80.0, 0.0);
                    let pct = (clamped + 80.0) / 80.0;
                    let h = pct * 120.0;
                    let y = 120.0 - h;
                    let x = i as f32 * 10.0;
                    let opacity = 0.3 + (pct * 0.7);
                    
                    rsx! {
                        rect {
                            key: "{i}",
                            x: "{x}", y: "{y}",
                            width: "8", height: "{h}",
                            fill: "var(--accent-insights)",
                            opacity: "{opacity}"
                        }
                    }
                })
            }
        }
    }
}

