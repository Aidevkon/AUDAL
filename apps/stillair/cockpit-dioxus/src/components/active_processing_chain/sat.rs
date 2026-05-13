use dioxus::prelude::*;
use super::types::SatTelemetry;

#[component]
pub fn SatModule(state: SatTelemetry) -> Element {
    // Build SVG polyline points from knee_curve [f32; 64]
    // X: evenly spaced across 168px (70% of 240px viewBox)
    // Y: inverted — SVG top = 0, so (1.0 - y) * 60.0
    let knee_points: String = state.knee_curve
        .iter()
        .enumerate()
        .map(|(i, &y)| {
            let x = i as f32 / 63.0 * 168.0;
            let sy = (1.0 - y) * 60.0;
            format!("{:.1},{:.1}", x, sy)
        })
        .collect::<Vec<_>>()
        .join(" ");

    // THD bar: 0%→12% maps to 0→60px height, anchored at bottom
    let thd_bar_h = (state.thd_percent / 12.0 * 60.0).clamp(0.0, 60.0);
    let thd_bar_y = 60.0 - thd_bar_h;

    rsx! {
        div {
            class: "oled-tile sat-tile",

            div { class: "oled-header",
                span { class: "oled-label", "SAT — HARMONICS" }
                span { class: "oled-readout", "THD {state.thd_percent:.1}%" }
            }

            div { class: "oled-subheader",
                span { class: "oled-sub", "HEADROOM {state.headroom_db:.1} dB" }
            }

            div { class: "oled-content",
                // Knee curve — soft-clip transfer function (70% width)
                svg { class: "sat-curve", view_box: "0 0 168 60",
                      preserve_aspect_ratio: "none",
                    // unity line — linear passthrough reference
                    line { x1: "0", y1: "60", x2: "168", y2: "0",
                           stroke: "#1a2a1a", stroke_width: "0.5" }
                    // soft-clip knee curve
                    polyline {
                        points: "{knee_points}",
                        fill: "none",
                        stroke: "#c8a020",
                        stroke_width: "1.5",
                        stroke_linejoin: "round"
                    }
                }
                // THD vertical bar (30% width)
                svg { class: "sat-thd", view_box: "0 0 72 60",
                      preserve_aspect_ratio: "none",
                    // bar fill
                    rect {
                        x: "20", y: "{thd_bar_y}",
                        width: "32", height: "{thd_bar_h}",
                        fill: "rgba(232,80,32,0.25)"
                    }
                    // bar top edge marker
                    line {
                        x1: "20", y1: "{thd_bar_y}",
                        x2: "52", y2: "{thd_bar_y}",
                        stroke: "#e85020", stroke_width: "1.0"
                    }
                }
            }
        }
    }
}
