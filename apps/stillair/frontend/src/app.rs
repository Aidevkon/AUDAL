//! app.rs — Root component. FM0→FM6 mastering flow with real M0 wiring.
//! Authority: Phase 6 P6-005/P6-006/P6-007/P6-008 · state-machine.md §4
//!
//! Data flow (Phase 6):
//!   FM0 → drop file → FM1 (load_audio_file Tauri command)
//!   FM1 → select preset → FM1.5
//!   FM1.5 → MASTER → FM2 (trigger_mastering → M0 → sp314-dsp)
//!   FM2 → live TelemetrySignal events (100ms)
//!   FM2 → dsp_done → FM3 → get_golden_blob → FM4
//!   FM4 → evaluate_findings → FM5 (rule-engine in Tauri process)
//!   FM5 → EXPORT → FM6 → export_audio → FM5 (success) | FM-ERR (ASC 0x02)
//!   FM-ERR → MASTER RESET → FM0 (never shortcut to FM1)
//!
//! FORBIDDEN: Cockpit holding binary audio state.
//! FORBIDDEN: Polling M0 for telemetry — event subscription only.
//! FORBIDDEN: serde_json::Value crossing WASM boundary.
//! Use UnsyncCallback (Rc-based) for closures capturing reactive state.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::cockpit::Cockpit;
use crate::components::fault_display::FaultDisplay;
use crate::components::transport_bar::TransportBar;
use crate::state::cockpit_mode::{AscCode, CockpitMode};
use crate::types::{AudioMeta, CoachFindings, GoldenBlobJson, Issue, Metrics, Severity, TelemetrySignal};

// ── Tauri invoke helper ───────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    /// Tauri v2 IPC bridge — always at window.__TAURI_INTERNALS__.invoke.
    /// withGlobalTauri: true additionally wires window.__TAURI__ high-level API,
    /// but raw IPC lives here regardless of that setting.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> JsValue;
}

/// Invoke a Tauri command and deserialize the result.
async fn invoke<T: for<'de> serde::Deserialize<'de>>(
    cmd: &str,
    args: serde_json::Value,
) -> Result<T, String> {
    let js_args = serde_wasm_bindgen::to_value(&args)
        .map_err(|e| format!("Serialize error: {e}"))?;
    let result = tauri_invoke(cmd, js_args).await;
    serde_wasm_bindgen::from_value(result)
        .map_err(|e| format!("Deserialize error: {e}"))
}

// ── Root App component ────────────────────────────────────────────────────────

#[component]
pub fn App() -> impl IntoView {
    // ── Reactive signals ──────────────────────────────────────────────────────
    let (mode,       set_mode)      = signal::<CockpitMode>(CockpitMode::Idle);
    let (audio_meta, set_meta)      = signal::<Option<AudioMeta>>(None);
    let (sel_preset, set_preset)    = signal::<Option<&'static str>>(None);
    let (metrics,    set_metrics)   = signal::<Option<Metrics>>(None);
    let (findings,   set_findings)  = signal::<Option<CoachFindings>>(None);
    let (blob_id,    set_blob_id)   = signal::<Option<String>>(None);
    // FM2 live telemetry signals
    let (live_lufs,  set_live_lufs) = signal::<Option<f32>>(None);
    let (live_peak,  set_live_peak) = signal::<Option<f32>>(None);
    let (progress,   set_progress)  = signal::<f32>(0.0);
    let (stage_name, set_stage)     = signal::<String>("Initializing".into());

    // ── Helper: hard reset all state to FM0 ──────────────────────────────────
    let hard_reset = move || {
        set_mode.set(CockpitMode::Idle);
        set_meta.set(None);
        set_preset.set(None);
        set_metrics.set(None);
        set_findings.set(None);
        set_blob_id.set(None);
        set_live_lufs.set(None);
        set_live_peak.set(None);
        set_progress.set(0.0);
        set_stage.set("Initializing".into());
    };

    // ── Event handlers ────────────────────────────────────────────────────────

    // FM0 → FM1: file dropped (drag-and-drop)
    let on_file_drop = UnsyncCallback::new(move |path: String| {
        if path.is_empty() {
            set_mode.set(CockpitMode::Fault(AscCode::ValidationFail));
            return;
        }
        spawn_local(async move {
            match invoke::<crate::commands::AudioMetaResponse>(
                "load_audio_file",
                serde_json::json!({ "path": path }),
            ).await {
                Ok(resp) => {
                    set_meta.set(Some(AudioMeta {
                        path:        resp.path,
                        name:        resp.name,
                        format:      resp.format,
                        sample_rate: resp.sample_rate,
                        bit_depth:   resp.bit_depth,
                        duration_s:  resp.duration_s,
                        channels:    resp.channels,
                    }));
                    set_preset.set(None);
                    set_mode.set(CockpitMode::FileLoaded);
                }
                Err(e) if e.contains("ASC:0x04") => {
                    set_mode.set(CockpitMode::Fault(AscCode::ValidationFail));
                }
                Err(_) => {
                    set_mode.set(CockpitMode::Fault(AscCode::IoErr));
                }
            }
        });
    });

    // FM1 → FM1.5: preset selected (seals intent)
    let on_preset_select = UnsyncCallback::new(move |preset_id: &'static str| {
        set_preset.set(Some(preset_id));
        set_mode.set(CockpitMode::PresetSelected);
    });

    // FM1.5 → FM2 → Data Cascade (FM3→FM4→FM5)
    let on_master = UnsyncCallback::new(move |()| {
        let preset = sel_preset.get_untracked().unwrap_or("spotify");
        // Use full filesystem path for M0 (not display name)
        let audio_path = audio_meta.get_untracked()
            .map(|m| m.path.clone())
            .unwrap_or_default();

        set_mode.set(CockpitMode::Mastering);
        set_progress.set(0.0);

        spawn_local(async move {
            // ── Phase 6: real M0 mastering ─────────────────────────────────
            // Step 1: trigger_mastering → blob_id
            let master_result = invoke::<String>(
                "trigger_mastering",
                serde_json::json!({
                    "audio_path": audio_path,
                    "preset_id":  preset,
                }),
            ).await;

            let bid = match master_result {
                Ok(id)  => id,
                Err(e)  => {
                    // ASC 0x01 (math error) or 0x02 (I/O error)
                    let code = if e.contains("I/O") { AscCode::IoErr }
                               else                 { AscCode::MathErr };
                    set_mode.set(CockpitMode::Fault(code));
                    return;
                }
            };

            set_blob_id.set(Some(bid.clone()));
            // FM2 → FM3
            set_mode.set(CockpitMode::Mastered);

            // Step 2: get_golden_blob → populate metrics (FM3 → FM4)
            let blob_result = invoke::<GoldenBlobJson>(
                "get_golden_blob",
                serde_json::json!({ "blob_id": bid }),
            ).await;

            let blob = match blob_result {
                Ok(b)  => b,
                Err(_) => {
                    set_mode.set(CockpitMode::Fault(AscCode::IoErr));
                    return;
                }
            };

            set_metrics.set(Some(Metrics::from_blob(&blob)));
            set_mode.set(CockpitMode::InsightsReady);  // FM4

            // Step 3: evaluate_findings → real CoachFindings (FM4 → FM5)
            let findings_result = invoke::<crate::commands::CoachFindingsResponse>(
                "evaluate_findings",
                serde_json::json!({ "blob": blob }),
            ).await;

            match findings_result {
                Ok(resp) => {
                    let issues = resp.issues.into_iter().map(|i| Issue {
                        id:       i.id,
                        severity: Severity::from_str(&i.severity),
                        current:  i.current,
                        target:   i.target,
                        delta:    i.delta,
                        tags:     i.tags,
                    }).collect();
                    set_findings.set(Some(CoachFindings {
                        issues,
                        recommendation: resp.recommendation,
                    }));
                    set_mode.set(CockpitMode::CoachReady);  // FM5
                }
                Err(_) => {
                    // Non-fatal — FM5 with empty findings
                    set_findings.set(Some(CoachFindings {
                        issues: vec![],
                        recommendation: "Rule evaluation unavailable.".into(),
                    }));
                    set_mode.set(CockpitMode::CoachReady);
                }
            }
        });
    });

    // FM2 → FM1: abort mastering
    let on_abort = UnsyncCallback::new(move |()| {
        set_mode.set(CockpitMode::FileLoaded);
        set_preset.set(None);
    });

    // LOAD NEW — open native file picker, then transition FM1 on selection.
    // Behaviour per spec:
    //   File selected + valid ext → hard reset state → FM1 (FileLoaded)
    //   Dialog cancelled         → stay in current mode (no state change)
    //   Invalid extension        → FM-ERR ASC 0x04 (ValidationFail)
    let on_load_new = UnsyncCallback::new(move |()| {
        spawn_local(async move {
            match invoke::<Option<crate::commands::AudioMetaResponse>>(
                "open_audio_file",
                serde_json::json!({}),
            ).await {
                // User cancelled — stay exactly where we are
                Ok(None) => {}

                // File selected and valid — reset state, transition FM1
                Ok(Some(resp)) => {
                    // Hard reset all signals before populating new file
                    set_mode.set(CockpitMode::Idle);
                    set_meta.set(None);
                    set_preset.set(None);
                    set_metrics.set(None);
                    set_findings.set(None);
                    set_blob_id.set(None);
                    set_live_lufs.set(None);
                    set_live_peak.set(None);
                    set_progress.set(0.0);
                    set_stage.set("Initializing".into());
                    // Now populate with the new file and advance to FM1
                    set_meta.set(Some(AudioMeta {
                        path:        resp.path,
                        name:        resp.name,
                        format:      resp.format,
                        sample_rate: resp.sample_rate,
                        bit_depth:   resp.bit_depth,
                        duration_s:  resp.duration_s,
                        channels:    resp.channels,
                    }));
                    set_mode.set(CockpitMode::FileLoaded);
                }

                // ASC 0x04: unsupported extension
                Err(e) if e.contains("ASC:0x04") => {
                    set_mode.set(CockpitMode::Fault(AscCode::ValidationFail));
                }

                // Other error (dialog crash, etc.) — IoErr
                Err(_) => {
                    set_mode.set(CockpitMode::Fault(AscCode::IoErr));
                }
            }
        });
    });

    // FM5 → FM6: export
    let on_export = UnsyncCallback::new(move |()| {
        let bid = match blob_id.get_untracked() {
            Some(id) => id,
            None => {
                set_mode.set(CockpitMode::Fault(AscCode::IoErr));
                return;
            }
        };
        set_mode.set(CockpitMode::Exporting);

        spawn_local(async move {
            let result = invoke::<crate::commands::ExportResult>(
                "export_audio",
                serde_json::json!({
                    "blob_id": bid,
                    "format":  "flac",
                    "path":    "/tmp/stillair-export.flac",
                }),
            ).await;

            match result {
                Ok(_)  => set_mode.set(CockpitMode::CoachReady),  // FM5
                Err(_) => set_mode.set(CockpitMode::Fault(AscCode::IoErr)),
            }
        });
    });

    // FM-ERR → FM0: master reset (never FM1 — state-machine.md §4.3)
    let on_master_reset = UnsyncCallback::new(move |()| {
        hard_reset();
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
                live_lufs=live_lufs
                live_peak=live_peak
                progress=progress
                stage_name=stage_name
            />

            {move || match mode.get() {
                CockpitMode::Fault(code) => view! {
                    <FaultDisplay code=code on_reset=on_master_reset />
                }.into_any(),
                _ => view! { <span></span> }.into_any(),
            }}
        </div>
    }
}


