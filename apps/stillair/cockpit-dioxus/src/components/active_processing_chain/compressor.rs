use super::eq::eq_stroke_path;
use super::types::CompressorState;
use dioxus::prelude::*;

pub fn comp_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() {
        return format!(
            "M0,{} L{},{} L{},{} L0,{} Z",
            height, width, 0.0, width, height, height
        );
    }
    let mut path = eq_stroke_path(points, width, height);
    path.push_str(&format!(" L{},{} L0,{} Z", width, height, height));
    path
}

/// Compressor display — compact row inside the chain-cell.
/// Label + GR readout header + mini SVG transfer curve.
/// Mirrors VuPanel row style from InsightsPanel.
#[component]
pub fn CompDisplay(state: CompressorState) -> Element {
    let thresh_x = ((state.threshold_db + 40.0) / 40.0 * 200.0).clamp(0.0, 200.0);
    let readout = format!("{:+.1} GR", state.gain_reduction_db);

    rsx! {
        div { class: "dsp-module-row",

            div { class: "dsp-module-header",
                span { class: "dsp-module-label", "COMPRESSOR — OPTICAL" }
                span { class: "dsp-module-value dsp-module-value--comp", "{readout}" }
            }

            div { class: "dsp-module-graph",
                svg { view_box: "0 0 200 48", preserve_aspect_ratio: "none",
                    // threshold marker
                    line {
                        x1: "{thresh_x}", y1: "0",
                        x2: "{thresh_x}", y2: "48",
                        stroke: "rgba(0,152,152,0.25)",
                        stroke_width: "0.6",
                        stroke_dasharray: "2,2",
                    }
                    // curve fill
                    path {
                        d: "{comp_fill_path(&state.curve_points, 200.0, 48.0)}",
                        fill: "rgba(0,152,152,0.10)",
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points, 200.0, 48.0)}",
                        fill: "none",
                        stroke: "#009898",
                        stroke_width: "1.4",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}
