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

fn eq_fill_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{} L{},{} L0,{} Z", height/2.0, width, height/2.0, width, height/2.0, height/2.0); }
    let mut path = eq_stroke_path(points, width, height);
    path.push_str(&format!(" L{},{} L0,{} Z", width, height/2.0, height/2.0));
    path
}

#[component]
pub fn EQModule(state: EQState) -> Element {
    // Generate a pseudo-telemetry number from state
    let sum_db = state.low_db + state.mid_db + state.presence_db + state.air_db;
    let readout_val = format!("{:+.1}", sum_db);

    rsx! {
        div {
            class: "oled-tile oled-tile--eq",

            div { class: "oled-header",
                span { class: "oled-label", "EQ — 4-BAND PAR" }
                span { class: "oled-readout", "{readout_val}" }
            }
            div { class: "oled-content",
                svg { view_box: "0 0 240 60", preserve_aspect_ratio: "none",
                    // zero line
                    line { x1: "0", y1: "30", x2: "240", y2: "30",
                           stroke: "#0e1c28", stroke_width: "0.5" }
                    // curve fill
                    path {
                        d: "{eq_fill_path(&state.curve_points, 240.0, 60.0)}",
                        fill: "rgba(119,85,238,.1)"
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
        }
    }
}
