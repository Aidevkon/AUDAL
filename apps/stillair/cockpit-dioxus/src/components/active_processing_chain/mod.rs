pub mod eq;
pub mod compressor;
pub mod limiter;
pub mod readout;
pub mod types;

use dioxus::prelude::*;
use eq::EQModule;
use compressor::CompressorModule;
use limiter::LimiterModule;
use types::{EQState, CompressorState, LimiterState};

#[derive(Props, Clone, PartialEq)]
pub struct ActiveProcessingChainProps {
    pub eq: EQState,
    pub compressor: CompressorState,
    pub limiter: LimiterState,
}

#[component]
pub fn ActiveProcessingChain(props: ActiveProcessingChainProps) -> Element {
    rsx! {
        div { class: "active-processing-chain",
            EQModule { state: props.eq.clone() }
            CompressorModule { state: props.compressor.clone() }
            LimiterModule { state: props.limiter.clone() }
        }
    }
}
