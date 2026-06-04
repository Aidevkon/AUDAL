use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JourneyViewProps {
    pub stage: Signal<String>,
    pub elapsed_ms: Signal<u64>,
}

#[component]
pub fn JourneyView(props: JourneyViewProps) -> Element {
    let stages = ["INITIALIZING", "ANALYZING", "STEMS", "MARKOV", "DSP", "SPATIAL", "CERTIFIED"];
    let current_stage = props.stage.read().clone();
    let current_stage_str = current_stage.as_str();
    let current_idx = stages.iter().position(|&s| s == current_stage_str).unwrap_or(0);
    let elapsed_str = format!("{} ms", *props.elapsed_ms.read());

    rsx! {
        div { class: "journey-view-overlay",
            div { class: "journey-title",
                "THE SIGNAL JOURNEY"
            }
            div { class: "journey-stages",
                for (i, s) in stages.into_iter().enumerate() {
                    div {
                        key: "{s}",
                        class: format!("journey-stage {}", if i < current_idx { "done" } else if i == current_idx { "active" } else { "" }),
                        div { class: "journey-stage-dot" }
                        "{s}"
                    }
                }
            }
            div { class: "journey-elapsed",
                "{elapsed_str}"
            }
        }
    }
}
