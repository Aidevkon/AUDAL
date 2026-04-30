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
    rsx! {
        div { class: "intent-knob-container",
            div {
                class: "intent-knob-lcd",
                {
                    let val = props.angle / 27.0; // -135..135 → -5.0..+5.0
                    let sign = if val >= 0.0 { "+" } else { "" };
                    format!("{sign}{val:.1}")
                }
            }
            div { class: "intent-knob-label", "{props.label}" }
            div { 
                class: "intent-knob {hl_class}",
                onmousedown: move |e| props.on_down.call(e),
                div { 
                    class: "intent-knob-indicator {hl_class}",
                    style: "transform: rotate({props.angle}deg);"
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
