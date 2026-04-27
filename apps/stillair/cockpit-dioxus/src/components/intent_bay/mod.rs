pub mod damper;
pub mod knob;

use dioxus::prelude::*;
use damper::Damper;
use knob::IntentKnob;

#[derive(Props, Clone, PartialEq)]
pub struct IntentBayProps {
    pub open: bool,
    pub tone_angle: f32,
    pub dyn_angle: f32,
    pub space_angle: f32,
    pub loud_angle: f32,
    pub on_down_tone: EventHandler<MouseEvent>,
    pub on_down_dyn: EventHandler<MouseEvent>,
    pub on_down_space: EventHandler<MouseEvent>,
    pub on_down_loud: EventHandler<MouseEvent>,
}

#[component]
pub fn IntentBay(props: IntentBayProps) -> Element {
    let noise_filter = r#"
<filter id="chassis-noise" x="0%" y="0%" width="100%" height="100%">
  <feTurbulence type="fractalNoise" baseFrequency="0.65" numOctaves="3"
                stitchTiles="stitch" result="noise"/>
  <feColorMatrix type="saturate" values="0" in="noise" result="grey"/>
  <feBlend in="SourceGraphic" in2="grey" mode="overlay" result="blend"/>
  <feComposite in="blend" in2="SourceGraphic" operator="in"/>
</filter>
    "#;

    rsx! {
        div { 
            class: if props.open { "intent-bay open" } else { "intent-bay" },
            svg { 
                width: "0", height: "0", 
                dangerous_inner_html: "{noise_filter}" 
            }
            div { class: "intent-bay-top",
                Damper {}
            }
            div { class: "intent-knobs-wrapper",
                div { class: "intent-knobs",
                    IntentKnob { label: "TONE", range: "Warm ↔ Bright", angle: props.tone_angle, highlighted: true, on_down: move |e| props.on_down_tone.call(e) }
                    IntentKnob { label: "DYNAMICS", range: "Punch ↔ Glue", angle: props.dyn_angle, highlighted: false, on_down: move |e| props.on_down_dyn.call(e) }
                    IntentKnob { label: "SPACE", range: "Center ↔ Widen", angle: props.space_angle, highlighted: false, on_down: move |e| props.on_down_space.call(e) }
                    IntentKnob { label: "LOUDNESS", range: "Gain ↔ Ceiling", angle: props.loud_angle, highlighted: false, on_down: move |e| props.on_down_loud.call(e) }
                }
            }
        }
    }
}
