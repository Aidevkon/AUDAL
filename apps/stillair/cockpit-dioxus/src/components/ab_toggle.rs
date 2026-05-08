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
        AbToggleState::A       => "ab-toggle ab-toggle--a",
        AbToggleState::B       => "ab-toggle ab-toggle--b",
        AbToggleState::Toggled => "ab-toggle ab-toggle--toggled",
    };

    let (top_label, top_sub, bot_label, bot_sub) = match props.state {
        AbToggleState::A => ("A", "BYPASS", "B", "MASTER"),
        AbToggleState::B | AbToggleState::Toggled => ("A", "BYPASS", "B", "MASTER"),
    };

    rsx! {
        div {
            class: "{outer_class}",
            onmousedown: move |_| props.on_mousedown.call(()),
            onmouseup:   move |_| props.on_mouseup.call(()),
            onclick:     move |_| props.on_click.call(()),

            // Knurled bezel ring
            div { class: "ab-toggle__bezel",
                // SVG knurled pattern
                svg {
                    class: "ab-toggle__knurl",
                    view_box: "0 0 100 100",
                    // 36 tick marks around the ring
                    for i in 0..36_u32 {
                        line {
                            x1: "50",
                            y1: "4",
                            x2: "50",
                            y2: "10",
                            stroke: "rgba(255,255,255,0.15)",
                            stroke_width: "1.5",
                            transform: "rotate({i * 10} 50 50)",
                        }
                    }
                }

                // Domed lens cap
                div { class: "ab-toggle__cap",
                    // Context ring glow
                    div { class: "ab-toggle__ring" }
                    // OLED display face
                    div { class: "ab-toggle__face",
                        div { class: "ab-toggle__label-a",
                            span { class: "ab-toggle__letter", "{top_label}" }
                            span { class: "ab-toggle__sub",    "{top_sub}" }
                        }
                        div { class: "ab-toggle__divider" }
                        div { class: "ab-toggle__label-b",
                            span { class: "ab-toggle__letter", "{bot_label}" }
                            span { class: "ab-toggle__sub",    "{bot_sub}" }
                        }
                    }
                }
            }
        }
    }
}
