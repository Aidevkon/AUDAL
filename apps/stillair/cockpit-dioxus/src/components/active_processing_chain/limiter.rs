use dioxus::prelude::*;
use super::types::LimiterState;
use super::readout::ReadoutItem;
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
    
    rsx! {
        div { class: "chain-module",
            div { class: "cm-header",
                span { class: "cm-num", "03" }
                span { class: "cm-name", "LIMITER" }
                span { class: "cm-type", "LOOKAHEAD BUS" }
            }
            div { class: "cm-plot",
                div { class: "cm-grid" }
                svg { class: "cm-svg", view_box: "0 0 240 60",
                    // ceiling line
                    line { x1: "0", y1: "{ceil_y}", x2: "240", y2: "{ceil_y}",
                           stroke: "rgba(232,56,32,.35)", stroke_width: "1.0", stroke_dasharray: "3,2" }
                    text { x: "238", y: "{ceil_y - 2.0}", fill: "rgba(232,56,32,.45)", font_size: "6px", text_anchor: "end", "{state.ceiling_dbtp:.1} dBTP" }
                    // curve fill
                    path {
                        d: "{lim_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(232,56,32,.05)"
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
            div { class: "cm-reads",
                ReadoutItem { label: "CEIL".to_string(), value: format!("{:.1}", state.ceiling_dbtp), lit: state.ceiling_dbtp < -0.1 }
                ReadoutItem { label: "REL".to_string(), value: if state.release_auto { "AUTO".to_string() } else { "MAN".to_string() }, lit: state.release_auto }
                ReadoutItem { label: "ISP".to_string(), value: format!("{}x", state.isp_factor), lit: state.isp_factor > 1 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limiter_empty_curve() {
        assert_eq!(lim_fill_path(&[], 240.0, 60.0), "M0,60 L240,0 L240,60 L0,60 Z");
    }
}
