use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct JiniAnalysingProps {
    pub stage: String,
}

#[component]
pub fn JiniAnalysing(props: JiniAnalysingProps) -> Element {
    let text = match props.stage.as_str() {
        "Ingest"      => "Decoding raw audio data.",
        "Scout Pass"  => "Reading the color of your sound.",
        "Stem Engine" => "Isolating structural stems.",
        "Spatial"     => "Mapping the stereo field.",
        "Mastering"   => "Calibrating dynamics. Securing ceiling.",
        "CERTIFIED"   => "Certified.",
        _             => "Analysing.",
    };

    rsx! {
        div {
            style: "width:100%; height:100%; display:flex; align-items:center; justify-content:center;",
            p { class: "jini-text", "{text}" }
        }
    }
}
