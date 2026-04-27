use dioxus::prelude::*;
use crate::components::screw::Screw;

#[derive(Props, PartialEq, Clone)]
pub struct ModuleFrameProps {
    /// The engraved title shown at the top of the module
    pub title: String,
    
    /// Optional specific class for the panel (e.g., "panel-session")
    #[props(default = "".to_string())]
    pub panel_class: String,
    
    /// Optional CSS style for the header (e.g., color, border-bottom)
    #[props(default = "".to_string())]
    pub header_style: String,

    /// Add overflow-y: auto styling for internal scrolling
    #[props(default = false)]
    pub is_scrollable: bool,

    /// The inner panel content
    pub children: Element,
}

#[component]
pub fn ModuleFrame(props: ModuleFrameProps) -> Element {
    let scroll_style = if props.is_scrollable {
        "flex: 1; overflow-y: auto; padding: 0;"
    } else {
        "flex: 1; overflow: hidden; padding: 0;"
    };

    rsx! {
        div { class: "panel-screw-wrapper",
            div { class: "mfd-panel {props.panel_class}",
            
                // ── Hardware physical cut-out header ──
                div {
                    class: "panel-title",
                    style: "{props.header_style}",
                    "{props.title}"
                }

                // ── The internal content area ──
                div {
                    style: "{scroll_style}",
                    {props.children}
                }

                // ── Hardware anchoring: 4 Corner Torx Screws ──
                Screw { top: 10, left: 10 }
                Screw { top: 10, right: 10 }
                Screw { bottom: 10, left: 10 }
                Screw { bottom: 10, right: 10 }
            }
        }
    }
}
