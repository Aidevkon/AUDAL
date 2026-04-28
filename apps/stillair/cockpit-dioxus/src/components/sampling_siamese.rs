use dioxus::prelude::*;
use crate::panels::insights::InsightsPanel;
use crate::panels::session::SessionPanel;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, SessionStateJson, VisualizationDataJson};

#[derive(Props, Clone, PartialEq)]
pub struct SamplingSiameseProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
    pub playback_state: Signal<Option<PlaybackStateJson>>,
    pub viz_data: Signal<Option<VisualizationDataJson>>,
    pub show_mastered: Signal<bool>,
}

#[component]
pub fn SamplingSiamese(props: SamplingSiameseProps) -> Element {
    rsx! {
        div {
            class: "sampling-siamese",
            
            // INSIGHTS - LEFT (25%)
            div { class: "siamese-col siamese-insights",
                InsightsPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    playback_state: props.playback_state,
                    viz_data: props.viz_data,
                }
            }

            // JOIN VERTICAL SLOT (1px)
            div { class: "siamese-join" }

            // SESSION - CENTER (50%)
            div { class: "siamese-col siamese-session",
                SessionPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    viz_data: props.viz_data,
                    show_mastered: props.show_mastered,
                }
            }
            
            // MASTERING CHAIN - RIGHT (25% Empty space reserved for later)
            div { class: "siamese-col siamese-reserved" }
        }
    }
}
