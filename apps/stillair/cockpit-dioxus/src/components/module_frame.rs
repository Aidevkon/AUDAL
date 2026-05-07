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

    /// Optional element to project to the right of the title (e.g. readouts)
    #[props(default = None)]
    pub right_header: Option<Element>,

    /// The inner panel content
    pub children: Element,
    /// Show corner screws (default: true — set false for moving panels)
    #[props(default = false)]
    pub show_screws: bool,
}

#[component]
pub fn ModuleFrame(props: ModuleFrameProps) -> Element {
    let scroll_style = if props.is_scrollable {
        "flex: 1; overflow-y: auto; padding: 0;"
    } else {
        "flex: 1; overflow: hidden; padding: 0;"
    };

    rsx! {
        div { class: "mfd-panel {props.panel_class}",

            // ── Hardware physical cut-out header ──
            div {
                class: "panel-title",
                style: "{props.header_style}",
                div { class: "panel-title-left", "{props.title}" }
                if let Some(right) = props.right_header {
                    div { class: "panel-title-right", {right} }
                }
            }

            // ── The internal content area ──
            div {
                class: "module-screen",
                style: "{scroll_style}",
                {props.children}
            }

            // ── Hardware anchoring: 4 Corner Torx Screws ──
            if props.show_screws {
                Screw { top: 10, left: 10 }
                Screw { top: 10, right: 10 }
                Screw { bottom: 10, left: 10 }
                Screw { bottom: 10, right: 10 }
            }
        }
    }
}
