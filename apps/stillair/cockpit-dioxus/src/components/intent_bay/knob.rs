use dioxus::prelude::*;
use gloo_events::EventListener;
use wasm_bindgen::JsCast;
use web_sys::window;

#[derive(Props, Clone, PartialEq)]
pub struct IntentKnobProps {
    pub label: &'static str,
    pub range: &'static str,
    pub highlighted: bool,
    pub angle: Signal<f32>,
    pub on_down: EventHandler<MouseEvent>,
}

#[component]
pub fn IntentKnob(props: IntentKnobProps) -> Element {
    let hl_class = if props.highlighted { "highlighted" } else { "" };
    let socket_class = if props.highlighted {
        "intent-knob-socket socket-active"
    } else {
        "intent-knob-socket"
    };

    let mut dragging = use_signal(|| false);
    let mut start_y = use_signal(|| 0.0_f64);
    let mut start_angle = use_signal(|| 0.0_f32);
    let mut listeners = use_signal(|| None::<(EventListener, EventListener)>);

    let on_mouse_down = move |e: MouseEvent| {
        props.on_down.call(e.clone());
        dragging.set(true);
        start_y.set(e.client_coordinates().y);
        start_angle.set(*props.angle.read());

        if let Some(win) = window() {
            let mut angle_sig = props.angle;
            
            let move_listener = EventListener::new(&win, "mousemove", move |ev: &web_sys::Event| {
                let me = ev.unchecked_ref::<web_sys::MouseEvent>();
                let dy = *start_y.read() - me.client_y() as f64;
                let new_angle = (*start_angle.read() + (dy as f32) * 1.5).clamp(-135.0, 135.0);
                angle_sig.set(new_angle);
            });

            let up_listener = EventListener::new(&win, "mouseup", move |_: &web_sys::Event| {
                dragging.set(false);
                listeners.set(None);
            });

            listeners.set(Some((move_listener, up_listener)));
        }
    };

    rsx! {
        div { class: "intent-knob-container",
            div { class: "intent-knob-label", "{props.label}" }
            // ── Cavity socket — conical sinkhole ──
            div { class: "{socket_class}",
                // ── Knob cap rotates entirely — conic-gradient brushing turns with it ──
                div {
                    class: "intent-knob {hl_class}",
                    style: "transform: rotate({props.angle.read()}deg);",
                    onmousedown: on_mouse_down,
                    // Indicator is fixed on knob face — rotates with parent
                    div { class: "intent-knob-indicator {hl_class}" }
                }
            }
            div { class: "intent-knob-range", "{props.range}" }
        }
    }
}

pub fn normalize_angle(angle: f32) -> f32 {
    angle / 135.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intent_knob_normalization() {
        assert_eq!(normalize_angle(135.0), 1.0);
        assert_eq!(normalize_angle(-135.0), -1.0);
        assert_eq!(normalize_angle(0.0), 0.0);
    }
}
