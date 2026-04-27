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
}

#[component]
pub fn Annunciator(props: AnnunciatorProps) -> Element {
    let mut slot_class = "dsp-slot".to_string();
    let mut led_class = "dsp-led".to_string();
    
    if props.is_master {
        slot_class.push_str(" fm0-slot");
        led_class.push_str(" fm0-led");
    }
    
    if props.active {
        // We use flicker-active which in CSS will trigger a 0.1s flicker animation
        led_class.push_str(" active flicker-active");
    }

    rsx! {
        div { class: "{slot_class}",
            span { class: "dsp-label", "{props.label}" }
            div { class: "{led_class}" }
        }
    }
}
