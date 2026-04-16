//! ipc.rs — Tauri IPC bridge for Dioxus web. P11-003
//! Authority: Phase 11 task-decomposition P11-003
//!
//! Direct wasm-bindgen IPC: bypasses Dioxus eval() entirely.
//! Calls window.__TAURI_INTERNALS__.invoke() as a JsFuture from Rust.
//! This is the correct pattern for Tauri + Dioxus web:
//!
//!   1. Get window.__TAURI_INTERNALS__ as a JsValue
//!   2. Call .invoke(command, args) → Promise
//!   3. Await it as a JsFuture
//!   4. Deserialize the JSON result
//!
//! No eval() timeout issues. No intermediate eval wrapper overhead.
//! All command names: exact Rust snake_case (Tauri v2 convention).
//! All arg keys: exact Rust parameter names (snake_case).

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ── Console logging ───────────────────────────────────────────────────────────

#[allow(unused_macros)]
macro_rules! clog {
    ($($arg:tt)*) => {{
        web_sys::console::log_1(&::wasm_bindgen::JsValue::from_str(
            &format!($($arg)*)
        ));
    }};
}

// ── JS bindings ───────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    /// window object
    type Window;

    #[wasm_bindgen(js_name = window)]
    static WINDOW: Window;

    /// window.__TAURI_INTERNALS__
    #[wasm_bindgen(method, getter, js_name = __TAURI_INTERNALS__)]
    fn tauri_internals(this: &Window) -> JsValue;
}

/// Invoke a Tauri command directly via window.__TAURI_INTERNALS__.invoke().
///
/// Uses wasm_bindgen::JsFuture instead of dioxus eval() so there is no
/// eval wrapper timeout or communication overhead — the Promise resolves
/// as soon as the Tauri command returns, regardless of how long it takes.
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    let args_js = serde_wasm_bindgen(command, args)?;

    clog!("[IPC] invoke: {}", command);

    // Get window.__TAURI_INTERNALS__
    let internals = WINDOW.tauri_internals();
    if internals.is_undefined() || internals.is_null() {
        let err = "[IPC] ERROR: window.__TAURI_INTERNALS__ is undefined. \
                   Check withGlobalTauri:true in tauri.conf.json.".to_string();
        clog!("{}", err);
        return Err(err);
    }

    // Call window.__TAURI_INTERNALS__.invoke(command, args) → Promise
    let invoke_fn = js_sys::Reflect::get(&internals, &JsValue::from_str("invoke"))
        .map_err(|e| format!("[IPC] invoke fn not found: {e:?}"))?;
    let invoke_fn: js_sys::Function = invoke_fn.dyn_into()
        .map_err(|_| "[IPC] __TAURI_INTERNALS__.invoke is not a function".to_string())?;

    let internals_obj: js_sys::Object = js_sys::Object::try_from(&internals)
        .ok_or_else(|| "[IPC] __TAURI_INTERNALS__ is not an object".to_string())?.clone();

    let promise = invoke_fn
        .call2(&internals_obj, &JsValue::from_str(command), &args_js)
        .map_err(|e| format!("[IPC] invoke call failed [{command}]: {e:?}"))?;

    let promise: js_sys::Promise = promise.dyn_into()
        .map_err(|e| format!("[IPC] invoke did not return a Promise [{command}]: {e:?}"))?;

    // Await the Promise
    let result_js = JsFuture::from(promise)
        .await
        .map_err(|e| {
            // Tauri errors come back as strings or objects
            let msg = e.as_string()
                .or_else(|| {
                    js_sys::Reflect::get(&e, &JsValue::from_str("message"))
                        .ok()
                        .and_then(|v| v.as_string())
                })
                .unwrap_or_else(|| format!("{e:?}"));
            let full = format!("[IPC] ❌ {command}: {msg}");
            clog!("{}", full);
            full
        })?;

    // Serde: JsValue → R
    // Tauri serializes results as JSON-compatible JS values
    let result = serde_wasm_bindgen::from_value::<R>(result_js)
        .map_err(|e| format!("[IPC] deserialize failed [{command}]: {e:?}"))?;

    clog!("[IPC] ✅ {}", command);
    Ok(result)
}

/// Convenience: invoke with no arguments.
pub async fn invoke_no_args<R>(command: &str) -> Result<R, String>
where
    R: DeserializeOwned,
{
    // Empty args object — Tauri expects an object, not null
    invoke(command, &serde_json::json!({})).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Serialize args to a JS object via JSON.
fn serde_wasm_bindgen<A: Serialize>(command: &str, args: A) -> Result<JsValue, String> {
    let json_str = serde_json::to_string(&args)
        .map_err(|e| format!("[IPC] serialize args failed [{command}]: {e}"))?;
    js_sys::JSON::parse(&json_str)
        .map_err(|e| format!("[IPC] args JSON.parse failed [{command}]: {e:?}"))
}
