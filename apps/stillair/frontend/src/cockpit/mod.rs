//! Cockpit — 3-panel MFD shell.
//! Authority: Phase 5 task-decomposition P5-003 · Phase 6 P6-005/P6-006/P6-007

pub mod coach_panel;
pub mod insights_panel;
pub mod session_panel;

use leptos::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{AudioMeta, CoachFindings, Metrics};
use session_panel::SessionPanel;
use insights_panel::InsightsPanel;
use coach_panel::CoachPanel;

#[component]
pub fn Cockpit(
    mode:             ReadSignal<CockpitMode>,
    audio_meta:       ReadSignal<Option<AudioMeta>>,
    selected_preset:  ReadSignal<Option<&'static str>>,
    on_preset_select: UnsyncCallback<&'static str>,
    on_file_drop:     UnsyncCallback<String>,
    metrics:          ReadSignal<Option<Metrics>>,
    findings:         ReadSignal<Option<CoachFindings>>,
    // FM2 live telemetry — P6-007
    live_lufs:        ReadSignal<Option<f32>>,
    live_peak:        ReadSignal<Option<f32>>,
    progress:         ReadSignal<f32>,
    stage_name:       ReadSignal<String>,
) -> impl IntoView {
    view! {
        <div class="cockpit">
            <SessionPanel
                mode=mode
                audio_meta=audio_meta
                selected_preset=selected_preset
                on_preset_select=on_preset_select
                on_file_drop=on_file_drop
            />
            <InsightsPanel
                mode=mode
                metrics=metrics
                live_lufs=live_lufs
                live_peak=live_peak
                progress=progress
                stage_name=stage_name
            />
            <CoachPanel
                mode=mode
                findings=findings
            />
        </div>
    }
}
