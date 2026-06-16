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
        
        let f: Rc<RefCell<Option<wasm_bindgen::closure::Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let g = f.clone();
        
        let props_clone = props_for_effect.clone();
        let f_clone = f.clone();
        
        *g.borrow_mut() = Some(wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            let width = props_clone.width as f64;
            let height = props_clone.height as f64;
            
            ctx.clear_rect(0.0, 0.0, width, height);

            let time_ms = js_sys::Date::now();
            let mut spectrum = vec![];
            if let Some(telemetry_signal) = &props_clone.telemetry {
                if let Some(frame) = telemetry_signal.read().clone() {
                    spectrum = frame.spectrum;
                }
            }

            render_grid(&ctx, width, height, time_ms);
            render_topography(&ctx, width, height, &spectrum);
            render_lasers(&ctx, width, height, time_ms, props_clone.bpm, &spectrum);

            let window = web_sys::window().unwrap();
            
            let cb = f_clone.borrow();
            let closure_ref: &wasm_bindgen::closure::Closure<dyn FnMut()> = cb.as_ref().unwrap();
            let js_val: &JsValue = <wasm_bindgen::closure::Closure<dyn FnMut()> as AsRef<JsValue>>::as_ref(closure_ref);
            let id = window.request_animation_frame(js_val.unchecked_ref()).unwrap();
            raf_id.set(Some(id));
        }) as Box<dyn FnMut()>));
        
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

fn render_grid(ctx: &CanvasRenderingContext2d, width: f64, height: f64, time_ms: f64) {
    ctx.set_global_alpha(0.15);
    ctx.set_stroke_style_str("#00d1ff");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    let vp_x = width / 2.0;
    let vp_y = 0.0;

    // 12 vertical lines with jitter
    for i in 0..GRID_COLS {
        let x = (i as f64 / (GRID_COLS - 1) as f64) * width;
        let jitter = libm::sin(time_ms * 0.05 + i as f64) * MAX_JITTER_PX;
        ctx.move_to(vp_x, vp_y);
        ctx.line_to(x + jitter, height);
    }

    // 8 horizontal lines
    for i in 0..GRID_ROWS {
        let y = (i as f64 / (GRID_ROWS - 1) as f64) * height;
        ctx.move_to(0.0, y);
        ctx.line_to(width, y);
    }

    ctx.stroke();
    ctx.set_global_alpha(1.0);
}

fn render_topography(ctx: &CanvasRenderingContext2d, width: f64, height: f64, spectrum: &[f32]) {
    if spectrum.is_empty() { return; }

    let avg = spectrum.iter().sum::<f32>() / spectrum.len() as f32;
    let color = if avg > -12.0 { "#ff2a7f" } else { "#00d1ff" };

    ctx.set_stroke_style_str(color);
    ctx.set_line_width(2.0);
    ctx.begin_path();

    for i in 0..spectrum.len() {
        let x = (i as f64 / spectrum.len() as f64) * width;
        let y = height - ((spectrum[i].clamp(-60.0, 0.0) + 60.0) as f64 / 60.0 * height * 0.8);
        if i == 0 {
            ctx.move_to(x, y);
        } else {
            ctx.line_to(x, y);
        }
    }
    ctx.stroke();
}

fn render_lasers(ctx: &CanvasRenderingContext2d, width: f64, height: f64, time_ms: f64, bpm: f32, spectrum: &[f32]) {
    if spectrum.is_empty() { return; }

    let pulse_ms = if bpm > 0.0 { 60000.0 / bpm as f64 } else { FALLBACK_PULSE_MS };
    let phase = (time_ms % pulse_ms) / pulse_ms;
    let opacity = 0.4 + 0.6 * libm::sin(phase * PI * 2.0).abs();

    ctx.set_global_alpha(opacity);
    ctx.set_stroke_style_str("#c8a832");
    ctx.set_line_width(1.0);
    ctx.begin_path();

    for i in 0..spectrum.len() {
        if spectrum[i] > -6.0 {
            let x = (i as f64 / spectrum.len() as f64) * width;
            ctx.move_to(x, height);
            ctx.line_to(x, 0.0);
        }
    }
    ctx.stroke();
    ctx.set_global_alpha(1.0);
}
