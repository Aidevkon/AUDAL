//! Still Air — Tauri 2.x backend entry point.
//! Authority: Creator OS Constitution v2.6 · state-machine.md §6.3
//!
//! Phase 12A commands (A-003 §8):
//!   playback::playback_control  — play | pause | stop | seek → xaak → cpal
//!   playback::get_playback_state — current position, duration, is_playing

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
            // Phase 12A: xaak playback (A-003 §8)
            commands::playback::playback_control,
            commands::playback::get_playback_state,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Still Air");
}
