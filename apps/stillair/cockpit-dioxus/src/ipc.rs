//! ipc.rs — Tauri IPC bridge for Dioxus web. P11-003
//! Authority: Phase 11 task-decomposition P11-003
//!
//! Uses web_sys::window() + js_sys::Reflect to call
//! window.__TAURI_INTERNALS__.invoke() as a JsFuture.
//!
//! This avoids the deprecated JsStatic binding pattern and the
//! eval() timeout issue. Resolution is driven entirely by the
//! Tauri IPC Promise — no wrapper overhead, no eval timeout race.
//!
//! Command names: exact Rust snake_case (Tauri v2 convention).
//! Arg keys: exact Rust parameter names (snake_case).

use js_sys::{Function, Promise, Reflect};
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

// ── Console logging ───────────────────────────────────────────────────────────

#[allow(unused_macros)]
macro_rules! clog {
    ($($arg:tt)*) => {{
        web_sys::console::log_1(&JsValue::from_str(&format!($($arg)*)));
    }};
}

// ── IPC entry point ───────────────────────────────────────────────────────────

/// Invoke a Tauri command via window.__TAURI_INTERNALS__.invoke().
///
/// Uses web_sys::window() → Reflect::get("__TAURI_INTERNALS__") → invoke().
/// The returned Promise is driven by JsFuture, which resolves as soon as the
/// Tauri command returns — no eval wrapper timeout, no poll_join race.
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    clog!("[IPC] invoke: {}", command);

    // 1. Serialize args to a JS object via JSON
    let args_json = serde_json::to_string(&args)
        .map_err(|e| format!("[IPC] serialize failed [{command}]: {e}"))?;
    let args_js: JsValue = js_sys::JSON::parse(&args_json)
        .map_err(|e| format!("[IPC] JSON.parse failed [{command}]: {e:?}"))?;

    // 2. Get window
    let window = web_sys::window()
        .ok_or_else(|| "[IPC] no window — are we in a WASM context?".to_string())?;
    let window_val: JsValue = window.into();

    // 3. Get window.__TAURI_INTERNALS__
    let internals = Reflect::get(&window_val, &JsValue::from_str("__TAURI_INTERNALS__"))
        .map_err(|e| format!("[IPC] Reflect::get __TAURI_INTERNALS__ failed: {e:?}"))?;
    if internals.is_undefined() || internals.is_null() {
        return Err("[IPC] window.__TAURI_INTERNALS__ is undefined. \
                    Check tauri.conf.json withGlobalTauri:true".to_string());
    }
    clog!("[IPC] __TAURI_INTERNALS__ found ✅");

    // 4. Get the invoke function
    let invoke_val = Reflect::get(&internals, &JsValue::from_str("invoke"))
        .map_err(|e| format!("[IPC] Reflect::get invoke failed: {e:?}"))?;
    let invoke_fn: Function = invoke_val.dyn_into()
        .map_err(|_| "[IPC] __TAURI_INTERNALS__.invoke is not a function".to_string())?;

    // 5. Call invoke(command, args) → Promise
    //    `this` = internals object (preserves Tauri's internal this binding)
    let promise_val = invoke_fn
        .call2(&internals, &JsValue::from_str(command), &args_js)
        .map_err(|e| format!("[IPC] invoke({command}) call failed: {e:?}"))?;

    let promise: Promise = promise_val.dyn_into()
        .map_err(|e| format!("[IPC] invoke({command}) did not return Promise: {e:?}"))?;

    // 6. Await the Promise via JsFuture
    //    Resolves when Tauri command returns — respects any duration.
    let result_js = JsFuture::from(promise)
        .await
        .map_err(|e| {
            // Tauri surfaces command errors as rejected Promises.
            // Try to extract a string message from the rejection value.
            let msg = e.as_string()
                .or_else(|| {
                    Reflect::get(&e, &JsValue::from_str("message"))
                        .ok()
                        .and_then(|v| v.as_string())
                })
                .unwrap_or_else(|| {
                    js_sys::JSON::stringify(&e)
                        .ok()
                        .and_then(|s| s.as_string())
                        .unwrap_or_else(|| format!("{e:?}"))
                });
            let full = format!("[IPC] ❌ {command}: {msg}");
            clog!("{}", full);
            full
        })?;

    // 7. Deserialize result (Tauri sends JSON-serializable JS values)
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
    invoke(command, serde_json::json!({})).await
}
