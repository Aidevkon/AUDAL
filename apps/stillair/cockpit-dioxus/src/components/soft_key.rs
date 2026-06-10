use dioxus::prelude::*;

#[derive(Default, PartialEq, Clone, Copy, Debug)]
pub enum SoftKeyVariant {
    #[default]
    Standard, // Grey
    Active, // Amber
    Danger, // Red
}

#[derive(Props, PartialEq, Clone)]
pub struct SoftKeyProps {
    /// The button text or content
    pub label: String,

    /// Triggered state -> true if it should appear "pressed" or "activated"
    #[props(default = false)]
    pub active: bool,

    /// Disabled state
    #[props(default = false)]
    pub disabled: bool,

    /// Color and usage variant
    #[props(default = Default::default())]
    pub variant: SoftKeyVariant,

    /// If true, adds the guard rails to the left and right
    #[props(default = false)]
    pub is_guarded: bool,

    /// Optional tooltip
    #[props(default = "".to_string())]
    pub title: String,

    /// Click handler
    pub onclick: EventHandler<MouseEvent>,
}

#[component]
pub fn SoftKey(props: SoftKeyProps) -> Element {
    // We will build the button's class based on variant and state.
    let base_class = match props.variant {
        SoftKeyVariant::Standard => "btn-avionics btn-standard",
        SoftKeyVariant::Active => "btn-avionics btn-active",
        SoftKeyVariant::Danger => "btn-abort",
    };

    let active_class = if props.active { "is-pressed" } else { "" };

    let button_element = rsx! {
        button {
            class: "{base_class} {active_class}",
            title: "{props.title}",
            disabled: props.disabled,
            onclick: move |evt| props.onclick.call(evt),
            "{props.label}"
        }
    };

    // If guarded, wrap in the module cavity with guard rails
    if props.is_guarded {
        rsx! {
            div { class: "abort-module recessed-well",
                div { class: "guard-rail" }
                {button_element}
                div { class: "guard-rail" }
            }
        }
    } else {
        button_element
    }
}
