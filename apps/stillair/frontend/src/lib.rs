//! Still Air — Leptos WASM frontend entry point.
//! Mounts the App component to the document body.

mod app;
mod cockpit;
mod commands;
mod components;
mod state;
mod types;


use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    // Install panic hook FIRST — converts WASM aborts into browser console
    // messages with a Rust backtrace. Must be called before any other code.
    // Authority: Phase 6 debugging invariant — silent aborts are FORBIDDEN.
    console_error_panic_hook::set_once();

    // Leptos CSR: mount to document body
    leptos::mount::mount_to_body(app::App);
}
