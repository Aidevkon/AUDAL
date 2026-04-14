//! Still Air — Tauri 2.x backend entry point.
//! Authority: Creator OS Constitution v2.6 · state-machine.md §6.3
//! Commands: mastering (stub), load_audio_file (stub)
//! Full M0 IPC wired in Phase 6.

// Prevent a console window from popping up on Windows (no-op on Linux/macOS)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod commands;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::mastering::trigger_mastering,
            commands::mastering::load_audio_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Still Air");
}
