use dioxus::prelude::*;

#[component]
pub fn JourneyView() -> Element {
    rsx! {
        div { class: "journey-view-overlay",
            style: "width: 100%; height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; background: transparent;",
            div { style: "font-family: 'Share Tech Mono', monospace; font-size: 24px; color: var(--accent-cyan); letter-spacing: 0.1em; margin-bottom: 12px;",
                "THE SIGNAL JOURNEY"
            }
            div { style: "font-family: 'Barlow Condensed', sans-serif; font-size: 16px; color: var(--text-muted); letter-spacing: 0.05em;",
                "Pre-Analysis → Stems → Markov → DSP → Spatial → Certified"
            }
        }
    }
}
