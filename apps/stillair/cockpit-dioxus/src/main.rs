//! main.rs — Still Air Cockpit entry point.
//! Phase 11 — Dioxus 0.6 desktop.
//!
//! Launches the Dioxus desktop app with the Cockpit root component.
//! Amendment A-002 §2: UI surface only — no core logic here.

fn main() {
    dioxus::launch(stillair_cockpit::app::App);
}
