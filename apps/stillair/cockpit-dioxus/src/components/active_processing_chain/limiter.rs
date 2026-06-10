use super::eq::eq_stroke_path;
use super::types::LimiterState;
use dioxus::prelude::*;

pub fn lim_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
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

/// Limiter display — compact row inside the chain-cell.
/// Label + ceiling readout header + mini SVG curve with ceiling line.
/// Mirrors VuPanel row style from InsightsPanel.
#[component]
pub fn LimDisplay(state: LimiterState) -> Element {
    let ceil_y = (state.ceiling_dbtp.abs() / 12.0 * 48.0).clamp(0.0, 48.0);
    let readout = format!("{:.2} dBTP", state.ceiling_dbtp);
    let isp = if state.release_auto {
        format!("ISP×{} AUTO", state.isp_factor)
    } else {
        format!("ISP×{}", state.isp_factor)
    };

    rsx! {
        div { class: "dsp-module-row",

            div { class: "dsp-module-header",
                span { class: "dsp-module-label", "LIMITER — BRICKWALL" }
                span { class: "dsp-module-value dsp-module-value--lim", "{readout}" }
            }

            div { class: "dsp-module-graph",
                svg { view_box: "0 0 200 48", preserve_aspect_ratio: "none",
                    // ceiling line
                    line {
                        x1: "0", y1: "{ceil_y}",
                        x2: "200", y2: "{ceil_y}",
                        stroke: "rgba(232,56,32,0.40)",
                        stroke_width: "0.8",
                        stroke_dasharray: "3,2",
                    }
                    // ceiling label
                    text {
                        x: "198", y: "{ceil_y - 2.0}",
                        fill: "rgba(232,56,32,0.50)",
                        font_size: "5px",
                        text_anchor: "end",
                        "{isp}"
                    }
                    // curve fill
                    path {
                        d: "{lim_fill_path(&state.curve_points, 200.0, 48.0)}",
                        fill: "rgba(232,56,32,0.10)",
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points, 200.0, 48.0)}",
                        fill: "none",
                        stroke: "#e83820",
                        stroke_width: "1.4",
                        stroke_linejoin: "round",
                    }
                }
            }
        }
    }
}
