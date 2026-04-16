//! ipc.rs — Tauri IPC bridge for Dioxus web. P11-003
//! Authority: Phase 11 task-decomposition P11-003
//!
//! Calls window.__TAURI_INTERNALS__.invoke() via dioxus_document::eval().
//! All command names are the exact Rust snake_case registered name.
//! All JSON arg keys are the exact Rust parameter name (snake_case).
//!
//! Console logging is active for diagnostics — grep "IPC" in DevTools.

use dioxus_document::eval;
use serde::{de::DeserializeOwned, Serialize};

/// Log to the browser/WebView console.
#[allow(unused_macros)]
macro_rules! clog {
    ($($arg:tt)*) => {{
        web_sys::console::log_1(&::wasm_bindgen::JsValue::from_str(
            &format!($($arg)*)
        ));
    }};
}

/// Invoke a Tauri command from Dioxus web via JavaScript eval.
///
/// Uses dioxus_document::eval() → Eval::join::<R>() to run JS inside
/// the Tauri WebView and deserialize the return value.
///
/// Tauri v2: command names are exact Rust snake_case, arg keys are
/// exact Rust parameter names (also snake_case).
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned + 'static,
    A: Serialize,
{
    let args_json = serde_json::to_string(&args)
        .map_err(|e| format!("IPC serialize args failed: {e}"))?;

    clog!("[IPC] invoke: {} args={}", command, args_json);

    // Check that the Tauri IPC bridge is present
    let check_script = r#"
        if (typeof window.__TAURI_INTERNALS__ === 'undefined') {
            return "TAURI_INTERNALS_MISSING";
        }
        return "TAURI_INTERNALS_OK";
    "#;

    let bridge_check = eval(check_script)
        .join::<String>()
        .await
        .unwrap_or_else(|e| format!("check_failed: {e:?}"));

    clog!("[IPC] TAURI_INTERNALS check: {}", bridge_check);

    if bridge_check != "TAURI_INTERNALS_OK" {
        let err = format!(
            "Tauri IPC bridge not available ({bridge_check}). \
             Ensure withGlobalTauri: true in tauri.conf.json."
        );
        clog!("[IPC] ERROR: {}", err);
        return Err(err);
    }

    // Invoke the actual command
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

    let result = eval(&script)
        .join::<R>()
        .await
        .map_err(|e| {
            let msg = format!("IPC failed [{command}]: {e:?}");
            clog!("[IPC] ERROR: {}", msg);
            msg
        });

    match &result {
        Ok(_)    => clog!("[IPC] {} ✅ ok", command),
        Err(e)   => clog!("[IPC] {} ❌ {}", command, e),
    }

    result
}

/// Convenience: invoke with no arguments.
pub async fn invoke_no_args<R>(command: &str) -> Result<R, String>
where
    R: DeserializeOwned + 'static,
{
    invoke(command, serde_json::json!({})).await
}
