//! app.rs — Root component. FM0→FM5 core mastering flow wiring.
//! Authority: Phase 5 task-decomposition P5-009 · state-machine.md §4
//!
//! State ownership:
//!   - `mode` signal: derived from FSM, drives all MFD rendering
//!   - `audio_meta` signal: set when file loaded
//!   - `selected_preset` signal: set when preset chosen
//!   - `metrics` signal: stub set after dsp_done()
//!   - `findings` signal: stub CoachFindings set after coach_done()
//!
//! FORBIDDEN: No business logic in event handlers.
//! FORBIDDEN: No direct DSP calls from this layer.
//! FSM transitions are the only way mode changes.
//! Use UnsyncCallback (Rc-based) for closures capturing reactive state.

use leptos::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;

use crate::cockpit::Cockpit;
use crate::components::fault_display::FaultDisplay;
use crate::components::transport_bar::TransportBar;
use crate::state::cockpit_mode::{AscCode, CockpitMode};
use crate::types::{AudioMeta, CoachFindings, Issue, IssueParams, Metrics, Severity};

/// Stub CoachFindings wired per P5-009.
/// Phase 6: replaced with real rule-engine output from M0 IPC.
fn stub_findings(preset: &'static str) -> CoachFindings {
    CoachFindings {
        issues: vec![
            Issue {
                id:       "lufs_compliance".into(),
                severity: Severity::Medium,
                params:   IssueParams { current: -12.0, target: -14.0, delta: 2.0 },
                tags:     vec![format!("platform:{preset}")],
            },
        ],
        recommendation: "Reduce gain to meet loudness target.".into(),
    }
}

/// Stub AudioMeta — Phase 6 replaces with real Tauri IPC response.
fn stub_meta(path: &str) -> AudioMeta {
    AudioMeta {
        name:        path.split('/').last().unwrap_or("unknown").to_string(),
        format:      "WAV".to_string(),
        sample_rate: 48_000,
        bit_depth:   24,
        duration_s:  180.0,
        channels:    2,
    }
}

#[component]
pub fn App() -> impl IntoView {
    // ── Reactive signals ──────────────────────────────────────────────────────
    let (mode,        set_mode)       = signal::<CockpitMode>(CockpitMode::Idle);
    let (audio_meta,  set_audio_meta) = signal::<Option<AudioMeta>>(None);
    let (sel_preset,  set_preset)     = signal::<Option<&'static str>>(None);
    let (metrics,     set_metrics)    = signal::<Option<Metrics>>(None);
    let (findings,    set_findings)   = signal::<Option<CoachFindings>>(None);

    // ── Event handlers — all UnsyncCallback (Rc-based, no Send+Sync needed) ─

    let on_file_drop = UnsyncCallback::new(move |path: String| {
        if path.is_empty() {
            set_mode.set(CockpitMode::Fault(AscCode::ValidationFail));
            return;
        }
        // FM0 → FM1
        set_audio_meta.set(Some(stub_meta(&path)));
        set_preset.set(None);
        set_metrics.set(None);
        set_findings.set(None);
        set_mode.set(CockpitMode::FileLoaded);
    });

    let on_preset_select = UnsyncCallback::new(move |preset_id: &'static str| {
        // FM1 → FM1.5
        set_preset.set(Some(preset_id));
        set_mode.set(CockpitMode::PresetSelected);
    });

    let on_master = UnsyncCallback::new(move |()| {
        // FM1.5 → FM2: seal Intent, invoke async stub
        set_mode.set(CockpitMode::Mastering);
        let preset = sel_preset.get_untracked().unwrap_or("spotify");

        spawn_local(async move {
            // Phase 5: 2-second simulated DSP (Phase 6: real Tauri IPC call)
            TimeoutFuture::new(2_000).await;

            // FM2 → FM3 (dsp_done)
            set_mode.set(CockpitMode::Mastered);

            // FM3 → FM4 (automatic — analysis_done)
            TimeoutFuture::new(400).await;
            set_metrics.set(Some(Metrics::stub()));
            set_mode.set(CockpitMode::InsightsReady);

            // FM4 → FM5 (automatic — coach_done)
            TimeoutFuture::new(400).await;
            set_findings.set(Some(stub_findings(preset)));
            set_mode.set(CockpitMode::CoachReady);
        });
    });

    let on_abort = UnsyncCallback::new(move |()| {
        // FM2 → FM1 (abort)
        set_mode.set(CockpitMode::FileLoaded);
        set_preset.set(None);
    });

    let on_load_new = UnsyncCallback::new(move |()| {
        // Atomic reset FM3/4/5 → FM0 → FM1 (user sees FM1, FM0 is internal)
        // state-machine.md §4.2
        set_mode.set(CockpitMode::Idle);
        set_audio_meta.set(None);
        set_preset.set(None);
        set_metrics.set(None);
        set_findings.set(None);
    });

    let on_export = UnsyncCallback::new(move |()| {
        // FM5 → FM6 (Phase 5 placeholder)
        set_mode.set(CockpitMode::Exporting);
        spawn_local(async move {
            // Phase 5 placeholder: 1.5s, then back to FM5
            TimeoutFuture::new(1_500).await;
            set_mode.set(CockpitMode::CoachReady);
        });
    });

    let on_master_reset = UnsyncCallback::new(move |()| {
        // FM-ERR → FM0 ONLY — never shortcut to FM1
        // state-machine.md §4.3
        set_mode.set(CockpitMode::Idle);
        set_audio_meta.set(None);
        set_preset.set(None);
        set_metrics.set(None);
        set_findings.set(None);
    });

    // ── View ──────────────────────────────────────────────────────────────────
    view! {
        <div class="app-shell">
            <TransportBar
                mode=mode
                on_master=on_master
                on_abort=on_abort
                on_load_new=on_load_new
                on_export=on_export
            />

            <Cockpit
                mode=mode
                audio_meta=audio_meta
                selected_preset=sel_preset
                on_preset_select=on_preset_select
                on_file_drop=on_file_drop
                metrics=metrics
                findings=findings
            />

            // FM-ERR overlay — rendered on top of all panels
            {move || match mode.get() {
                CockpitMode::Fault(code) => view! {
                    <FaultDisplay code=code on_reset=on_master_reset />
                }.into_any(),
                _ => view! { <span></span> }.into_any(),
            }}
        </div>
    }
}
