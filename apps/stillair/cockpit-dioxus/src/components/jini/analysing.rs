use dioxus::prelude::*;

#[component]
pub fn JiniAnalysing() -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center;",
            p { class: "jini-text", "Analysing." }
        }
    }
}
