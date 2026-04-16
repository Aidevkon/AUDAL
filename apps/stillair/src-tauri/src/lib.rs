//! Still Air — Tauri 2.x backend entry point.
//! Authority: Creator OS Constitution v2.6 · state-machine.md §6.3
//!
//! Phase 6 commands:
//!   mastering::trigger_mastering — POST /master → M0 → sp314-dsp
//!   mastering::load_audio_file   — stub (Phase 7: real M0 decode)
//!   mastering::get_golden_blob   — GET /blob/{id} → M0
//!   insights::evaluate_findings  — rule-engine evaluation (in-process)
//!   export::export_audio         — POST /export → M0
//! Phase 8: coach::get_coach_narrative — Aether Coach LLM narrative
//! P9-008:  session::get_session_state — unified snapshot (Phase 11 Dioxus entry point)

// Prevent a console window from popping up on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;
pub mod ipc;
pub mod aether;   // Phase 8: Aether Coach — LLM narrative layer

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::mastering::open_audio_file,
            commands::mastering::trigger_mastering,
            commands::mastering::load_audio_file,
            commands::mastering::get_golden_blob,
            commands::insights::evaluate_findings,
            commands::export::export_audio,
            commands::coach::get_coach_narrative,    // Phase 8: Aether Coach
            commands::session::get_session_state,    // P9-008: Session State Unification
        ])
        .run(tauri::generate_context!())
        .expect("error while running Still Air");
}
