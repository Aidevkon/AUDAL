use dioxus::prelude::*;

#[component]
pub fn Damper() -> Element {
    rsx! {
        svg {
            width: "120", height: "16", view_box: "0 0 120 16",
            polyline { points: "10,8 16,4 22,12 28,4 34,12 40,8", fill: "none", stroke: "#1e3448", stroke_width: "1" }
            line { x1: "40", y1: "8", x2: "50", y2: "8", stroke: "#162738", stroke_width: "1" }
            rect { x: "50", y: "4", width: "20", height: "8", fill: "#0e1c28", stroke: "#1a2e42", stroke_width: "1" }
            rect { x: "58", y: "2", width: "4", height: "12", fill: "#2a3e52" }
            line { x1: "70", y1: "8", x2: "80", y2: "8", stroke: "#162738", stroke_width: "1" }
            polyline { points: "80,8 86,4 92,12 98,4 104,12 110,8", fill: "none", stroke: "#1e3448", stroke_width: "1" }
        }
    }
}
