use dioxus::prelude::*;
use super::types::LimiterState;
use crate::components::module_frame::ModuleFrame;
use super::eq::eq_stroke_path;

pub fn lim_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{} L{},{} L0,{} Z", height, width, 0.0, width, height, height); }
    let mut path = eq_stroke_path(points, width, height);
    path.push_str(&format!(" L{},{} L0,{} Z", width, height, height));
    path
}

#[component]
pub fn LimiterModule(state: LimiterState) -> Element {
    let ceil_y = (state.ceiling_dbtp.abs() / 12.0 * 60.0).clamp(0.0, 60.0);
    
    // Pseudo telemetry readout
    // In actual implementation, we might want to sum up real gain reduction from limit block
    let readout_val = format!("{:.2}", state.ceiling_dbtp); // just showing mock ceiling

    rsx! {
        div {
            class: "chain-module-container chain-limit-panel",
            
            // 4 corner screws
            crate::components::screw::Screw { top: 6, left: 6 }
            crate::components::screw::Screw { top: 6, right: 6 }
            crate::components::screw::Screw { bottom: 6, left: 6 }
            crate::components::screw::Screw { bottom: 6, right: 6 }

            div { class: "chain-header",
                span { class: "chain-header-title", "LIMITER - BRICKWALL" }
                span { class: "chain-ro-badge", "{readout_val}" }
            }

            div { class: "chain-oled-container",
                svg { class: "chain-svg", view_box: "0 0 240 60", preserve_aspect_ratio: "none",
                    // ceiling line
                    line { x1: "0", y1: "{ceil_y}", x2: "240", y2: "{ceil_y}",
                           stroke: "rgba(232,56,32,.35)", stroke_width: "1.0", stroke_dasharray: "3,2" }
                    text { x: "238", y: "{ceil_y - 2.0}", fill: "rgba(232,56,32,.45)", font_size: "6px", text_anchor: "end", "{state.ceiling_dbtp:.1} dBTP" }
                    // curve fill
                    path {
                        d: "{lim_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(232,56,32,.1)"
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "none",
                        stroke: "#e83820",
                        stroke_width: "1.5"
                    }
                }
            }
        }
    }
}
