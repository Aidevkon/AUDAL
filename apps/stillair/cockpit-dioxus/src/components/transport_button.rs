use dioxus::prelude::*;

#[derive(PartialEq, Clone)]
pub enum LedColor {
    Amber,
    Red,
}

impl LedColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            LedColor::Amber => "led-amber",
            LedColor::Red => "led-red",
        }
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct TransportActuatorProps {
    pub label: String,
    pub color: LedColor,
    pub active: bool,
    pub on_click: EventHandler<MouseEvent>,
}

#[allow(non_snake_case)]
pub fn TransportActuator(props: TransportActuatorProps) -> Element {
    let active_class = if props.active { "active" } else { "" };
    let color_class = props.color.as_str();

    rsx! {
        button {
            class: "transport-actuator {color_class} {active_class}",
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
