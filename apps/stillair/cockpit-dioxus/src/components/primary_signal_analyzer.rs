use dioxus::prelude::*;


#[component]
pub fn PrimarySignalAnalyzer() -> Element {
    // Two static waveform lines as requested
    // A: blue #00b0cc, B: pink #b83488
    let wave_a = "M 0,50 L 10,20 L 20,80 L 30,30 L 40,70 L 50,10 L 60,90 L 70,40 L 80,60 L 90,20 L 100,50";
    let wave_b = "M 0,50 L 10,80 L 20,20 L 30,70 L 40,30 L 50,90 L 60,10 L 70,60 L 80,40 L 90,80 L 100,50";

    rsx! {
        div { class: "psa-panel",

                div {
                class: "primary-analyzer-body",
                style: "flex: 1; display: flex; flex-direction: column; background: #03060a; padding: 12px; gap: 8px;",

                // Top Header (Match mockup text styling hints if possible)
                div {
                    style: "display: flex; justify-content: space-between; align-items: center;",
                    span {
                        style: "font-family: var(--cond); font-size: 11px; color: var(--txt3); letter-spacing: 0.05em;",
                        "Electric Blue #00A8FF average waveform  ·  A/B"
                    }
                    span {
                        style: "font-family: var(--mono); font-size: 11px; color: var(--hud2);",
                        "−11.2 LUFS / −0.8 dBTP"
                    }
                }

                // Waveform Display Area — Split vertically 50/50
                div {
                    class: "oled-screen",
                    style: "flex: 1; display: flex; flex-direction: column; border: 1px solid var(--b2); border-radius: 4px; overflow: hidden;",
                    
                    // PRE Waveform
                    div {
                        style: "flex: 1; position: relative; border-bottom: 1px solid var(--b2);",
                        svg {
                            view_box: "0 0 100 100", preserve_aspect_ratio: "none", style: "position: absolute; inset: 0; width: 100%; height: 100%;",
                            path { d: "{wave_a}", fill: "none", stroke: "#00b0cc", stroke_width: "1.5", stroke_linejoin: "round" }
                        }
                        span {
                            style: "position: absolute; top: 6px; left: 10px; font-family: var(--mono); font-size: 10px; color: #00b0cc;",
                            "Pre"
                        }
                    }

                    // POST Waveform
                    div {
                        style: "flex: 1; position: relative;",
                        svg {
                            view_box: "0 0 100 100", preserve_aspect_ratio: "none", style: "position: absolute; inset: 0; width: 100%; height: 100%;",
                            path { d: "{wave_b}", fill: "none", stroke: "#b83488", stroke_width: "1.5", stroke_linejoin: "round" }
                        }
                        span {
                            style: "position: absolute; top: 6px; left: 10px; font-family: var(--mono); font-size: 10px; color: #b83488;",
                            "Post"
                        }
                    }
                }

                // Bottom Footer
                div {
                    style: "display: flex; padding-top: 2px;",
                    span {
                        style: "font-family: var(--cond); font-size: 10px; color: var(--txt2); letter-spacing: 0.05em;",
                        "Sample Rate: 48 kHz | Bit Depth: 24-bit"
                    }
                }
            }
        }
    }
}
