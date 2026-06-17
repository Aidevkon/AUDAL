use dioxus::prelude::*;

#[component]
pub fn JiniDropZone() -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center;",
            p { class: "jini-text", "Drop your audio here." }
        }
    }
}
