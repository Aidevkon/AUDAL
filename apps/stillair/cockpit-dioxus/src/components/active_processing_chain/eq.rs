use dioxus::prelude::*;
use super::types::EQState;
use super::readout::ReadoutItem;

pub fn eq_stroke_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{}", height/2.0, width, height/2.0); }
    let first = points[0];
    let mut path = format!("M{},{}", first.0 * width, first.1 * height);
    for p in &points[1..] {
        path.push_str(&format!(" L{},{}", p.0 * width, p.1 * height));
    }
    path
}

fn eq_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{} L{},{} L0,{} Z", height/2.0, width, height/2.0, width, height/2.0, height/2.0); }
    let mut path = eq_stroke_path(points, width, height);
    path.push_str(&format!(" L{},{} L0,{} Z", width, height/2.0, height/2.0));
    path
}

#[component]
pub fn EQModule(state: EQState) -> Element {
    rsx! {
        div { class: "chain-module",
            div { class: "cm-header",
                span { class: "cm-num", "01" }
                span { class: "cm-name", "EQ" }
                span { class: "cm-type", "4-Band PAR" }
            }
            div { class: "cm-plot",
                div { class: "cm-grid" }
                svg { class: "cm-svg", view_box: "0 0 240 60",
                    // zero line
                    line { x1: "0", y1: "30", x2: "240", y2: "30",
                           stroke: "#0e1c28", stroke_width: "0.5" }
                    // curve fill
                    path {
                        d: "{eq_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(119,85,238,.07)"
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "none",
                        stroke: "#7755ee",
                        stroke_width: "1.5"
                    }
                }
            }
            div { class: "cm-reads",
                ReadoutItem { label: "LOW".to_string(), value: format!("{:+.1}", state.low_db), lit: state.low_db.abs() > 0.5 }
                ReadoutItem { label: "MID".to_string(), value: format!("{:+.1}", state.mid_db), lit: state.mid_db.abs() > 0.5 }
                ReadoutItem { label: "PRES".to_string(), value: format!("{:+.1}", state.presence_db), lit: state.presence_db.abs() > 0.5 }
                ReadoutItem { label: "AIR".to_string(), value: format!("{:+.1}", state.air_db), lit: state.air_db.abs() > 0.5 }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eq_empty_curve() {
        assert_eq!(eq_stroke_path(&[], 240.0, 60.0), "M0,30 L240,30");
        assert_eq!(eq_fill_path(&[], 240.0, 60.0), "M0,30 L240,30 L240,30 L0,30 Z");
    }
}
