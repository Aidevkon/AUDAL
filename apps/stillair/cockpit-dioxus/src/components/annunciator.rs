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

    /// True = SEARCH state (ping-pong bar, no full bloom).
    /// False + active = LOCK state (full bloom + stable underline).
    /// False + !active = IDLE state (no animation, no glow).
    #[props(default = false)]
    pub searching: bool,
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

    // State modifier applied to the pit container for ::after animation
    let state_mod = if props.active && props.searching {
        " searching"
    } else if props.active {
        " locked"
    } else {
        ""
    };

    let fm0_pit_class = format!("fm0-pit{}", state_mod);
    let dsp_pit_class = format!("dsp-pit{}", state_mod);

    rsx! {
        if props.is_master {
            div { class: "dsp-slot fm0-slot",
                span { class: "fm0-engraved-label", "{props.label}" }
                div { class: "{fm0_pit_class}",
                    div { class: "{led_class}" }
                }
            }
        } else {
            // Same column layout as transport-btn-col:
            // label (transport-top-label) above → housing (dsp-pit) below
            div { class: "dsp-slot-col",
                span { class: "transport-top-label", "{props.label}" }
                div { class: "{dsp_pit_class}",
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
