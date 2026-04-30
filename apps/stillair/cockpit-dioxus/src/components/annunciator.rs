use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct AnnunciatorProps {
    /// The label above the LED (e.g., "FM0", "EQ")
    pub label: String,
    
    /// Is the LED lit/active?
    #[props(default = false)]
    pub active: bool,
    
    /// True if this is the master indicator (larger size, FM0)
    #[props(default = false)]
    pub is_master: bool,

    /// Optional sub-labels for the micro-LED pipeline indicators
    #[props(default = None)]
    pub sub_labels: Option<Vec<String>>,
}

#[component]
pub fn Annunciator(props: AnnunciatorProps) -> Element {
    let mut slot_class = "dsp-slot".to_string();
    let mut led_class = "dsp-led".to_string();
    
    if props.is_master {
        slot_class.push_str(" fm0-slot");
        led_class.push_str(" fm0-led");
    } else {
        led_class.push_str(" dsp-square-btn");
    }
    
    if props.active {
        // We use flicker-active which in CSS will trigger a 0.1s flicker animation
        led_class.push_str(" active flicker-active");
    }

    rsx! {
        div { class: "{slot_class}",
            if props.is_master {
                div { class: "fm0-screw top-left" }
                div { class: "fm0-screw top-right" }
                div { class: "fm0-screw bottom-left" }
                div { class: "fm0-screw bottom-right" }
                span { class: "fm0-engraved-label", "{props.label}" }
                div { class: "fm0-pit",
                    div { class: "{led_class}" }
                }
            } else {
                span { class: "dsp-label", "{props.label}" }
                div { class: "{led_class}" }
                if let Some(ref sub) = props.sub_labels {
                    div {
                        class: "dsp-sub-leds",
                        for label in sub {
                            div { class: "dsp-sub-led-col",
                                div { class: "dsp-sub-led-dot" }
                                span { class: "dsp-sub-led-label", "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
