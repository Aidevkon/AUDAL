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

    let label = match props.state {
        OledTileState::Idle | OledTileState::Search => "FM0",
        OledTileState::Lock => "FHQ",
    };

    rsx! {
        div { class: "{tile_class}",
            // Substrate — true black OLED base
            div { class: "oled-tile__substrate",
                // Glass reflection layer handled via ::before in CSS
                // Emissive text
                span { class: "oled-tile__text", "{label}" }
                // Status bar — ping-pong (Search) or underline (Lock)
                div { class: "oled-tile__bar" }
            }
        }
    }
}
