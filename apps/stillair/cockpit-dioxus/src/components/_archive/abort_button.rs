//! abort_button.rs — Archived Physical Abort Button
//! 
//! This is the legacy physical flip-cap abort button, complete with the
//! deep cavity, jewel glow, and pa-family red actuator.
//! Archived from Phase C0 UI refactoring.

use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
pub enum PhysicalAbortState {
    IdleClosed,
    Armed,
    Triggered,
    Cooldown,
}

#[derive(Props, Clone, PartialEq)]
pub struct PhysicalAbortProps {
    pub state: PhysicalAbortState,
    pub on_cover_click: EventHandler<MouseEvent>,
    pub on_button_click: EventHandler<MouseEvent>,
}

#[component]
pub fn PhysicalAbortButton(props: PhysicalAbortProps) -> Element {
    rsx! {
        // ABORT — Flip-Guard Cap + Deep Cavity + PA-Family Red Actuator
        div { class: "abort-housing",
            div {
                class: match props.state {
                    PhysicalAbortState::IdleClosed => "abort-column",
                    PhysicalAbortState::Armed      => "abort-column armed",
                    PhysicalAbortState::Triggered  => "abort-column triggered",
                    PhysicalAbortState::Cooldown   => "abort-column cooldown",
                },

                // Layer 1: Flip cap
                div {
                    class: "abort-cover",
                    onclick: move |e| props.on_cover_click.call(e),
                    div { class: "abort-cover-frame" }
                }

                // Layer 2: Deep cavity — PA-family red actuator inside
                div { class: "abort-cavity",
                    div { class: "abort-inner-socket",
                        button {
                            class: "abort-inner-button",
                            style: if props.state != PhysicalAbortState::Armed { "pointer-events: none;" } else { "" },
                            onclick: move |e| props.on_button_click.call(e),
                            div { class: "abort-jewel-glow" }
                            span { class: "abort-inner-label", "ABORT" }
                        }
                    }
                }
            }
        }
    }
}
