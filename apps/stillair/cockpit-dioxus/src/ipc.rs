//! ipc.rs — Tauri IPC bridge for Dioxus web. P11-003
//! Authority: Phase 11 task-decomposition P11-003
//!
//! Dual-mode IPC:
//!   1. **Tauri mode** — uses window.__TAURI_INTERNALS__.invoke() (native WebView)
//!   2. **Browser mode** — falls back to direct HTTP fetch to M0 via Trunk proxy
//!      (enables Chrome/Firefox development without Tauri)
//!
//! The browser fallback maps Tauri command names to M0 HTTP endpoints:
//!   trigger_mastering    → POST /m0/master
//!   get_session_state    → GET  /m0/blob/{blobId}  (partial — returns blob as session)
//!   get_golden_blob      → GET  /m0/blob/{blobId}
//!   playback_control     → POST /m0/playback/control
//!   get_playback_state   → GET  /m0/playback/state
//!   get_live_telemetry   → GET  /m0/playback/telemetry
//!   export_audio         → POST /m0/export
//!   open_audio_file      → browser <input type="file"> (stub: returns None)
//!
//! Trunk.toml proxies /m0/ → http://127.0.0.1:7402/ (no CORS needed).

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

// ── Tauri detection ───────────────────────────────────────────────────────────

/// Returns true if running inside a Tauri WebView (native app).
fn is_tauri() -> bool {
    let window = match web_sys::window() {
        Some(w) => w,
        None => return false,
    };
    let val: JsValue = window.into();
    match Reflect::get(&val, &JsValue::from_str("__TAURI__")) {
        Ok(tauri) => !tauri.is_undefined() && !tauri.is_null(),
        Err(_) => false,
    }
}

// ── IPC entry point ───────────────────────────────────────────────────────────

/// Invoke a Tauri command — with automatic browser fallback.
///
/// In Tauri WebView: uses window.__TAURI_INTERNALS__.invoke().
/// In Chrome/Firefox: maps the command to an M0 HTTP endpoint via Trunk proxy.
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    if is_tauri() {
        invoke_tauri(command, args).await
    } else {
        invoke_browser(command, args).await
    }
}

/// Convenience: invoke with no arguments.
pub async fn invoke_no_args<R>(command: &str) -> Result<R, String>
where
    R: DeserializeOwned,
{
    invoke(command, serde_json::json!({})).await
}

// ── Tauri-native invoke ───────────────────────────────────────────────────────

async fn invoke_tauri<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    clog!("[IPC/tauri] invoke: {}", command);

    // 1. Serialize args to a JS object via JSON
    let args_json = serde_json::to_string(&args)
        .map_err(|e| format!("[IPC] serialize failed [{command}]: {e}"))?;
    let args_js: JsValue = js_sys::JSON::parse(&args_json)
        .map_err(|e| format!("[IPC] JSON.parse failed [{command}]: {e:?}"))?;

    // 2. Get window
    let window = web_sys::window()
        .ok_or_else(|| "[IPC] no window — are we in a WASM context?".to_string())?;
    let window_val: JsValue = window.into();

    // 3. Get window.__TAURI__
    let tauri = Reflect::get(&window_val, &JsValue::from_str("__TAURI__"))
        .map_err(|e| format!("[IPC] Reflect::get __TAURI__ failed: {e:?}"))?;
    if tauri.is_undefined() || tauri.is_null() {
        return Err("[IPC] window.__TAURI__ is undefined. \
                    Check tauri.conf.json withGlobalTauri:true"
            .to_string());
    }

    // 4. Get the core object
    let core = Reflect::get(&tauri, &JsValue::from_str("core"))
        .map_err(|e| format!("[IPC] Reflect::get core failed: {e:?}"))?;
    if core.is_undefined() || core.is_null() {
        return Err("[IPC] window.__TAURI__.core is undefined.".to_string());
    }

    // 5. Get the invoke function
    let invoke_val = Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| format!("[IPC] Reflect::get invoke failed: {e:?}"))?;
    let invoke_fn: Function = invoke_val
        .dyn_into()
        .map_err(|_| "[IPC] __TAURI__.core.invoke is not a function".to_string())?;

    // 6. Call invoke(command, args) → Promise
    let promise_val = invoke_fn
        .call2(&core, &JsValue::from_str(command), &args_js)
        .map_err(|e| format!("[IPC] invoke({command}) call failed: {e:?}"))?;

    let promise: Promise = promise_val
        .dyn_into()
        .map_err(|e| format!("[IPC] invoke({command}) did not return Promise: {e:?}"))?;

    // 6. Await the Promise via JsFuture
    let result_js = JsFuture::from(promise).await.map_err(|e| {
        let msg = e
            .as_string()
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

    // 7. Deserialize result
    let result = serde_wasm_bindgen::from_value::<R>(result_js)
        .map_err(|e| format!("[IPC] deserialize failed [{command}]: {e:?}"))?;

    clog!("[IPC/tauri] ✅ {}", command);
    Ok(result)
}

// ── Browser-native fallback (HTTP fetch via Trunk proxy) ──────────────────────

/// Map Tauri command names to M0 HTTP endpoints.
/// Returns (method, url, body_json_option).
fn map_command_to_http(
    command: &str,
    args: &serde_json::Value,
) -> Result<(&'static str, String, Option<String>), String> {
    match command {
        // ── Mastering ────────────────────────────────────────────────────
        "trigger_mastering" => {
            let body = serde_json::json!({
                "audio_path": args.get("audioPath").and_then(|v| v.as_str()).unwrap_or(""),
                "preset_id":  args.get("presetId").and_then(|v| v.as_str()).unwrap_or("spotify"),
            });
            Ok(("POST", "/m0/master".into(), Some(body.to_string())))
        }

        // ── Blob / Session ───────────────────────────────────────────────
        "get_golden_blob" | "get_session_state" => {
            let blob_id = args.get("blobId").and_then(|v| v.as_str()).unwrap_or("");
            Ok(("GET", format!("/m0/blob/{blob_id}"), None))
        }

        "get_visualization_data" => {
            // Visualization is computed in Tauri backend, not in M0.
            // In browser mode, return a minimal stub so the UI doesn't crash.
            Err("[IPC/browser] get_visualization_data not available in browser mode".into())
        }

        // ── Playback ─────────────────────────────────────────────────────
        "playback_control" => {
            let body = serde_json::json!({
                "action":      args.get("action").and_then(|v| v.as_str()).unwrap_or("stop"),
                "position_ms": args.get("positionMs").and_then(|v| v.as_u64()),
            });
            Ok((
                "POST",
                "/m0/playback/control".into(),
                Some(body.to_string()),
            ))
        }

        "get_playback_state" => Ok(("GET", "/m0/playback/state".into(), None)),

        "get_live_telemetry" => Ok(("GET", "/m0/playback/telemetry".into(), None)),

        // ── Export ────────────────────────────────────────────────────────
        "export_audio" => {
            let body = serde_json::json!({
                "blob_id":     args.get("blobId").and_then(|v| v.as_str()).unwrap_or(""),
                "format":      args.get("format").and_then(|v| v.as_str()).unwrap_or("flac"),
                "output_path": args.get("outputPath").and_then(|v| v.as_str()).unwrap_or(""),
            });
            Ok(("POST", "/m0/export".into(), Some(body.to_string())))
        }

        // ── File picker (browser stub) ───────────────────────────────────
        "open_audio_file" | "load_audio_file" => {
            // Native file dialog is unavailable in browser.
            // Return None (= dialog cancelled) so the UI stays in current mode.
            Err("[IPC/browser] File dialog not available — use Tauri dev for file picking".into())
        }

        // ── Coach / Report / other ───────────────────────────────────────
        "get_coach_narrative"
        | "evaluate_findings"
        | "export_pdf_report"
        | "preview_pdf_report" => Err(format!(
            "[IPC/browser] {command} not available in browser mode"
        )),

        _ => Err(format!("[IPC/browser] Unknown command: {command}")),
    }
}

/// Browser-mode invoke: maps Tauri commands to M0 HTTP calls via Trunk proxy.
async fn invoke_browser<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    clog!("[IPC/browser] invoke: {}", command);

    let args_value =
        serde_json::to_value(&args).map_err(|e| format!("[IPC/browser] serialize failed: {e}"))?;

    let (method, url, body) = map_command_to_http(command, &args_value)?;

    let window = web_sys::window().ok_or("[IPC/browser] no window")?;

    // Build fetch request
    let opts = web_sys::RequestInit::new();
    opts.set_method(method);

    if let Some(ref body_str) = body {
        opts.set_body(&JsValue::from_str(body_str));
    }

    let request = web_sys::Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("[IPC/browser] Request::new failed: {e:?}"))?;

    if body.is_some() {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|e| format!("[IPC/browser] header set failed: {e:?}"))?;
    }

    // Execute fetch
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| {
            let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
            format!("[IPC/browser] fetch failed [{command}]: {msg}")
        })?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| "[IPC/browser] response is not a Response object".to_string())?;

    if !resp.ok() {
        let status = resp.status();
        return Err(format!("[IPC/browser] HTTP {status} for {command}"));
    }

    // Parse JSON body
    let json_promise = resp
        .json()
        .map_err(|e| format!("[IPC/browser] .json() failed: {e:?}"))?;

    let json_val = JsFuture::from(json_promise).await.map_err(|e| {
        let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
        format!("[IPC/browser] JSON parse failed [{command}]: {msg}")
    })?;

    // For trigger_mastering: M0 returns { blob_id, status, message }
    // But the Tauri command returns just the blob_id string.
    // We need to adapt the response shape for certain commands.
    let adapted_val = adapt_response(command, json_val)?;

    let result = serde_wasm_bindgen::from_value::<R>(adapted_val)
        .map_err(|e| format!("[IPC/browser] deserialize failed [{command}]: {e:?}"))?;

    clog!("[IPC/browser] ✅ {}", command);
    Ok(result)
}

/// Adapt M0 HTTP responses to match what the Tauri commands return.
///
/// M0 returns full response objects, but Tauri commands often extract/transform
/// specific fields before returning to the frontend.
fn adapt_response(command: &str, val: JsValue) -> Result<JsValue, String> {
    match command {
        "trigger_mastering" => {
            // Tauri returns Ok(blob_id: String)
            // M0 returns { blob_id: "...", status: "ok"|"error", message: "..." }
            let status = Reflect::get(&val, &JsValue::from_str("status"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();

            if status != "ok" {
                let msg = Reflect::get(&val, &JsValue::from_str("message"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "mastering failed".into());
                return Err(msg);
            }

            let blob_id = Reflect::get(&val, &JsValue::from_str("blob_id"))
                .map_err(|_| "Missing blob_id in M0 response".to_string())?;
            Ok(blob_id)
        }

        "get_session_state" => {
            // Tauri's get_session_state does: blob → findings → narrative → compose
            // In browser mode, we return a simplified session from the blob directly.
            // The blob has loudness, quality — we compose a minimal SessionStateJson.
            let loudness =
                Reflect::get(&val, &JsValue::from_str("loudness")).unwrap_or(JsValue::UNDEFINED);
            let quality =
                Reflect::get(&val, &JsValue::from_str("quality")).unwrap_or(JsValue::UNDEFINED);
            let blob_id = Reflect::get(&val, &JsValue::from_str("id"))
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();

            // Build compliance from loudness flags
            let get_bool = |obj: &JsValue, key: &str| -> bool {
                Reflect::get(obj, &JsValue::from_str(key))
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };

            let compliance = serde_json::json!({
                "spotify":   get_bool(&loudness, "spotify_compliant"),
                "youtube":   get_bool(&loudness, "youtube_compliant"),
                "apple":     get_bool(&loudness, "apple_music_compliant"),
                "tidal":     get_bool(&loudness, "tidal_compliant"),
                "broadcast": get_bool(&loudness, "broadcast_compliant"),
                "ebu_r128":  get_bool(&loudness, "ebu_r128_compliant"),
            });

            let findings = serde_json::json!({
                "issues": [],
                "recommendation": "Browser mode — rule engine not available."
            });

            // Compose SessionStateJson
            let session = js_sys::Object::new();
            Reflect::set(&session, &"blob_id".into(), &JsValue::from_str(&blob_id)).ok();
            Reflect::set(&session, &"loudness".into(), &loudness).ok();
            Reflect::set(&session, &"quality".into(), &quality).ok();

            let compliance_js =
                js_sys::JSON::parse(&compliance.to_string()).unwrap_or(JsValue::UNDEFINED);
            Reflect::set(&session, &"compliance".into(), &compliance_js).ok();

            let findings_js =
                js_sys::JSON::parse(&findings.to_string()).unwrap_or(JsValue::UNDEFINED);
            Reflect::set(&session, &"findings".into(), &findings_js).ok();

            Reflect::set(&session, &"narrative".into(), &JsValue::NULL).ok();

            // Pass through Aether fields
            let cert =
                Reflect::get(&val, &JsValue::from_str("aether_cert")).unwrap_or(JsValue::NULL);
            let persona =
                Reflect::get(&val, &JsValue::from_str("aether_persona")).unwrap_or(JsValue::NULL);
            let config =
                Reflect::get(&val, &JsValue::from_str("aether_config")).unwrap_or(JsValue::NULL);
            Reflect::set(&session, &"aether_cert".into(), &cert).ok();
            Reflect::set(&session, &"aether_persona".into(), &persona).ok();
            Reflect::set(&session, &"aether_config".into(), &config).ok();

            Ok(session.into())
        }

        "playback_control" => {
            // Tauri returns Option<PlaybackStateJson>
            // M0 returns { status, state, message }
            let state = Reflect::get(&val, &JsValue::from_str("state")).unwrap_or(JsValue::NULL);
            Ok(state)
        }

        // For these commands, the M0 response matches what Tauri returns
        "get_playback_state" | "get_live_telemetry" | "get_golden_blob" | "export_audio" => Ok(val),

        _ => Ok(val),
    }
}
