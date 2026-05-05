pub mod knob;

use dioxus::prelude::*;
use knob::IntentKnob;

#[derive(Props, Clone, PartialEq)]
pub struct IntentBayProps {
    pub open: bool,
    pub tone_angle: f32,
    pub dyn_angle: f32,
    pub space_angle: f32,
    pub loud_angle: f32,
    pub on_down_tone: EventHandler<MouseEvent>,
    pub on_down_dyn: EventHandler<MouseEvent>,
    pub on_down_space: EventHandler<MouseEvent>,
    pub on_down_loud: EventHandler<MouseEvent>,
}

#[component]
pub fn IntentBay(props: IntentBayProps) -> Element {
    rsx! {
        div { class: "intent-bay",

            // ── Knob Grid ──
            div { class: "intent-knobs-wrapper",
                div { class: "intent-knobs",
                    IntentKnob {
                        label: "TONE",
                        range: "Warm ↔ Bright",
                        angle: props.tone_angle,
                        highlighted: true,
                        on_down: move |e| props.on_down_tone.call(e)
                    }
                    IntentKnob {
                        label: "DYNAMICS",
                        range: "Punch ↔ Glue",
                        angle: props.dyn_angle,
                        highlighted: false,
                        on_down: move |e| props.on_down_dyn.call(e)
                    }
                    IntentKnob {
                        label: "SPACE",
                        range: "Center ↔ Widen",
                        angle: props.space_angle,
                        highlighted: false,
                        on_down: move |e| props.on_down_space.call(e)
                    }
                    IntentKnob {
                        label: "LOUDNESS",
                        range: "Gain ↔ Ceiling",
                        angle: props.loud_angle,
                        highlighted: false,
                        on_down: move |e| props.on_down_loud.call(e)
                    }
                }
            }
        }
    }
}
