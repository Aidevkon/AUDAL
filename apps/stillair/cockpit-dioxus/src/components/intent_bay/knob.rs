use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct IntentKnobProps {
    pub label: &'static str,
    pub range: &'static str,
    pub highlighted: bool,
    pub angle: f32,
    pub on_down: EventHandler<MouseEvent>,
}

#[component]
pub fn IntentKnob(props: IntentKnobProps) -> Element {
    let hl_class = if props.highlighted { "highlighted" } else { "" };
    let socket_class = if props.highlighted {
        "intent-knob-socket socket-active"
    } else {
        "intent-knob-socket"
    };
    rsx! {
        div { class: "intent-knob-container",
            div { class: "intent-knob-label", "{props.label}" }
            // ── Cavity socket — conical sinkhole ──
            div { class: "{socket_class}",
                // ── Knob cap rotates entirely — conic-gradient brushing turns with it ──
                div {
                    class: "intent-knob {hl_class}",
                    style: "transform: rotate({props.angle}deg);",
                    onmousedown: move |e| props.on_down.call(e),
                    // Indicator is fixed on knob face — rotates with parent
                    div { class: "intent-knob-indicator {hl_class}" }
                }
            }
            div { class: "intent-knob-range", "{props.range}" }
        }
    }
}

pub fn normalize_angle(angle: f32) -> f32 {
    angle / 135.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intent_knob_normalization() {
        assert_eq!(normalize_angle(135.0), 1.0);
        assert_eq!(normalize_angle(-135.0), -1.0);
        assert_eq!(normalize_angle(0.0), 0.0);
    }
}
