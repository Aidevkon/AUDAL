use crate::state::presets::PLATFORMS;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniPlatformSelectorProps {
    pub on_select: EventHandler<String>,
}

#[component]
pub fn JiniPlatformSelector(props: JiniPlatformSelectorProps) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:8px;",
            p { class: "jini-text", "Where is this going?" }
            div { style: "display:flex; gap:8px;",
                for p in PLATFORMS {
                    {
                        let id = p.id;
                        let on_select = props.on_select;
                        rsx! {
                            button {
                                onclick: move |_| on_select.call(id.to_string()),
                                "{p.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
