use dioxus::prelude::*;

#[derive(PartialEq, Clone)]
pub enum LedColor {
    Amber,
    Red,
    Neutral,
}

impl LedColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            LedColor::Amber => "led-amber",
            LedColor::Red => "led-red",
            LedColor::Neutral => "led-neutral",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct PlayActuatorProps {
    pub label: String,
    pub color: LedColor,
    pub active: bool,
    pub on_click: EventHandler<MouseEvent>,
}

#[allow(non_snake_case)]
pub fn PlayActuator(props: PlayActuatorProps) -> Element {
    let active_class = if props.active { "active" } else { "" };
    let color_class = props.color.as_str();

    rsx! {
        button {
            class: "play-actuator {active_class}",
            onclick: move |evt| props.on_click.call(evt),
            // The matte, light-absorbing outer chassis
            div { class: "pa-housing",
                // The translucent, beveled top acrylic
                div { class: "pa-lens",
                    // The directional, blooming LED light plate
                    div { class: "pa-led" }
                    // The etched/engraved physical label
                    span { class: "pa-label", "{props.label}" }
                }
            }
        }
    }
}
