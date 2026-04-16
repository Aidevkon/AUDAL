//! main.rs — Still Air Cockpit entry point (Dioxus web/WASM).
//! Phase 11 — Dioxus 0.6, served to Tauri WebView via trunk.
//!
//! Tauri injects window.__TAURI_INTERNALS__ (withGlobalTauri: true).
//! ipc.rs uses dioxus_document::eval() to call it.
//! Amendment A-002 §2: UI surface only — no core logic here.

fn main() {
    dioxus::launch(stillair_cockpit::app::App);
}
