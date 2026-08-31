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

use crate::components::intent_bay::IntentBay;
use crate::components::module_frame::ModuleFrame;
use crate::components::neon_canvas::NeonCanvas;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, RealtimeFrameJson, SessionStateJson, VisualizationDataJson};
use dioxus::prelude::*;

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
    pub telemetry: Signal<Option<RealtimeFrameJson>>,
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

    let mut intents_active = use_signal(|| false);
    let mut spatial_collapsed = use_signal(|| false);

    // Removed lissajous code to fix unused variable warnings since StereoScope is gone

    let (correlation, width) = match state.as_ref() {
        Some(s) => (s.quality.stereo_correlation, s.quality.stereo_width),
        None => (0.21_f32, 0.62_f32),
    };

    // VECTOR: goniometer angle ΑΠΟ ΤΟ ΜΕΤΡΗΜΕΝΟ stereo_correlation.
    // Σύμβαση, ρητή:
    //   corr = +1 (mono, ταυτόσημα κανάλια) →  0°
    //   corr =  0 (ασυσχέτιστα)             → 45°
    //   corr = −1 (αντίθετη φάση)           → 90°
    // ⇒ angle = 45 * (1 − corr), εύρος [0°, 90°].
    //
    // ΠΡΙΝ (F-085, ως 2026-08-25): `phase_coherence * 45.0`, με το
    // phase_coherence σταθερό 0.97 ⇒ **πάντα «+44°»**, σε κάθε master,
    // από 15/04. Και η μπάρα διαιρούσε με 90 ενώ η γωνία έφτανε ως 45
    // ⇒ δεν ξεπερνούσε ποτέ το 50%. Δύο κλίμακες στην ίδια γραμμή.
    // Το πεδίο αφαιρέθηκε (δεν είχε ορισμό πουθενά)· η γωνία παράγεται
    // πλέον από μέγεθος που ΜΕΤΡΙΕΤΑΙ.
    let angle_deg = 45.0 * (1.0 - correlation);

    let width_str = format!("{:.2}", width);
    let angle_str = format!("{:.0}°", angle_deg);

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
                            telemetry: Some(props.telemetry),
                            session_state: Some(props.session_state),
                            bpm: 0.0,
                            width: 800,
                            height: if *intents_active.read() { 180 } else { 280 },
                            paused: false,
                            is_delta_mode: true,
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
                            on_down_tone: props.on_down_tone,
                            on_down_dyn: props.on_down_dyn,
                            on_down_space: props.on_down_space,
                            on_down_loud: props.on_down_loud,
                        }
                    }
                }

                button { onclick: move |_| spatial_collapsed.toggle(),
                    if *spatial_collapsed.read() { "▶ SPATIAL" } else { "▼ SPATIAL" }
                }

                if !*spatial_collapsed.read() {
                    // TEMPORARILY DISABLED — freeing vertical space for canvas live-test.
                    // WIDTH/VECTOR are real data (session_state.quality), MID/SIDE are demo placeholders.
                    // Re-enable + redesign placement after live telemetry test confirms canvas works.
                    if false {
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
                                    bar_pct: (angle_deg / 90.0 * 100.0).clamp(0.0, 100.0),
                                }
                            }
                        }
                    }

                    // ── CORRELATION: full-width horizontal meter ──────────────────
                    // ARCHIVED: correlation bar — to be re-homed to Left MFD
                    //           (Spatial Telemetry) in a later pass. See TODO.
                    // div { class: "spatial-corr-cell oled-screen",
                    //     CorrelationMeter { correlation }
                    // }

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
