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
        OledTileState::Idle   => "oled-tile oled-tile--idle",
        OledTileState::Search => "oled-tile oled-tile--search",
        OledTileState::Lock   => "oled-tile oled-tile--lock",
    };

    // Screen content — dynamic per state
    let screen_label = match props.state {
        OledTileState::Idle | OledTileState::Search => "FM0",
        OledTileState::Lock => "FHQ",
    };

    rsx! {
        // Column wrapper — mirrors dsp-slot-col structure exactly
        div { class: "oled-tile-col",
            // Slot identifier label — always "FM0", mirrors transport-top-label
            span { class: "oled-tile__label", "FM0" }
            // OLED screen module
            div { class: "{tile_class}",
                span { class: "oled-tile__text", "{screen_label}" }
                div { class: "oled-tile__bar" }
            }
        }
    }
}
