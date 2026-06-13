//! Still Air — Tauri 2.x backend entry point.
//! Authority: Creator OS Constitution v2.6 · state-machine.md §6.3
//!
//! Phase 12A commands (A-003 §8):
//!   playback::playback_control  — play | pause | stop | seek → xaak → cpal
//!   playback::get_playback_state — current position, duration, is_playing
//! Phase 13:
//!   report::export_pdf_report — BMR-128 PDF compliance report (printpdf, MIT)
//! Phase 14:
//!   visualization::get_visualization_data — SVG paths + ellipse params (libm)

// Prevent a console window from popping up on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod telemetry_listener;
use tauri::Manager;

pub mod coach_narrative;
pub mod commands;
pub mod ipc; // Phase 8: Aether Coach — LLM narrative layer

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(ipc::m0_client::M0Client::new());
            
            let latest_arc = std::sync::Arc::new(std::sync::Mutex::new(None));
            app.manage(crate::telemetry_listener::LatestFrame(latest_arc.clone()));
            telemetry_listener::spawn_udp_listener(latest_arc);
            
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            commands::mastering::open_audio_file,
            commands::mastering::trigger_mastering,
            commands::mastering::load_audio_file,
            commands::mastering::get_golden_blob,
            commands::mastering::export_certificate_png,
            commands::insights::evaluate_findings,
            commands::export::export_audio,
            commands::coach::get_coach_narrative, // Phase 8: Aether Coach
            commands::session::get_session_state, // P9-008: Session State Unification
            // Phase 12A/12B: xaak playback (A-003 §8)
            commands::playback::playback_control,
            commands::playback::get_playback_state,
            commands::playback::get_live_telemetry, // P12B-005: live LUFS
            // Phase 13B: BMR-128 PDF report
            commands::report::export_pdf_report,
            commands::report::preview_pdf_report,
            // Phase 14: precomputed SVG paths (UI Agent Context v2.1 §2)
            commands::visualization::get_visualization_data,
            commands::telemetry::get_live_telemetry_realtime,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Still Air");
}
