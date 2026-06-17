use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniDetectionProps {
    pub track_count: usize,
}

#[component]
pub fn JiniDetection(props: JiniDetectionProps) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center;",
            p { class: "jini-text",
                if props.track_count == 1 { "Single track." }
                else { "Album. {props.track_count} tracks." }
            }
        }
    }
}
