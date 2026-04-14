//! FaultDisplay — FM-ERR modal overlay.
//! Authority: Phase 5 task-decomposition P5-010 · state-machine.md §4.3
//!
//! Shows ASC code + human message + MASTER RESET → FM0 button.
//! FORBIDDEN: FM-ERR → FM1 shortcut (master_reset() goes to FM0 only).

use leptos::callback::Callable;
use leptos::prelude::*;
use crate::state::cockpit_mode::AscCode;

#[component]
pub fn FaultDisplay(code: AscCode, on_reset: UnsyncCallback<()>) -> impl IntoView {
    let (code_str, msg) = match &code {
        AscCode::MathErr        => ("ASC 0x01", "DSP arithmetic error — NaN or Inf detected in pipeline output"),
        AscCode::IoErr          => ("ASC 0x02", "I/O failure — file read failure or Golden Blob write failure"),
        AscCode::Aborted        => ("ASC 0x03", "Mastering aborted — user-initiated emergency stop"),
        AscCode::ValidationFail => ("ASC 0x04", "Audio validation failed — input violates sanitization rules"),
        AscCode::WasmPanic      => ("ASC 0x05", "LineOS runtime crash — WASM panic detected"),
    };

    view! {
        <div class="fault-overlay">
            <div class="fault-display">
                <div class="fault-code">"FM-ERR · "{code_str}</div>
                <div class="fault-message">{msg}</div>
                <button
                    id="btn-master-reset"
                    class="btn-reset"
                    on:click=move |_| on_reset.run(())
                >
                    "MASTER RESET → FM0"
                </button>
            </div>
        </div>
    }
}
