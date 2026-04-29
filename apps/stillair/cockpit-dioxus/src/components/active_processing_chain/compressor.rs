use dioxus::prelude::*;
use super::types::CompressorState;
use crate::components::module_frame::ModuleFrame;
use super::eq::eq_stroke_path;

pub fn comp_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{} L{},{} L0,{} Z", height, width, 0.0, width, height, height); }
    let mut path = eq_stroke_path(points, width, height);
    path.push_str(&format!(" L{},{} L0,{} Z", width, height, height));
    path
}

#[component]
pub fn CompressorModule(state: CompressorState) -> Element {
    let thresh_x = ((state.threshold_db + 40.0) / 40.0 * 240.0).clamp(0.0, 240.0);
    
    // Pseudo telemetry number
    let readout_val = format!("{:+.1}", state.gain_reduction_db);

    rsx! {
        div {
            class: "chain-module-container chain-comp-panel",
            
            // 4 corner screws
            crate::components::screw::Screw { top: 6, left: 6 }
            crate::components::screw::Screw { top: 6, right: 6 }
            crate::components::screw::Screw { bottom: 6, left: 6 }
            crate::components::screw::Screw { bottom: 6, right: 6 }

            div { class: "chain-header",
                span { class: "chain-header-title", "COMPRESSOR - OPTICAL" }
                span { class: "chain-ro-badge", "{readout_val}" }
            }

            div { class: "chain-oled-container",
                svg { class: "chain-svg", view_box: "0 0 240 60", preserve_aspect_ratio: "none",
                    // threshold line
                    line { x1: "{thresh_x}", y1: "0", x2: "{thresh_x}", y2: "60",
                           stroke: "rgba(0,152,152,.2)", stroke_width: "0.5", stroke_dasharray: "2,2" }
                    // curve fill
                    path {
                        d: "{comp_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(0,152,152,.1)"
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "none",
                        stroke: "#009898",
                        stroke_width: "1.5"
                    }
                }
            }
        }
    }
}
