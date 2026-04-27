use dioxus::prelude::*;

#[component]
pub fn ReadoutItem(label: String, value: String, lit: bool) -> Element {
    rsx! {
        div { class: "cr",
            span { class: "cr-l", "{label}" }
            span { class: if lit { "cr-v lit" } else { "cr-v" }, "{value}" }
        }
    }
}
