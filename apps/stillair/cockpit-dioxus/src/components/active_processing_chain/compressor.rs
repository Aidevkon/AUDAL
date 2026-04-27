use dioxus::prelude::*;
use super::types::CompressorState;
use super::readout::ReadoutItem;
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
    
    rsx! {
        div { class: "chain-module",
            div { class: "cm-header",
                span { class: "cm-num", "02" }
                span { class: "cm-name", "COMPRESSOR" }
                span { class: "cm-type", "VCA BUS" }
            }
            div { class: "cm-plot",
                div { class: "cm-grid" }
                svg { class: "cm-svg", view_box: "0 0 240 60",
                    // threshold line
                    line { x1: "{thresh_x}", y1: "0", x2: "{thresh_x}", y2: "60",
                           stroke: "rgba(0,152,152,.2)", stroke_width: "0.5", stroke_dasharray: "2,2" }
                    // curve fill
                    path {
                        d: "{comp_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(0,152,152,.06)"
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
            div { class: "cm-reads",
                ReadoutItem { label: "THR".to_string(), value: format!("{:.1}", state.threshold_db), lit: state.threshold_db < -0.1 }
                ReadoutItem { label: "RATIO".to_string(), value: format!("{:.1}:1", state.ratio), lit: state.ratio > 1.0 }
                ReadoutItem { label: "GR".to_string(), value: format!("{:+.1}", state.gain_reduction_db), lit: state.gain_reduction_db < -0.1 }
                ReadoutItem { label: "MAKEUP".to_string(), value: format!("{:+.1}", state.makeup_db), lit: state.makeup_db > 0.1 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compressor_empty_curve() {
        assert_eq!(comp_fill_path(&[], 240.0, 60.0), "M0,60 L240,0 L240,60 L0,60 Z");
    }
}
