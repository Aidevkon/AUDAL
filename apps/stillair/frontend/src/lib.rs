//! Still Air — Leptos WASM frontend entry point.
//! Mounts the App component to the document body.

mod app;
mod cockpit;
mod components;
mod state;
mod types;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    // Leptos CSR: mount to document body
    leptos::mount::mount_to_body(app::App);
}
