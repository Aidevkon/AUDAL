//! components/primary_signal_analyzer.rs — PSA 5-cell Reference Analysis Layout
//!
//! Layout:
//!   ┌──────────────────────────────────────────────────────────────────────┐
//!   │ Cell 1: SOURCE INFO HEADER (File, PCM, Format, Clock, Sample Rate)   │
//!   ├──────────────────────────────────────────────────────────────────────┤
//!   │ Cell 2: SPECTROGRAM (Freq over time heat map)                        │
//!   ├──────────────────────────────────────────────────────────────────────┤
//!   │ Cell 3: LOUDNESS METERS (Integrated, Short-Term, True Peak)          │
//!   │         + INPUT TRIM, REF LOCK, [ LOAD NEW ] button                  │
//!   ├──────────────────────────────────────────────────────────────────────┤
//!   │ Cell 4: A/B SPECTRAL (A, B, and diff curves)                         │
//!   ├──────────────────────────────────────────────────────────────────────┤
//!   │ Cell 5: LOUDNESS TRACE (Loudness history over 120s rolling window)   │
//!   └──────────────────────────────────────────────────────────────────────┘

use crate::components::neon_canvas::NeonCanvas;
use crate::types::RealtimeFrameJson;
use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum AnalysisStage {
    Idle,
    Ingest,
    Scout,
    Stems,
    Spatial,
    Master,
    Certified,
}

pub fn stage_from_str(s: &str) -> AnalysisStage {
    match s {
        "Ingest" => AnalysisStage::Ingest,
        "Scout Pass" => AnalysisStage::Scout,
        "Stem Engine" => AnalysisStage::Stems,
        "Spatial" => AnalysisStage::Spatial,
        "Mastering" => AnalysisStage::Master,
        "CERTIFIED" => AnalysisStage::Certified,
        _ => AnalysisStage::Idle,
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PrimarySignalAnalyzerProps {
    pub filename: String,
    pub format: String,
    pub telemetry: Option<Signal<Option<RealtimeFrameJson>>>,
    pub bpm: f32,
    pub journey_stage: Signal<String>,
    // Add additional props like session state when backend provides it
}

#[component]
pub fn PrimarySignalAnalyzer(props: PrimarySignalAnalyzerProps) -> Element {
    // Demo values since backend doesn't provide these yet
    let int_lufs = -14.2;
    let st_lufs = -9.8;
    let tp_dbtp = -1.82;

    rsx! {
        div { class: "psa-body oled-screen",

            // ── CELL 1: SOURCE INFO HEADER ─────────────────────────────────────
            div { class: "psa-header-cell",
                div { class: "psa-header-text",
                    "SOURCE: ", span { class: "psa-header-val", "{props.filename}" }
                }
                div { class: "psa-header-text",
                    "PCM: ", span { class: "psa-header-val", "32F" }
                }
                div { class: "psa-header-text",
                    "FORMAT: ", span { class: "psa-header-val", "{props.format}" }
                }
                div { class: "psa-header-text",
                    "CLOCK: ", span { class: "psa-header-val", "INT" }
                }
                div { class: "psa-header-text",
                    span { class: "psa-header-val", "96.0 kHz" }
                }
            }

            // ── CELL 2: SPECTROGRAM ────────────────────────────────────────────
            div { class: "psa-spectrogram-cell",
                div { class: "psa-cell-header",
                    span { "SPECTROGRAM" }
                    span { "FREQ: 0–20 kHz   TIME: -5000 → 0 ms" }
                }
                div { class: "psa-canvas",
                    // Placeholder for actual spectrogram
                    div {
                        style: "position:absolute; inset:0; border-radius:4px;
                                background: linear-gradient(0deg, var(--bg-cell) 0%, rgba(0, 152, 152, 0.1) 40%, rgba(232, 160, 0, 0.2) 75%, rgba(232, 56, 32, 0.3) 100%);
                                border: 1px solid rgba(255,255,255,0.02);",
                    }
                    div {
                        style: "position:absolute; inset:0; opacity:0.1;
                                background-image: repeating-linear-gradient(90deg, transparent, transparent 4px, rgba(255,255,255,0.5) 4px, rgba(255,255,255,0.5) 5px);",
                    }
                }
            }

            // ── CELL 3: LOUDNESS METERS & CONTROLS ─────────────────────────────
            div { class: "psa-loudness-cell",
                MeterRow { label: "INTEGRATED", value: format!("{int_lufs:.1} LUFS"), pct: 75.0 }
                MeterRow { label: "SHORT-TERM", value: format!("{st_lufs:.1} LUFS"), pct: 85.0 }
                MeterRow { label: "TRUE PEAK",  value: format!("{tp_dbtp:.2} dBTP"), pct: 92.0 }

                div { class: "analysis-leds",
                    {
                        let current = stage_from_str(&props.journey_stage.read());
                        let class_ingest = if current == AnalysisStage::Ingest { "led active pulse" } else if current >= AnalysisStage::Ingest { "led active" } else { "led" };
                        let class_scout = if current == AnalysisStage::Scout { "led active pulse" } else if current >= AnalysisStage::Scout { "led active" } else { "led" };
                        let class_stems = if current == AnalysisStage::Stems { "led active pulse" } else if current >= AnalysisStage::Stems { "led active" } else { "led" };
                        let class_spatial = if current == AnalysisStage::Spatial { "led active pulse" } else if current >= AnalysisStage::Spatial { "led active" } else { "led" };
                        let class_master = if current == AnalysisStage::Master { "led active pulse" } else if current >= AnalysisStage::Master { "led active" } else { "led" };
                        rsx! {
                            div { class: "{class_ingest}", "INGEST" }
                            div { class: "{class_scout}", "SCOUT" }
                            div { class: "{class_stems}", "STEMS" }
                            div { class: "{class_spatial}", "SPATIAL" }
                            div { class: "{class_master}", "MASTER" }
                        }
                    }
                }


            }

            // ── CELL 4: A/B SPECTRAL ───────────────────────────────────────────
            div { class: "psa-ab-cell",
                div { class: "psa-cell-header",
                    span { "A/B SPECTRAL" }
                    div { style: "display:flex; gap:12px;",
                        span { "A: ", span { style: "color:#fff;", "───" } }
                        span { "B: ", span { style: "color:var(--accent-cyan);", "───" } }
                        span { "Δ: ", span { style: "color:var(--status-err);", "───" } }
                    }
                }
                div { class: "psa-canvas",
                    NeonCanvas {
                        telemetry: props.telemetry,
                        session_state: None,
                        bpm: props.bpm,
                        width: 600,
                        height: 250,
                        paused: false,
                        is_delta_mode: false,
                    }
                }
            }

            // ── CELL 5: LOUDNESS TRACE ─────────────────────────────────────────
            div { class: "psa-trace-cell",
                div { class: "psa-cell-header",
                    span { "LOUDNESS TRACE" }
                    span { "0s → 120s" }
                }
                div { class: "psa-canvas",
                    svg {
                        view_box: "0 0 400 60", preserve_aspect_ratio: "none",
                        style: "position:absolute; inset:0; width:100%; height:100%;",

                        // Y-axis labels
                        text { x:"2", y:"10", fill:"rgba(255,255,255,0.3)", font_size:"8", font_family:"monospace", " 0" }
                        text { x:"2", y:"32", fill:"rgba(255,255,255,0.3)", font_size:"8", font_family:"monospace", "-20" }
                        text { x:"2", y:"56", fill:"rgba(255,255,255,0.3)", font_size:"8", font_family:"monospace", "-40" }

                        // Grid lines
                        line { x1:"25", y1:"7", x2:"400", y2:"7", stroke:"rgba(255,255,255,0.05)", stroke_width:"1" }
                        line { x1:"25", y1:"29", x2:"400", y2:"29", stroke:"rgba(255,255,255,0.05)", stroke_width:"1" }
                        line { x1:"25", y1:"53", x2:"400", y2:"53", stroke:"rgba(255,255,255,0.05)", stroke_width:"1" }

                        // Loudness history line (cyan)
                        path { d:"M25,53 L50,45 L75,30 L100,28 L150,32 L200,20 L250,15 L300,22 L350,10 L400,12", fill:"none", stroke:"var(--accent-insights)", stroke_width:"1.5" }

                        // Highlight block for short-term peak
                        rect { x:"240", y:"15", width:"20", height:"38", fill:"var(--accent-insights)", opacity:"0.2" }
                    }
                }
            }
        }
    }
}

#[component]
fn MeterRow(label: &'static str, value: String, pct: f32) -> Element {
    rsx! {
        div { class: "psa-meter-row",
            div { class: "psa-meter-label", "{label}:" }
            div { class: "psa-meter-val", "{value}" }
            div { class: "psa-meter-bar-container",
                div { class: "psa-meter-bar-fill", style: "width: {pct}%" }
            }
        }
    }
}
