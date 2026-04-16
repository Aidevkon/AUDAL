//! ipc.rs — Tauri IPC bridge for Dioxus desktop. P11-003
//! Authority: Phase 11 task-decomposition P11-003
//!
//! Dioxus 0.6 desktop apps run inside a Tauri WebView.
//! `window.__TAURI_INTERNALS__` is injected by Tauri with `withGlobalTauri: true`.
//!
//! The same Tauri commands (triggerMastering, getSessionState, exportAudio,
//! openAudioFile) work identically from Dioxus as from the Leptos frontend.
//!
//! API (dioxus-document 0.6.3):
//!   eval(script: &str) -> Eval        — free function from dioxus::prelude::*
//!   Eval::join::<T>() -> Result<T, _> — typed async return via IntoFuture
//!
//! FORBIDDEN:
//!   ❌ Calling M0 directly (all calls go through Tauri commands)
//!   ❌ Business logic in IPC layer (IPC is pure transport)
//!   ❌ Core crate imports

use dioxus_document::eval;
use serde::{de::DeserializeOwned, Serialize};

/// Invoke a Tauri command from Dioxus desktop via JavaScript eval.
///
/// Uses dioxus_document::eval() to execute JS in the embedded WebView,
/// calling window.__TAURI_INTERNALS__.invoke — same as the Leptos frontend.
///
/// Eval::join::<R>() awaits the JS return and deserializes into R.
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned + 'static,
    A: Serialize,
{
    let args_json = serde_json::to_string(&args)
        .map_err(|e| format!("IPC serialize args failed: {e}"))?;

    let script = format!(
        r#"
        try {{
            const result = await window.__TAURI_INTERNALS__.invoke(
                "{command}", {args_json}
            );
            return result;
        }} catch (e) {{
            throw new Error(e.message || String(e));
        }}
        "#
    );

    // eval() returns an Eval handle; join::<R>() drives the JS to completion
    // and deserializes the return value. The explicit turbofish is required
    // because R itself carries the type information.
    eval(&script)
        .join::<R>()
        .await
        .map_err(|e| format!("IPC failed [{command}]: {e:?}"))
}

/// Convenience: invoke with no arguments.
pub async fn invoke_no_args<R>(command: &str) -> Result<R, String>
where
    R: DeserializeOwned + 'static,
{
    invoke(command, serde_json::json!({})).await
}
