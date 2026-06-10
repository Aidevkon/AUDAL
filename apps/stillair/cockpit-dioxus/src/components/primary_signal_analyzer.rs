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

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct PrimarySignalAnalyzerProps {
    pub filename: String,
    pub format: String,
    // Add additional props like session state when backend provides it
    pub on_load_new: EventHandler<()>,
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

                div { class: "psa-controls-row",
                    div { class: "psa-control-item",
                        "INPUT TRIM: ", span { style: "color:var(--text-primary); font-weight:700;", "+0.0 dB" },
                        span { style: "color:var(--accent-amber); margin-left:4px;", "(◉ encoder)" }
                    }
                    div { class: "psa-control-item",
                        "REF LOCK: ", span { style: "color:var(--status-err); font-weight:700;", "[OFF]" }
                    }
                    button {
                        class: "psa-load-btn",
                        onclick: move |_| props.on_load_new.call(()),
                        "LOAD NEW"
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
                    svg {
                        view_box: "0 0 400 100", preserve_aspect_ratio: "none",
                        style: "position:absolute; inset:0; width:100%; height:100%;",

                        // Grid lines
                        line { x1:"0", y1:"50", x2:"400", y2:"50", stroke:"rgba(255,255,255,0.1)", stroke_width:"1", stroke_dasharray:"4 4" }
                        line { x1:"200", y1:"0", x2:"200", y2:"100", stroke:"rgba(255,255,255,0.1)", stroke_width:"1", stroke_dasharray:"4 4" }

                        // A curve (white)
                        path { d:"M0,80 Q100,20 200,50 T400,30", fill:"none", stroke:"rgba(255,255,255,0.6)", stroke_width:"1.5" }
                        // B curve (cyan)
                        path { d:"M0,90 Q100,10 200,60 T400,20", fill:"none", stroke:"var(--accent-cyan)", stroke_width:"1.5" }
                        // Delta curve (red)
                        path { d:"M0,50 Q100,40 200,40 T400,60", fill:"none", stroke:"var(--status-err)", stroke_width:"2", opacity: "0.8" }

                        text { x:"2", y:"96", fill:"rgba(255,255,255,0.3)", font_size:"8", font_family:"monospace", "20Hz" }
                        text { x:"375", y:"96", fill:"rgba(255,255,255,0.3)", font_size:"8", font_family:"monospace", "20kHz" }
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
