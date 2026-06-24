use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen_futures::spawn_local;

#[derive(Props, Clone, PartialEq)]
pub struct JiniDetectionProps {
    pub track_count: usize,
    pub on_timeout: EventHandler<()>,
}

#[component]
pub fn JiniDetection(props: JiniDetectionProps) -> Element {
    use_effect(move || {
        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(
            "[DET-TIMER] Detection timer started",
        ));
        spawn_local(async move {
            TimeoutFuture::new(1200).await;
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(
                "[DET-TIMER] Detection timeout fired -> dispatching",
            ));
            props.on_timeout.call(());
        });
    });

    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center;",
            p { class: "jini-text",
                if props.track_count == 1 { "Single track." }
                else { "Album. {props.track_count} tracks." }
            }
        }
    }
}
