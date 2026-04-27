use dioxus::prelude::*;

#[derive(Props, PartialEq, Clone)]
pub struct ScrewProps {
    pub top: Option<i32>,
    pub bottom: Option<i32>,
    pub left: Option<i32>,
    pub right: Option<i32>,
}

#[component]
pub fn Screw(props: ScrewProps) -> Element {
    let mut style = String::new();
    if let Some(t) = props.top {
        style.push_str(&format!("top: {}px; ", t));
    }
    if let Some(b) = props.bottom {
        style.push_str(&format!("bottom: {}px; ", b));
    }
    if let Some(l) = props.left {
        style.push_str(&format!("left: {}px; ", l));
    }
    if let Some(r) = props.right {
        style.push_str(&format!("right: {}px; ", r));
    }

    rsx! {
        div {
            class: "screw",
            style: "{style}",
        }
    }
}
