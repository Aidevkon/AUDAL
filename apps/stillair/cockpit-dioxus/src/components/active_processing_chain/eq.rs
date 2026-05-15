use dioxus::prelude::*;
use super::types::EQState;

pub fn eq_stroke_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{}", height/2.0, width, height/2.0); }
    let first = points[0];
    let mut path = format!("M{},{}", first.0 * width, first.1 * height);
    for p in &points[1..] {
        path.push_str(&format!(" L{},{}", p.0 * width, p.1 * height));
    }
    path
}


/// EQ spectrum cell — mirrors SpectrumDisplay in InsightsPanel.
/// Full-width EQ transfer curve with oscilloscope grid + freq labels.
/// Readout strip at bottom: sum_db (left) + band label (right).
#[component]
pub fn EQDisplay(state: EQState) -> Element {
    let sum_db = state.low_db + state.mid_db + state.presence_db + state.air_db;
    let readout = format!("{:+.1} dB", sum_db);

    rsx! {
        div { class: "dsp-spectrum-wrap",

            svg {
                class: "dsp-spectrum-svg",
                view_box: "0 0 400 160",
                xmlns: "http://www.w3.org/2000/svg",

                defs {
                    linearGradient {
                        id: "eq-grad",
                        x1: "0", y1: "0", x2: "0", y2: "1",
                        stop { offset: "0%",   stop_color: "#7755ee", stop_opacity: "0.55" }
                        stop { offset: "85%",  stop_color: "#7755ee", stop_opacity: "0.06" }
                        stop { offset: "100%", stop_color: "transparent", stop_opacity: "0" }
                    }
                }

                // Oscilloscope grid — mirrors SpectrumGrid
                EQGrid {}

                // Zero line
                line {
                    x1: "10", y1: "80", x2: "390", y2: "80",
                    stroke: "rgba(119,85,238,0.15)", stroke_width: "0.8",
                    stroke_dasharray: "3,3"
                }

                // Curve fill
                path {
                    d: "{eq_fill_path_scaled(&state.curve_points)}",
                    fill: "url(#eq-grad)",
                }

                // Curve stroke
                path {
                    d: "{eq_stroke_path_scaled(&state.curve_points)}",
                    fill: "none",
                    stroke: "#7755ee",
                    stroke_width: "1.5",
                    stroke_linejoin: "round",
                }

                // Freq labels — mirrors FreqLabels
                EQFreqLabels {}
            }

            // Footer readout strip — mirrors spectrum-footer
            div { class: "dsp-spectrum-footer",
                span { class: "dsp-eq-label", "4-BAND PAR EQ" }
                span { class: "dsp-eq-readout", "{readout}" }
            }
        }
    }
}

/// Scale curve_points (normalized 0..1) to 400×160 viewBox with zero at y=80
fn eq_stroke_path_scaled(points: &[(f32, f32)]) -> String {
    if points.is_empty() { return "M10,80 L390,80".to_string(); }
    let first = points[0];
    let mut path = format!("M{:.1},{:.1}", 10.0 + first.0 * 380.0, 80.0 + (first.1 - 0.5) * 120.0);
    for p in &points[1..] {
        path.push_str(&format!(" L{:.1},{:.1}", 10.0 + p.0 * 380.0, 80.0 + (p.1 - 0.5) * 120.0));
    }
    path
}

fn eq_fill_path_scaled(points: &[(f32, f32)]) -> String {
    if points.is_empty() { return "M10,80 L390,80 L390,80 L10,80 Z".to_string(); }
    let mut path = eq_stroke_path_scaled(points);
    path.push_str(" L390,80 L10,80 Z");
    path
}

/// EQ oscilloscope grid — mirrors SpectrumGrid
#[component]
fn EQGrid() -> Element {
    rsx! {
        g {
            stroke: "#7755ee",
            stroke_width: "0.5",
            opacity: "0.10",
            fill: "none",
            // Horizontal dB reference lines
            line { x1: "10", y1: "20",  x2: "390", y2: "20"  }  // +6dB
            line { x1: "10", y1: "50",  x2: "390", y2: "50"  }  // +3dB
            line { x1: "10", y1: "80",  x2: "390", y2: "80"  }  // 0dB
            line { x1: "10", y1: "110", x2: "390", y2: "110" }  // -3dB
            line { x1: "10", y1: "140", x2: "390", y2: "140" }  // -6dB
            // Vertical frequency markers
            line { x1: "50",  y1: "10", x2: "50",  y2: "150" }
            line { x1: "90",  y1: "10", x2: "90",  y2: "150" }
            line { x1: "145", y1: "10", x2: "145", y2: "150" }
            line { x1: "195", y1: "10", x2: "195", y2: "150" }
            line { x1: "260", y1: "10", x2: "260", y2: "150" }
            line { x1: "310", y1: "10", x2: "310", y2: "150" }
            line { x1: "380", y1: "10", x2: "380", y2: "150" }
        }
    }
}

/// Frequency axis labels — mirrors FreqLabels
#[component]
fn EQFreqLabels() -> Element {
    let labels: &[(&str, &str)] = &[
        ("20Hz", "12"), ("100", "52"), ("500", "107"),
        ("1k", "155"), ("5k", "222"), ("10k", "272"), ("20k", "345"),
    ];
    rsx! {
        g {
            font_family: "monospace",
            font_size: "8",
            fill: "#7755ee",
            opacity: "0.45",
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
