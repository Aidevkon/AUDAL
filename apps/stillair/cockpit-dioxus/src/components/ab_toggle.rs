use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub enum AbToggleState {
    A,       // BYPASS — dim amber
    B,       // MST ACTIVE — bright amber + glow
    Toggled, // momentary B — same as B visually
}

#[derive(Props, PartialEq, Clone)]
pub struct AbToggleProps {
    pub state: AbToggleState,
    pub on_click: EventHandler<()>,
    pub on_mousedown: EventHandler<()>,
    pub on_mouseup: EventHandler<()>,
}

#[component]
pub fn AbToggle(props: AbToggleProps) -> Element {
    let outer_class = match props.state {
        AbToggleState::A => "ab-toggle ab-toggle--a",
        AbToggleState::B => "ab-toggle ab-toggle--b",
        AbToggleState::Toggled => "ab-toggle ab-toggle--toggled",
    };

    let active_b = matches!(props.state, AbToggleState::B | AbToggleState::Toggled);

    rsx! {
        div {
            class: "{outer_class}",
            onmousedown: move |_| props.on_mousedown.call(()),
            onmouseup:   move |_| props.on_mouseup.call(()),
            onclick:     move |_| props.on_click.call(()),

            div { class: "ab-actuator",
                div { class: "ab-cavity",
                    div { class: "ab-oled-screen",
                        div { class: "ab-state ab-state--a",
                            span { class: "ab-letter", "A" }
                            span { class: "ab-sub", "BYPASS" }
                        }
                        div { class: if active_b { "ab-state ab-state--b active" } else { "ab-state ab-state--b" },
                            span { class: "ab-letter",
                                "B"
                                sup { class: "ab-super", "MST" }
                            }
                            span { class: "ab-sub", "MASTER" }
                        }
                    }
                    div { class: "ab-domed-lens" }
                }
            }
        }
    }
}
