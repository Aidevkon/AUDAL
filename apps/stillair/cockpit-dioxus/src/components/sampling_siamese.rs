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
            
            // SESSION - LEFT (PSA)
            div { class: "siamese-col siamese-session",
                SessionPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    viz_data: props.viz_data,
                    show_mastered: props.show_mastered,
                }
            }



            // INSIGHTS - CENTER (Spectral Dynamics)
            div { class: "siamese-col siamese-insights",
                InsightsPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    playback_state: props.playback_state,
                    viz_data: props.viz_data,
                }
            }
            
            // MASTERING CHAIN - RIGHT (25%)
            div { class: "siamese-col siamese-reserved",
                crate::components::active_processing_chain::ActiveProcessingChain {
                    eq: crate::components::active_processing_chain::types::EQState {
                        low_db: 2.5, mid_db: -1.0, presence_db: 0.0, air_db: 1.5,
                        curve_points: vec![(0.0, 0.5), (0.1, 0.4), (0.5, 0.6), (0.8, 0.5), (1.0, 0.3)],
                    },
                    compressor: crate::components::active_processing_chain::types::CompressorState {
                        threshold_db: -18.0, ratio: 4.0, gain_reduction_db: -3.2, makeup_db: 2.0,
                        curve_points: vec![(0.0, 1.0), (0.5, 0.5), (1.0, 0.2)],
                    },
                    limiter: crate::components::active_processing_chain::types::LimiterState {
                        ceiling_dbtp: -1.0, release_auto: true, isp_factor: 4,
                        curve_points: vec![(0.0, 1.0), (0.5, 0.3), (1.0, 0.2)],
                    }
                }
            }
        }
    }
}
