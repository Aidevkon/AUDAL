//! neon_canvas.rs — 2.5D Delta Landscape canvas.
//!
//! Iteration 1 (STEP 4): DATA FLOW + MEMORY SAFETY only.
//!   - Ghost line = spectrum_before (pre-mastering / raw PCM)
//!   - Core  line = spectrum_after  (post-mastering / mastered PCM)
//!   - Plain polylines, no glow/shadow/perspective math yet (iteration 2).
//!
//! RAF pattern: Rc<RefCell<Option<Closure>>> self-referential loop.
//! No forget(). use_drop cancels the RAF id on component teardown.
//! DO NOT change the RAF setup without careful memory-safety review.

use std::rc::Rc;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, CanvasRenderingContext2d};
use dioxus::prelude::*;
use crate::types::{RealtimeFrameJson, SessionStateJson};
use libm;

const FALLBACK_PULSE_MS: f64 = 500.0;
const GRID_COLS: u32         = 12;
const GRID_ROWS: u32         = 8;
const MAX_JITTER_PX: f64     = 6.0;
const PI: f64                = core::f64::consts::PI;

#[derive(Props, Clone, PartialEq)]
pub struct NeonCanvasProps {
    pub telemetry: Option<Signal<Option<RealtimeFrameJson>>>,
    pub session_state: Option<Signal<Option<SessionStateJson>>>,
    pub bpm: f32,
    pub width: u32,
    pub height: u32,
    pub paused: bool,
    pub is_delta_mode: bool,
}

#[component]
pub fn NeonCanvas(props: NeonCanvasProps) -> Element {
    let mut canvas_ref = use_signal(|| None::<Rc<HtmlCanvasElement>>);
    let mut raf_id = use_signal(|| None::<i32>);

    let props_for_effect = props.clone();
    use_effect(move || {
        let Some(canvas_el) = canvas_ref.read().clone() else {
            return;
        };
        let ctx = canvas_el
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into::<CanvasRenderingContext2d>()
            .unwrap();

        let window = web_sys::window().unwrap();

        // ── RAF closure storage: Rc<RefCell<Option<Closure>>> ─────────────────
        // The closure holds a clone of `f` so it can re-schedule itself each
        // frame.  No forget() — the Rc keeps it alive as long as the component
        // lives, and use_drop below cancels the pending RAF on teardown.
        let f: Rc<RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut()>>>> =
            Rc::new(RefCell::new(None));
        let g = f.clone();

        // Clone the canvas element handle so the RAF closure can read its
        // dimensions live every frame. This makes the canvas resilient to
        // container resizes (e.g. intent-reveal-zone expanding/collapsing).
        let canvas_for_raf = canvas_el.clone();

        let props_clone = props_for_effect.clone();
        let f_clone = f.clone();

        *g.borrow_mut() = Some(wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            // Sync draw buffer to CSS display size each frame.
            // canvas.width/height (the draw buffer) must equal clientWidth/clientHeight
            // (the CSS display size) or coordinates distort. We do this lazily — only
            // when the size actually changes — to avoid thrashing the GPU texture.
            let cw = canvas_for_raf.client_width()  as u32;
            let ch = canvas_for_raf.client_height() as u32;
            if cw > 0 && ch > 0 {
                if canvas_for_raf.width()  != cw { canvas_for_raf.set_width(cw);  }
                if canvas_for_raf.height() != ch { canvas_for_raf.set_height(ch); }
            }

            // Read the now-correct draw-buffer dimensions for all coordinate math.
            let width  = canvas_for_raf.width()  as f64;
            let height = canvas_for_raf.height() as f64;
            ctx.clear_rect(0.0, 0.0, width, height);

            let time_ms = js_sys::Date::now();

            // ── Read both spectra from the telemetry signal ────────────────
            let mut spectrum_before: Vec<f32> = vec![];
            let mut spectrum_after:  Vec<f32> = vec![];
            if let Some(telemetry_signal) = &props_clone.telemetry {
                if let Some(frame) = telemetry_signal.read().clone() {
                    spectrum_before = frame.spectrum_before;
                    spectrum_after  = frame.spectrum_after;
                }
            }

            // ── Draw ──────────────────────────────────────────────────────
            render_grid(&ctx, width, height, time_ms);

            // [CANVAS-NEW] one-shot: confirm which canvas instance this is.
            {
                use std::sync::atomic::{AtomicBool, Ordering};
                static CN: AtomicBool = AtomicBool::new(false);
                if !CN.swap(true, Ordering::Relaxed) {
                    web_sys::console::log_1(&format!(
                        "[CANVAS-NEW] instance active — is_delta_mode={} buf={}x{}",
                        props_clone.is_delta_mode, width as u32, height as u32
                    ).into());
                }
            }

            // [DRAW-CHECK] throttled: confirm spectrum data reaches this closure.
            {
                use std::sync::atomic::{AtomicU32, Ordering};
                static FC: AtomicU32 = AtomicU32::new(0);
                let n = FC.fetch_add(1, Ordering::Relaxed);
                if n % 60 == 0 {
                    let b20 = spectrum_before.get(20).copied().unwrap_or(f32::NAN);
                    let a20 = spectrum_after.get(20).copied().unwrap_or(f32::NAN);
                    web_sys::console::log_1(&format!(
                        "[DRAW-CHECK] frame={} delta={} before.len={} after.len={} before[20]={:.1} after[20]={:.1}",
                        n, props_clone.is_delta_mode,
                        spectrum_before.len(), spectrum_after.len(),
                        b20, a20
                    ).into());
                }
            }

            // Ghost FIRST (drawn under Core), Core on top.
            if props_clone.is_delta_mode {
                render_ghost(&ctx, width, height, &spectrum_before);
                render_core(&ctx, width, height, &spectrum_after);
            } else {
                // Legacy single-line fallback (non-delta callers).
                render_core(&ctx, width, height, &spectrum_after);
                render_lasers(&ctx, width, height, time_ms, props_clone.bpm, &spectrum_after);
            }

            // Re-schedule next frame (same Rc — no new allocation).
            let window = web_sys::window().unwrap();
            let cb = f_clone.borrow();
            let closure_ref: &wasm_bindgen::closure::Closure<dyn FnMut()> = cb.as_ref().unwrap();
            let js_val: &JsValue = <wasm_bindgen::closure::Closure<dyn FnMut()> as AsRef<JsValue>>::as_ref(closure_ref);
            let id = window.request_animation_frame(js_val.unchecked_ref()).unwrap();
            raf_id.set(Some(id));
        }) as Box<dyn FnMut()>));

        // Fire the first frame.
        let cb = g.borrow();
        let closure_ref: &wasm_bindgen::closure::Closure<dyn FnMut()> = cb.as_ref().unwrap();
        let js_val: &JsValue = <wasm_bindgen::closure::Closure<dyn FnMut()> as AsRef<JsValue>>::as_ref(closure_ref);
        let id = window.request_animation_frame(js_val.unchecked_ref()).unwrap();
        raf_id.set(Some(id));
    });

    use_drop(move || {
        let id_opt = raf_id.read().clone();
        if let Some(id) = id_opt {
            if let Some(window) = web_sys::window() {
                window.cancel_animation_frame(id).ok();
            }
        }
    });

    rsx! {
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: "width: 100%; height: 100%; background: var(--bg-glass, #080809);",
            onmounted: move |cx| {
                if let Some(canvas_element) = cx.data().downcast::<web_sys::Element>() {
                    if let Ok(canvas) = canvas_element.clone().dyn_into::<HtmlCanvasElement>() {
                        canvas_ref.set(Some(Rc::new(canvas)));
                    }
                }
            }
        }
    }
}

// ── Projection helper ───────────────────────────────────────────────────────
//
// Maps a (base_x, base_y, z) point into 2D canvas coordinates using the same
// vanishing point as render_grid() — top-center at (width/2, 0).
//
// z = 0.0 → foreground (no transform, Core sits here)
// z = 1.0 → full depth (maximum push toward horizon)
//
// Tuning constants are derived from the grid geometry:
//   SKEW_H (0.22): horizontal compression factor — at z=0.65 the Ghost edges
//                  compress ~14% toward centre, matching the grid's convergence.
//   SKEW_V (0.30): vertical lift factor — at z=0.65 Ghost rises ~54px on a
//                  280px canvas, placing it visibly above Core on the grid plane.

const GHOST_Z: f64 = 1.0;  // Ghost depth (0=front … 1=back). Use 1.0 with tiny SKEW_V.
const CORE_Z:  f64 = 0.0;  // Core sits at the foreground — identity transform.
const SKEW_H:  f64 = 0.06; // horizontal pixels: at z=1 left-edge moves ~24px inward (400*0.06)
const SKEW_V:  f64 = 0.10; // vertical pixels: at z=1 Ghost rises ~28px on a 280px canvas

#[inline]
fn project(base_x: f64, base_y: f64, z: f64, width: f64, height: f64) -> (f64, f64) {
    let vp_x = width * 0.5; // matches render_grid vanishing point
    let rx = base_x + (vp_x - base_x) * z * SKEW_H;
    let ry = base_y - height * z * SKEW_V;
    (rx, ry)
}

// ── Grid ─────────────────────────────────────────────────────────────────────

fn render_grid(ctx: &CanvasRenderingContext2d, width: f64, height: f64, time_ms: f64) {
    ctx.set_global_alpha(0.15);
    ctx.set_stroke_style_str("#00d1ff");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    let vp_x = width / 2.0;
    let vp_y = 0.0;

    for i in 0..GRID_COLS {
        let x = (i as f64 / (GRID_COLS - 1) as f64) * width;
        let jitter = libm::sin(time_ms * 0.05 + i as f64) * MAX_JITTER_PX;
        ctx.move_to(vp_x, vp_y);
        ctx.line_to(x + jitter, height);
    }

    for i in 0..GRID_ROWS {
        let y = (i as f64 / (GRID_ROWS - 1) as f64) * height;
        ctx.move_to(0.0, y);
        ctx.line_to(width, y);
    }

    ctx.stroke();
    ctx.set_global_alpha(1.0);
}

// ── Ghost line (spectrum_before — pre-mastering / raw PCM) ───────────────────
// Step (a): Z-skew applied at GHOST_Z so Ghost sits on the grid's back plane.

fn render_ghost(ctx: &CanvasRenderingContext2d, width: f64, height: f64, spectrum: &[f32]) {
    // [DEBUG] fallback stroke when spectrum empty — confirms canvas/RAF is alive
    if spectrum.is_empty() {
        ctx.set_global_alpha(0.25);
        ctx.set_stroke_style_str("#ffffff");
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.move_to(0.0, height * 0.45);
        ctx.line_to(width, height * 0.45);
        ctx.stroke();
        ctx.set_global_alpha(1.0);
        return;
    }

    // [SKEW-CHECK] one-shot: print actual projected coords to browser console.
    // Remove after confirming lines are on-screen.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static SC: AtomicBool = AtomicBool::new(false);
        if !SC.swap(true, Ordering::Relaxed) {
            let bands = [0usize, 20, 63];
            let mut msg = format!("[SKEW-CHECK] canvas={}x{}  z_ghost={} z_core={} SKEW_H={} SKEW_V={}",
                width as u32, height as u32, GHOST_Z, CORE_Z, SKEW_H, SKEW_V);
            for &idx in &bands {
                if let Some(&db) = spectrum.get(idx) {
                    let bx = (idx as f64 / spectrum.len() as f64) * width;
                    let by = height - ((db.clamp(-60.0,0.0)+60.0) as f64/60.0*height*0.8);
                    let (rx, ry) = project(bx, by, GHOST_Z, width, height);
                    msg.push_str(&format!("  | ghost[{}] base=({:.0},{:.0}) proj=({:.0},{:.0})", idx, bx, by, rx, ry));
                }
            }
            web_sys::console::log_1(&msg.into());
        }
    }

    ctx.set_global_alpha(0.45);
    ctx.set_stroke_style_str("#aabbcc");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    for (i, &db) in spectrum.iter().enumerate() {
        let base_x = (i as f64 / spectrum.len() as f64) * width;
        let base_y = height - ((db.clamp(-60.0, 0.0) + 60.0) as f64 / 60.0 * height * 0.8);
        let (rx, ry) = project(base_x, base_y, GHOST_Z, width, height);
        if i == 0 { ctx.move_to(rx, ry); } else { ctx.line_to(rx, ry); }
    }

    ctx.stroke();
    ctx.set_global_alpha(1.0);
}

// ── Core line (spectrum_after — post-mastering / mastered PCM) ───────────────
// Step (a): Z-skew applied at CORE_Z (0.0 = no transform — Core stays front).

fn render_core(ctx: &CanvasRenderingContext2d, width: f64, height: f64, spectrum: &[f32]) {
    if spectrum.is_empty() { return; }

    let avg = spectrum.iter().sum::<f32>() / spectrum.len() as f32;
    let color = if avg > -12.0 { "#ff2a7f" } else { "#00d1ff" };

    ctx.set_stroke_style_str(color);
    ctx.set_line_width(2.0);
    ctx.begin_path();

    for (i, &db) in spectrum.iter().enumerate() {
        let base_x = (i as f64 / spectrum.len() as f64) * width;
        let base_y = height - ((db.clamp(-60.0, 0.0) + 60.0) as f64 / 60.0 * height * 0.8);
        let (rx, ry) = project(base_x, base_y, CORE_Z, width, height);
        if i == 0 { ctx.move_to(rx, ry); } else { ctx.line_to(rx, ry); }
    }

    ctx.stroke();
}

// ── Laser overlay (legacy single-line mode only) ──────────────────────────────

fn render_lasers(ctx: &CanvasRenderingContext2d, width: f64, height: f64, time_ms: f64, bpm: f32, spectrum: &[f32]) {
    if spectrum.is_empty() { return; }

    let pulse_ms = if bpm > 0.0 { 60000.0 / bpm as f64 } else { FALLBACK_PULSE_MS };
    let phase = (time_ms % pulse_ms) / pulse_ms;
    let opacity = 0.4 + 0.6 * libm::sin(phase * PI * 2.0).abs();

    ctx.set_global_alpha(opacity);
    ctx.set_stroke_style_str("#c8a832");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    for (i, &db) in spectrum.iter().enumerate() {
        if db > -6.0 {
            let x = (i as f64 / spectrum.len() as f64) * width;
            ctx.move_to(x, height);
            ctx.line_to(x, 0.0);
        }
    }

    ctx.stroke();
    ctx.set_global_alpha(1.0);
}
