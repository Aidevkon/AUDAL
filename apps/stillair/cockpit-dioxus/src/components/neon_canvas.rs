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
const GRID_COLS: u32         = 8;
const GRID_ROWS: u32         = 5;
const MAX_JITTER_PX: f64     = 2.0;  // reduced — iso lines are already dynamic enough
const PI: f64                = core::f64::consts::PI;

// ── Isometric projection constants ───────────────────────────────────────────
// One shared projection for grid, Ghost, and Core.
// iso_project(x, y, z) maps:
//   x → frequency axis  (goes lower-right as x increases)
//   y → amplitude axis  (goes straight up as y increases)
//   z → depth axis      (goes upper-left as z increases = Ghost sits behind Core)
//
// Tuning:
//   ISO_ANGLE    — angle of x/z axes from horizontal (30° = classic iso)
//   Z_DEPTH_SCALE — pixels of separation per z unit; set high enough that
//                   Ghost (z=1) is clearly above/behind Core (z=0).
//   X_FREQ_SCALE  — fraction of canvas width used for the frequency axis;
//                   keeps the projected floor inside the canvas at ISO_ANGLE=30°.
const ISO_ANGLE:     f64 = PI / 6.0;   // 30°
const Z_DEPTH_SCALE: f64 = 80.0;       // px depth per z unit
const X_FREQ_SCALE:  f64 = 0.55;       // frequency axis uses 55% of canvas width
const GHOST_Z:       f64 = 1.0;        // Ghost = back plane
const CORE_Z:        f64 = 0.0;        // Core  = front plane

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

            // [DRAW-CHECK] + [BIN-DUMP] throttled — fires every ~60 frames (~1s).
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

                    // [BIN-DUMP] raw bins 0..10 — same block, guaranteed to fire.
                    let fmt = |v: &[f32]| -> String {
                        v.iter().take(10)
                            .map(|&x| format!("{:.1}", x))
                            .collect::<Vec<_>>()
                            .join(",")
                    };
                    web_sys::console::log_1(&format!(
                        "[BIN-DUMP] after[0..10]=[{}] before[0..10]=[{}]",
                        fmt(&spectrum_after),
                        fmt(&spectrum_before),
                    ).into());
                }
            }

            // Ghost FIRST (drawn under Core), Core on top.
            if props_clone.is_delta_mode {
                render_ghost(&ctx, width, height, &spectrum_before);
                render_core(&ctx, width, height, &spectrum_before, &spectrum_after);
            } else {
                // Legacy single-line fallback (non-delta callers).
                render_core(&ctx, width, height, &spectrum_after, &spectrum_after);
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

// ── Oblique / Cavalier projection ─────────────────────────────────────────────
//
// ONE shared function for grid, Ghost, and Core.
//
// Axes — the pro-audio standard (FabFilter Pro-Q, iZotope Ozone style):
//   X axis (frequency):  HORIZONTAL  — left to right, no tilt
//   Y axis (amplitude):  VERTICAL    — straight up, no tilt
//   Z axis (depth):      45° tilt    — each z unit shoves the plane RIGHT and UP
//
// Inputs (all normalized):
//   x: 0..1 — frequency bin position (0=lowest, 1=highest)
//   y: 0..1 — amplitude (0=floor/silence, 1=peak)
//   z: 0.0  — Core (front plane)  |  1.0 — Ghost (back plane)
//
// At z=0 this is an exact flat spectrum plot (horizontal+vertical).
// At z=1 the whole plane shifts up-right by (Z_OFFX, Z_OFFY) — parallel copy.
//
// Tuning constants (all proportional to canvas size — responsive):
//   MARGIN_L  — left padding before frequency axis starts
//   PLOT_W    — horizontal width for the frequency axis
//   FLOOR_Y   — y position of the silence floor (front plane baseline)
//   AMP_H     — maximum amplitude height in pixels (loudest peak)
//   Z_OFFX    — depth offset RIGHT per z unit
//   Z_OFFY    — depth offset UP per z unit

#[inline]
fn perspective_project(x: f64, y: f64, z: f64, w: f64, h: f64) -> (f64, f64) {
    let center_x = w * 0.5;
    let horizon_y = h * 0.25; 
    
    // Base width at front plane (z=0)
    let plot_w = w * 0.85; 
    let base_x = center_x + (x - 0.5) * plot_w;
    
    // Y axis (amplitude) mapped to floor
    let floor_y = h * 0.85;
    let amp_h = h * 0.45;
    let base_y = floor_y - y * amp_h;
    
    // Perspective scaling (z=1 is back plane)
    let depth_strength = 1.5; 
    let scale = 1.0 / (1.0 + z * depth_strength);
    
    let sx = center_x + (base_x - center_x) * scale;
    let sy = horizon_y + (base_y - horizon_y) * scale;
    
    (sx, sy)
}

// ── Grid ─────────────────────────────────────────────────────────────────────
//
// Oblique floor:
//   - Horizontal amplitude lines on front plane (z=0) and back plane (z=1)
//   - Depth connectors: short 45° lines from z=0 to z=1 at a few x positions
// Sparse: ~6 amplitude bands, ~5 depth connectors.

fn render_grid(ctx: &CanvasRenderingContext2d, width: f64, height: f64, _time_ms: f64) {
    // ── Grid Lines ──
    ctx.set_global_alpha(0.25);
    ctx.set_stroke_style_str("#00d1ff");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    // Depth lines (Z axis)
    for i in 0..=10 {
        let x = i as f64 / 10.0;
        let (fx, fy) = perspective_project(x, 0.0, 0.0, width, height);
        let (bx, by) = perspective_project(x, 0.0, 1.0, width, height);
        ctx.move_to(fx, fy);
        ctx.line_to(bx, by);
    }

    // Horizontal frequency lines (X axis) at depth intervals
    for i in 0..=5 {
        let z = i as f64 / 5.0;
        let (x0, y0) = perspective_project(0.0, 0.0, z, width, height);
        let (x1, y1) = perspective_project(1.0, 0.0, z, width, height);
        ctx.move_to(x0, y0);
        ctx.line_to(x1, y1);
    }
    ctx.stroke();

    // ── Concentric Rings ──
    ctx.set_global_alpha(0.15);
    ctx.set_stroke_style_str("#00d1ff");
    for r in 1..=4 {
        let radius = (r as f64) * 0.12;
        ctx.begin_path();
        for angle_deg in (0..=360).step_by(5) {
            let angle = (angle_deg as f64) * std::f64::consts::PI / 180.0;
            let cx = 0.5 + radius * angle.cos();
            let cz = 0.5 + radius * angle.sin();
            let (px, py) = perspective_project(cx, 0.0, cz, width, height);
            if angle_deg == 0 {
                ctx.move_to(px, py);
            } else {
                ctx.line_to(px, py);
            }
        }
        ctx.stroke();
    }

    // ── Band Markers ──
    ctx.set_global_alpha(0.60);
    ctx.set_fill_style_str("#00d1ff");
    ctx.set_font("10px 'Inter', monospace");
    ctx.set_text_align("center");
    
    let bands = [
        (0.1, "Low"),
        (0.3, "Low-Mid"),
        (0.5, "Mid"),
        (0.7, "High-Mid"),
        (0.9, "High"),
    ];
    
    ctx.begin_path();
    for &(x, label) in &bands {
        let (fx, fy) = perspective_project(x, 0.0, 0.0, width, height);
        ctx.fill_text(label, fx, fy + 16.0).unwrap();
        ctx.move_to(fx, fy);
        ctx.line_to(fx, fy + 4.0);
    }
    ctx.stroke();

    ctx.set_global_alpha(1.0);
}

// ── Ghost line (spectrum_before — pre-mastering / raw PCM) ───────────────────
// Drawn at z=1.0 (back plane). Uses oblique_project — same space as the grid.

fn render_ghost(ctx: &CanvasRenderingContext2d, width: f64, height: f64, spectrum: &[f32]) {
    if spectrum.is_empty() {
        // Fallback: flat line on Ghost's back plane so RAF is confirmed alive.
        ctx.set_global_alpha(0.30);
        ctx.set_stroke_style_str("#667788");
        ctx.set_line_width(1.0);
        ctx.begin_path();
        let (x0, y0) = perspective_project(0.0, 0.0, 1.0, width, height);
        let (x1, y1) = perspective_project(1.0, 0.0, 1.0, width, height);
        ctx.move_to(x0, y0);
        ctx.line_to(x1, y1);
        ctx.stroke();
        ctx.set_global_alpha(1.0);
        return;
    }

    // Blueprint wireframe — faint, no glow, recedes behind Core.
    ctx.set_global_alpha(0.60);
    ctx.set_stroke_style_str("#667788");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    let n = spectrum.len() as f64;
    for (i, &db) in spectrum.iter().enumerate() {
        let x = i as f64 / n;
        let y = (db.clamp(-60.0, 0.0) + 60.0) as f64 / 60.0;
        let (sx, sy) = perspective_project(x, y, 1.0, width, height);
        if i == 0 { ctx.move_to(sx, sy); } else { ctx.line_to(sx, sy); }
    }

    ctx.stroke();
    ctx.set_global_alpha(1.0);
}

// ── Core line (spectrum_after — post-mastering / mastered PCM) ───────────────
// Drawn at z=0.0 (front plane). Uses oblique_project — same space as Ghost/grid.

fn render_core(ctx: &CanvasRenderingContext2d, width: f64, height: f64, spectrum_before: &[f32], spectrum_after: &[f32]) {
    if spectrum_after.is_empty() { return; }

    let n = spectrum_after.len() as f64;
    
    // Pre-calculate all projected points and deltas
    let mut points = Vec::with_capacity(spectrum_after.len());
    let mut deltas = Vec::with_capacity(spectrum_after.len());
    
    for (i, &db_after) in spectrum_after.iter().enumerate() {
        let x = i as f64 / n;
        let y = (db_after.clamp(-60.0, 0.0) + 60.0) as f64 / 60.0;
        let (sx, sy) = perspective_project(x, y, 0.0, width, height);
        points.push((sx, sy));
        
        let db_before = spectrum_before.get(i).copied().unwrap_or(db_after);
        deltas.push(db_after - db_before);
    }

    ctx.set_line_width(2.5);

    // 1. Boost segments (Δ > +1dB) — Teal
    ctx.set_stroke_style_str("#00d1ff");
    ctx.set_shadow_color("#00d1ff");
    ctx.set_shadow_blur(15.0);
    ctx.begin_path();
    for i in 1..points.len() {
        if deltas[i] > 1.0 {
            ctx.move_to(points[i-1].0, points[i-1].1);
            ctx.line_to(points[i].0, points[i].1);
        }
    }
    ctx.stroke();

    // 2. Cut segments (Δ < -1dB) — Indigo
    ctx.set_stroke_style_str("#5c33ff");
    ctx.set_shadow_color("#5c33ff");
    ctx.set_shadow_blur(15.0);
    ctx.begin_path();
    for i in 1..points.len() {
        if deltas[i] < -1.0 {
            ctx.move_to(points[i-1].0, points[i-1].1);
            ctx.line_to(points[i].0, points[i].1);
        }
    }
    ctx.stroke();

    // 3. Neutral segments (else) — Dim cyan, no glow
    ctx.set_stroke_style_str("rgba(0, 209, 255, 0.4)");
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_color("transparent");
    ctx.begin_path();
    for i in 1..points.len() {
        let d = deltas[i];
        if d >= -1.0 && d <= 1.0 {
            ctx.move_to(points[i-1].0, points[i-1].1);
            ctx.line_to(points[i].0, points[i].1);
        }
    }
    ctx.stroke();

    // Reset shadow — must not bleed onto subsequent grid/ghost draws.
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_color("transparent");
}

// ── Laser overlay (legacy single-line mode only) ──────────────────────────────

fn render_lasers(ctx: &CanvasRenderingContext2d, width: f64, height: f64, time_ms: f64, bpm: f32, spectrum: &[f32]) {
    if spectrum.is_empty() { return; }
    let pulse_ms = if bpm > 0.0 { 60000.0 / bpm as f64 } else { FALLBACK_PULSE_MS };
    let phase    = (time_ms % pulse_ms) / pulse_ms;
    let opacity  = 0.4 + 0.6 * libm::sin(phase * PI * 2.0).abs();
    ctx.set_global_alpha(opacity);
    ctx.set_stroke_style_str("#c8a832");
    ctx.set_line_width(1.0);
    ctx.begin_path();
    let n = spectrum.len() as f64;
    for (i, &db) in spectrum.iter().enumerate() {
        if db > -6.0 {
            let x = (i as f64 / n) * width;
            ctx.move_to(x, height);
            ctx.line_to(x, 0.0);
        }
    }
    ctx.stroke();
    ctx.set_global_alpha(1.0);
}
