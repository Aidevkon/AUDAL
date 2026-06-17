use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniAwaitingMoreProps {
    pub on_proceed: EventHandler<()>,
}

#[component]
pub fn JiniAwaitingMore(props: JiniAwaitingMoreProps) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:8px;",
            p { class: "jini-text", "Waiting for more tracks..." }
            button {
                onclick: move |_| props.on_proceed.call(()),
                "[+] Add"
            }
        }
    }
}
