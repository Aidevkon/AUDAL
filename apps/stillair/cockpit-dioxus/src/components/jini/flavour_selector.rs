use crate::state::presets::FLAVOURS;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniFlavourSelectorProps {
    pub on_select: EventHandler<String>,
}

#[component]
pub fn JiniFlavourSelector(props: JiniFlavourSelectorProps) -> Element {
    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:8px;",
            p { class: "jini-text", "How do you want it to sound?" }
            div { style: "display:flex; gap:8px;",
                for f in FLAVOURS {
                    {
                        let id = f.id;
                        let on_select = props.on_select;
                        rsx! {
                            button {
                                onclick: move |_| on_select.call(id.to_string()),
                                "{f.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
