use dioxus::prelude::*;
use super::types::SatTelemetry;

/// SAT scope cell — mirrors StereoScope in InsightsPanel.
/// Renders the soft-clip knee curve + THD bar as a scope-style visualization.
#[component]
pub fn SatDisplay(state: SatTelemetry) -> Element {
    // Build polyline points from knee_curve [f32; 64]
    // viewBox 0 0 120 120 — mirrors StereoScope viewBox
    let knee_points: String = state.knee_curve
        .iter()
        .enumerate()
        .map(|(i, &y)| {
            let x = i as f32 / 63.0 * 120.0;
            let sy = (1.0 - y) * 120.0;  // invert Y
            format!("{:.1},{:.1}", x, sy)
        })
        .collect::<Vec<_>>()
        .join(" ");

    // THD bar: 0%→12% maps to 0→120px height, anchored at bottom
    let thd_bar_h = (state.thd_percent / 12.0 * 90.0).clamp(0.0, 90.0);
    let thd_bar_y = 120.0 - thd_bar_h;

    rsx! {
        div { class: "dsp-sat-wrap",

            svg {
                view_box: "0 0 120 120",
                xmlns: "http://www.w3.org/2000/svg",
                style: "filter: drop-shadow(0 0 3px rgba(200,160,32,0.35));",

                // Scope grid — mirrors StereoGrid
                SatScopeGrid {}

                // Unity / linear reference diagonal
                line {
                    x1: "0", y1: "120", x2: "120", y2: "0",
                    stroke: "rgba(200,160,32,0.15)",
                    stroke_width: "0.6",
                    stroke_dasharray: "2,3",
                }

                // Knee curve — soft-clip transfer
                polyline {
                    points: "{knee_points}",
                    fill: "none",
                    stroke: "#c8a020",
                    stroke_width: "1.8",
                    stroke_linejoin: "round",
                    opacity: "0.9",
                }

                // THD bar — bottom-right indicator (mirrors StereoScope inner)
                rect {
                    x: "95", y: "{thd_bar_y}",
                    width: "10", height: "{thd_bar_h}",
                    fill: "rgba(232,80,32,0.20)",
                }
                line {
                    x1: "95", y1: "{thd_bar_y}",
                    x2: "105", y2: "{thd_bar_y}",
                    stroke: "#e85020", stroke_width: "1.2",
                    opacity: "0.9",
                }

                // Center hotspot
                circle {
                    cx: "0", cy: "120",
                    r: "2",
                    fill: "#c8a020",
                    opacity: "1.0",
                }
            }

            // Footer: THD + headroom readouts
            div { class: "dsp-sat-footer",
                span { class: "dsp-sat-label", "THD {state.thd_percent:.1}%" }
                span { class: "dsp-sat-label",
                    style: "color: rgba(232,80,32,0.65);",
                    "HR {state.headroom_db:.1}dB"
                }
            }
        }
    }
}

/// Scope grid — mirrors StereoGrid (diagonal grid for transfer curve)
#[component]
fn SatScopeGrid() -> Element {
    rsx! {
        g {
            stroke: "#c8a020",
            stroke_width: "0.6",
            opacity: "0.12",
            fill: "none",
            // Horizontal reference lines
            line { x1: "0", y1: "30",  x2: "120", y2: "30"  }
            line { x1: "0", y1: "60",  x2: "120", y2: "60"  }
            line { x1: "0", y1: "90",  x2: "120", y2: "90"  }
            // Vertical reference lines
            line { x1: "30",  y1: "0", x2: "30",  y2: "120" }
            line { x1: "60",  y1: "0", x2: "60",  y2: "120" }
            line { x1: "90",  y1: "0", x2: "90",  y2: "120" }
        }
    }
}
