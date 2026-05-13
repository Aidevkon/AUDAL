pub mod eq;
pub mod compressor;
pub mod limiter;
pub mod sat;
pub mod readout;
pub mod types;

use dioxus::prelude::*;
use eq::EQModule;
use compressor::CompressorModule;
use limiter::LimiterModule;
use sat::SatModule;
use types::{EQState, CompressorState, LimiterState};
pub use types::SatTelemetry;

#[derive(Props, Clone, PartialEq)]
pub struct ActiveProcessingChainProps {
    pub eq: EQState,
    pub compressor: CompressorState,
    pub limiter: LimiterState,
    pub sat: SatTelemetry,
}

#[component]
pub fn ActiveProcessingChain(props: ActiveProcessingChainProps) -> Element {
    rsx! {
        div { class: "dsp-screen",
            EQModule { state: props.eq.clone() }
            CompressorModule { state: props.compressor.clone() }
            LimiterModule { state: props.limiter.clone() }
            SatModule { state: props.sat.clone() }
        }
    }
}
