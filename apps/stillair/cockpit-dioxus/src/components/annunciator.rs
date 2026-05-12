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

    /// Number of pipeline-progress pills to render (0 = none).
    /// Pills fill bottom-to-top: pill 0 is lowest (pipeline start).
    #[props(default = 0)]
    pub pipeline_stages: u8,
}

#[component]
pub fn Annunciator(props: AnnunciatorProps) -> Element {
    let mut led_class = "dsp-led".to_string();

    if props.is_master {
        led_class.push_str(" fm0-led");
    }

    if props.active {
        led_class.push_str(" active flicker-active");
    }

    rsx! {
        if props.is_master {
            div { class: "dsp-slot fm0-slot",
                span { class: "fm0-engraved-label", "{props.label}" }
                div { class: "fm0-pit",
                    div { class: "{led_class}" }
                }
            }
        } else {
            // Same column layout as transport-btn-col:
            // label (transport-top-label) above → housing (dsp-pit) below
            div { class: "dsp-slot-col",
                span { class: "transport-top-label", "{props.label}" }
                div { class: "dsp-pit",
                    div { class: "{led_class}" }
                    if props.pipeline_stages > 0 {
                        div { class: "dsp-sub-leds",
                            for i in 0..props.pipeline_stages {
                                div { class: "dsp-sub-led-col",
                                    div { class: if i == 0 { "dsp-sub-led-dot active" } else { "dsp-sub-led-dot" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
