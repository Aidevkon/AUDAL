use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniDropZoneProps {
    pub on_browse: EventHandler<()>,
}

#[component]
pub fn JiniDropZone(props: JiniDropZoneProps) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center; cursor:pointer;",
            onclick: move |_| props.on_browse.call(()),
            p { class: "jini-text", "Drop audio, or click to browse." }
        }
    }
}
