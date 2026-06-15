use dioxus::prelude::*;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use serde_json::Value;

#[derive(Clone, PartialEq, Debug)]
pub enum AlbumTrackStatus {
    Waiting,
    Scanning,
    Processing,
    Certified,
    Error,
}

impl Default for AlbumTrackStatus {
    fn default() -> Self {
        Self::Waiting
    }
}

#[derive(Clone, PartialEq)]
struct TrackState {
    index: usize,
    lufs: Option<f32>,
    cohesion_target: Option<f32>,
    has_fatigue_flag: bool,
    ducking: Option<f32>,
    width: Option<f32>,
    phantom_distance: Option<f32>,
    status: AlbumTrackStatus,
}

#[component]
pub fn AlbumMatrix(total_tracks: usize) -> Element {
    let mut tracks = use_signal(|| {
        (0..total_tracks).map(|i| TrackState {
            index: i + 1,
            lufs: None,
            cohesion_target: None,
            has_fatigue_flag: false,
            ducking: None,
            width: None,
            phantom_distance: None,
            status: AlbumTrackStatus::Waiting,
        }).collect::<Vec<TrackState>>()
    });

    use_effect(move || {
        wasm_bindgen_futures::spawn_local(async move {
            let window = web_sys::window().expect("no window");
            let window_val: wasm_bindgen::JsValue = window.into();

            if let Ok(tauri) = js_sys::Reflect::get(&window_val, &wasm_bindgen::JsValue::from_str("__TAURI__")) {
                if !tauri.is_undefined() {
                    if let Ok(event_api) = js_sys::Reflect::get(&tauri, &wasm_bindgen::JsValue::from_str("event")) {
                        if let Ok(listen_val) = js_sys::Reflect::get(&event_api, &wasm_bindgen::JsValue::from_str("listen")) {
                            if let Ok(listen_fn) = listen_val.dyn_into::<js_sys::Function>() {
                                
                                // 1. listen("album://forensic")
                                let cb_forensic = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: wasm_bindgen::JsValue| {
                                    if let Ok(payload) = js_sys::Reflect::get(&ev, &wasm_bindgen::JsValue::from_str("payload")) {
                                        if let (Ok(track_v), Ok(lufs_v)) = (
                                            js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("track")),
                                            js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("lufs"))
                                        ) {
                                            if let (Some(track), Some(lufs)) = (track_v.as_f64(), lufs_v.as_f64()) {
                                                let track_idx = track as usize;
                                                let mut state = tracks.write();
                                                if let Some(t) = state.iter_mut().find(|t| t.index == track_idx) {
                                                    t.lufs = Some(lufs as f32);
                                                    t.status = AlbumTrackStatus::Scanning;
                                                }
                                            }
                                        }
                                    }
                                }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
                                
                                let _ = listen_fn.call2(&event_api, &wasm_bindgen::JsValue::from_str("album://forensic"), cb_forensic.as_ref().unchecked_ref());
                                cb_forensic.forget();

                                // 2. listen("album://cohesion")
                                let cb_cohesion = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: wasm_bindgen::JsValue| {
                                    if let Ok(payload) = js_sys::Reflect::get(&ev, &wasm_bindgen::JsValue::from_str("payload")) {
                                        if let Ok(targets_v) = js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("per_track_targets")) {
                                            if let Ok(arr) = targets_v.dyn_into::<js_sys::Array>() {
                                                let mut state = tracks.write();
                                                for i in 0..arr.length() {
                                                    let val = arr.get(i);
                                                    if let Some(target) = val.as_f64() {
                                                        if let Some(t) = state.iter_mut().find(|t| t.index == (i as usize) + 1) {
                                                            t.cohesion_target = Some(target as f32);
                                                            t.status = AlbumTrackStatus::Processing;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
                                
                                let _ = listen_fn.call2(&event_api, &wasm_bindgen::JsValue::from_str("album://cohesion"), cb_cohesion.as_ref().unchecked_ref());
                                cb_cohesion.forget();

                                // 3. listen("album://fatigue")
                                let cb_fatigue = wasm_bindgen::closure::Closure::wrap(Box::new(move |ev: wasm_bindgen::JsValue| {
                                    if let Ok(payload) = js_sys::Reflect::get(&ev, &wasm_bindgen::JsValue::from_str("payload")) {
                                        if let (Ok(track_v), Ok(duck_v), Ok(width_v)) = (
                                            js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("track")),
                                            js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("ducking")),
                                            js_sys::Reflect::get(&payload, &wasm_bindgen::JsValue::from_str("width")),
                                        ) {
                                            if let (Some(track), Some(duck), Some(width)) = (
                                                track_v.as_f64(), duck_v.as_f64(), width_v.as_f64()
                                            ) {
                                                let track_idx = track as usize;
                                                let mut state = tracks.write();
                                                if let Some(t) = state.iter_mut().find(|t| t.index == track_idx) {
                                                    t.has_fatigue_flag = true;
                                                    t.ducking = Some(duck as f32);
                                                    t.width = Some(width as f32);
                                                    t.status = AlbumTrackStatus::Certified;
                                                    // Explicitly ignoring the reason string as per HUD Constitution
                                                }
                                            }
                                        }
                                    }
                                }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
                                
                                let _ = listen_fn.call2(&event_api, &wasm_bindgen::JsValue::from_str("album://fatigue"), cb_fatigue.as_ref().unchecked_ref());
                                cb_fatigue.forget();
                            }
                        }
                    }
                }
            }
        });
    });

    rsx! {
        div { class: "album-matrix flex flex-col gap-2 p-4",
            for track in tracks.read().iter() {
                div {
                    class: "flex flex-row items-center justify-between p-3 border border-neutral-800 rounded bg-transparent",
                    // Left Column
                    div { class: "flex flex-col w-1/4",
                        span { style: "color: var(--accent-insights); font-weight: 600;", "Track {track.index}" }
                        span { class: "text-xs text-neutral-500", "ID: TRK-{track.index:03}" }
                    }
                    // Center Column
                    div { class: "flex flex-col items-center w-1/4",
                        if let Some(lufs) = track.lufs {
                            span { class: "text-sm", "Cur: {lufs:.1} LUFS" }
                        } else {
                            span { class: "text-sm text-neutral-500", "Cur: -- LUFS" }
                        }
                        if let Some(target) = track.cohesion_target {
                            span { class: "text-sm", "Tgt: {target:.1} LUFS" }
                            if let Some(lufs) = track.lufs {
                                let delta = target - lufs;
                                let color = if delta > 0.0 { "var(--accent-amber)" } else { "var(--status-err)" };
                                span { style: "color: {color}; font-size: 0.8rem;", "Δ {delta:+.1} dB" }
                            }
                        }
                    }
                    // Right Column
                    div { class: "flex flex-col items-end w-1/4",
                        if let Some(pd) = track.phantom_distance {
                            progress {
                                style: "accent-color: var(--accent-magenta); width: 100px;",
                                value: "{pd}",
                                max: "100"
                            }
                        }
                        if let (Some(d), Some(w)) = (track.ducking, track.width) {
                            span { class: "text-xs text-neutral-400", "Dck: {d:.2} | Wdt: {w:.2}" }
                        }
                    }
                    // Flags
                    div { class: "w-1/6 flex justify-center",
                        if track.has_fatigue_flag {
                            span {
                                class: "text-xs border border-amber-500 text-amber-500 px-1 py-0.5 rounded",
                                "[⚡ FATIGUE FLAG]"
                            }
                        }
                    }
                    // Action
                    div { class: "w-1/6 flex justify-end",
                        if track.status == AlbumTrackStatus::Certified {
                            button {
                                class: "text-xs border border-green-500 text-green-500 px-2 py-1 rounded hover:bg-green-500/20 cursor-pointer bg-transparent",
                                onclick: move |_| {
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let window = web_sys::window().expect("no window");
                                        let window_val: wasm_bindgen::JsValue = window.into();
                                        if let Ok(tauri) = js_sys::Reflect::get(&window_val, &wasm_bindgen::JsValue::from_str("__TAURI__")) {
                                            if !tauri.is_undefined() {
                                                if let Ok(core) = js_sys::Reflect::get(&tauri, &wasm_bindgen::JsValue::from_str("core")) {
                                                    if let Ok(invoke) = js_sys::Reflect::get(&core, &wasm_bindgen::JsValue::from_str("invoke")) {
                                                        if let Ok(invoke_fn) = invoke.dyn_into::<js_sys::Function>() {
                                                            let _ = invoke_fn.call1(&core, &wasm_bindgen::JsValue::from_str("get_album_certificate"));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    });
                                },
                                "[ ⬇ CERTIFICATE ]"
                            }
                        } else {
                            span { class: "text-xs text-neutral-600 uppercase", "{format!(\"{:?}\", track.status)}" }
                        }
                    }
                }
            }
        }
    }
}
