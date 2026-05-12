use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct TimecodeProps {
    pub value: String,
}

#[component]
pub fn TimecodeDisplay(props: TimecodeProps) -> Element {
    rsx! {
        span {
            class: "timecode-display",
            "{props.value}"
        }
    }
}
