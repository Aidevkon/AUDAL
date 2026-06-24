use crate::panels::insights::InsightsPanel;
use crate::panels::session::SessionPanel;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{PlaybackStateJson, RealtimeFrameJson, SessionStateJson, VisualizationDataJson};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Props, Clone, PartialEq)]
pub struct SamplingSiameseProps {
    pub mode: Signal<CockpitMode>,
    pub session_state: Signal<Option<SessionStateJson>>,
    pub playback_state: Signal<Option<PlaybackStateJson>>,
    pub viz_data: Signal<Option<VisualizationDataJson>>,
    pub show_mastered: Signal<bool>,
    pub wizard_findings: Signal<Vec<crate::wizard::WizardFinding>>,
    pub tone_angle: Signal<f32>,
    pub dyn_angle: Signal<f32>,
    pub space_angle: Signal<f32>,
    pub loud_angle: Signal<f32>,
    pub on_down_tone: EventHandler<MouseEvent>,
    pub on_down_dyn: EventHandler<MouseEvent>,
    pub on_down_space: EventHandler<MouseEvent>,
    pub on_down_loud: EventHandler<MouseEvent>,
    pub jini_persona: Signal<crate::types::JiniPersonaState>,
    pub is_journey_active: Signal<bool>,
    pub journey_stage: Signal<String>,
    pub journey_elapsed_ms: Signal<u64>,
}

#[component]
pub fn SamplingSiamese(mut props: SamplingSiameseProps) -> Element {
    // ── Telemetry lift: single polling source for all child panels ────────
    // Previously owned by InsightsPanel; moved here so the signal can be
    // shared with future consumers (e.g. Master Strip column) without
    // spawning redundant polling loops per child.
    let mut realtime: Signal<Option<RealtimeFrameJson>> = use_signal(|| None);
    let mut tele_generation = use_signal(|| 0u64);

    use_effect(move || {
        let my_gen = *tele_generation.peek() + 1;
        tele_generation.set(my_gen);
        spawn_local(async move {
            loop {
                if *tele_generation.peek() != my_gen {
                    break;
                }

                let m = props.mode.read().clone();
                // Poll whenever audio could be playing. CoachReady is the state
                // where the certified master plays — it MUST be included.
                // Idle: no file loaded. Exporting: I/O locked. Fault: broken.
                let is_active = !matches!(
                    m,
                    CockpitMode::Idle | CockpitMode::Exporting { .. } | CockpitMode::Fault { .. }
                );

                if is_active {
                    web_sys::console::log_1(&format!("[TELE-TRAP] polling, mode={:?}", m).into());
                    if let Ok(Some(frame)) =
                        crate::ipc::invoke_no_args::<Option<RealtimeFrameJson>>(
                            "get_live_telemetry_realtime",
                        )
                        .await
                    {
                        realtime.set(Some(frame));
                    }
                    gloo_timers::future::TimeoutFuture::new(200).await;
                } else {
                    gloo_timers::future::TimeoutFuture::new(200).await;
                }
            }
        });
    });

    let mfd1_active = props
        .wizard_findings
        .read()
        .iter()
        .any(|f| f.mfd == crate::wizard::MfdTarget::Mfd1SignalAnalyzer);

    let mfd2_active = props
        .wizard_findings
        .read()
        .iter()
        .any(|f| f.mfd == crate::wizard::MfdTarget::Mfd2SpatialTelemetry);

    rsx! {
        div {
            class: "sampling-siamese chassis-bezel chassis-substrate chassis-seam",
            // SESSION - LEFT (PSA)
            div {
                class: format!("siamese-col siamese-session{}",
                    if mfd1_active { " wizard-active" } else { "" }),
                SessionPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    viz_data: props.viz_data,
                    show_mastered: props.show_mastered,
                    wizard_findings: props.wizard_findings,
                    tone_angle: props.tone_angle,
                    dyn_angle: props.dyn_angle,
                    space_angle: props.space_angle,
                    loud_angle: props.loud_angle,
                    jini_persona: props.jini_persona,
                    journey_stage: props.journey_stage,
                }
                crate::components::hud_overlay::MfdHud {
                    findings: props.wizard_findings.read().clone(),
                    target: crate::wizard::MfdTarget::Mfd1SignalAnalyzer,
                    on_dismiss: move |id| {
                        props.wizard_findings.write().retain(|f| f.id != id);
                    },
                }
            }

            // INSIGHTS - CENTER (Spectral Dynamics)
            div {
                class: format!("siamese-col siamese-insights{}",
                    if mfd2_active { " wizard-active" } else { "" }),
                InsightsPanel {
                    mode: props.mode,
                    session_state: props.session_state,
                    playback_state: props.playback_state,
                    viz_data: props.viz_data,
                    telemetry: realtime,
                    tone_angle: props.tone_angle,
                    dyn_angle: props.dyn_angle,
                    space_angle: props.space_angle,
                    loud_angle: props.loud_angle,
                    on_down_tone: props.on_down_tone,
                    on_down_dyn: props.on_down_dyn,
                    on_down_space: props.on_down_space,
                    on_down_loud: props.on_down_loud,
                }
                crate::components::hud_overlay::MfdHud {
                    findings: props.wizard_findings.read().clone(),
                    target: crate::wizard::MfdTarget::Mfd2SpatialTelemetry,
                    on_dismiss: move |id| {
                        props.wizard_findings.write().retain(|f| f.id != id);
                    },
                }
            }

            // MASTERING CHAIN - RIGHT (25%)
            div { class: "siamese-col siamese-dsp",
                div { class: "mfd-panel",
                    div { class: "panel-title", "DSP CHAIN" }
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
                        },
                        sat: crate::components::active_processing_chain::SatTelemetry {
                            thd_percent: 1.8,
                            knee_curve: {
                                let mut k = [0.0f32; 64];
                                for (i, val) in k.iter_mut().enumerate() { *val = (i as f32 / 63.0).powf(0.72); }
                                k
                            },
                            headroom_db: 3.4,
                            harmonic_density: 0.35,
                        }
                    }
                }
            }
        }
    }
}
