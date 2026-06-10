use dioxus::prelude::*;

#[derive(Clone, PartialEq)]
pub enum OledTileState {
    Idle,   // FM0 — static text, dim
    Search, // FM0 — ping-pong bar below text
    Lock,   // FHQ — static underline, bright
}

#[derive(Props, PartialEq, Clone)]
pub struct OledTileProps {
    pub state: OledTileState,
}

#[component]
pub fn OledTile(props: OledTileProps) -> Element {
    let tile_class = match props.state {
        OledTileState::Idle => "oled-tile oled-tile--idle",
        OledTileState::Search => "oled-tile oled-tile--search",
        OledTileState::Lock => "oled-tile oled-tile--lock",
    };

    let screen_label = match props.state {
        OledTileState::Idle | OledTileState::Search => "FM0",
        OledTileState::Lock => "FHQ",
    };

    let data_state = match props.state {
        OledTileState::Idle => "idle",
        OledTileState::Search => "search",
        OledTileState::Lock => "lock",
    };

    let aria_label = match props.state {
        OledTileState::Idle => "FM0 — idle, no signal",
        OledTileState::Search => "FM0 — searching for signal",
        OledTileState::Lock => "FHQ — signal locked",
    };

    rsx! {
        // Column wrapper — mirrors dsp-slot-col structure exactly
        div {
            class: "oled-tile-col",
            role: "status",
            aria_label: "{aria_label}",
            "data-slot": "fm0",

            // Slot identifier label — always "FM0", mirrors transport-top-label
            span {
                class: "oled-tile__label",
                aria_hidden: "true",
                "FM0"
            }

            // Hardware housing — same material family as dsp-pit
            div {
                class: "oled-tile-housing",
                "data-state": "{data_state}",
                title: "{aria_label}",

                // OLED screen — recessed inside housing
                div { class: "{tile_class}",
                    span { class: "oled-tile__text", "{screen_label}" }
                    div { class: "oled-tile__bar" }
                }
            }
        }
    }
}
