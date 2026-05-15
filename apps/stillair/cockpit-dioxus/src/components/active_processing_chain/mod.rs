pub mod eq;
pub mod compressor;
pub mod limiter;
pub mod sat;
pub mod readout;
pub mod types;

use dioxus::prelude::*;
use eq::{EQDisplay};
use sat::SatDisplay;
use compressor::CompDisplay;
use limiter::LimDisplay;
use types::{EQState, CompressorState, LimiterState};
pub use types::SatTelemetry;

#[derive(Props, Clone, PartialEq)]
pub struct ActiveProcessingChainProps {
    pub eq: EQState,
    pub compressor: CompressorState,
    pub limiter: LimiterState,
    pub sat: SatTelemetry,
}

/// DSP Chain — 3-cell layout mirroring InsightsPanel (SPECTRAL DYNAMICS):
///
///  ┌────────────────────────────────┐
///  │  EQ SPECTRUM  (top, ~55%)      │
///  ├─────────────────┬──────────────┤
///  │  SAT SCOPE      │  COMP + LIM  │
///  └─────────────────┴──────────────┘
#[component]
pub fn ActiveProcessingChain(props: ActiveProcessingChainProps) -> Element {
    rsx! {
        div { class: "dsp-screen",
            div { class: "dsp-body",

                // ── TOP: EQ spectrum (full width, 55%) ───────────────────
                div { class: "dsp-eq-cell oled-screen",
                    EQDisplay { state: props.eq.clone() }
                }

                // ── BOTTOM ROW ────────────────────────────────────────────
                div { class: "dsp-bottom-row",

                    // Bottom-left: SAT knee scope
                    div { class: "dsp-sat-cell oled-screen",
                        SatDisplay { state: props.sat.clone() }
                    }

                    // Bottom-right: Comp + Lim readouts
                    div { class: "dsp-chain-cell oled-screen",
                        div { class: "dsp-chain-panel",
                            CompDisplay { state: props.compressor.clone() }
                            div { class: "dsp-chain-divider" }
                            LimDisplay { state: props.limiter.clone() }
                        }
                    }
                }
            }
        }
    }
}
