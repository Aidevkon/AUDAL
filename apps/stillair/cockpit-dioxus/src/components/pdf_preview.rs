//! Certificate Modal — Creator OS Mastering Certificate
//! Two themes: dark (in-app) + light (export PNG)
//! Triggered via pdf_preview_ctx signal (blob_id)

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;
use crate::ipc::invoke;

#[derive(Props, Clone, PartialEq)]
pub struct PdfPreviewProps {
    pub blob_id:  String,
    pub on_close: EventHandler<()>,
}

#[derive(Clone, Debug)]
struct CertData {
    filename:   String,
    lufs:       f32,
    tp:         f32,
    lra:        f32,
    blob_short: String,
    date:       String,
    qr_base64:  String,
    voice:      String,
    drums:      String,
    bass:       String,
    harmonics:  String,
    ambience:   String,
    pipeline:   String,
}

#[component]
pub fn PdfPreviewModal(props: PdfPreviewProps) -> Element {
    let mut cert     = use_signal(|| Option::<CertData>::None);
    let mut loading  = use_signal(|| true);
    let mut err_msg  = use_signal(|| Option::<String>::None);
    let blob_id      = props.blob_id.clone();

    use_effect(move || {
        let bid = blob_id.clone();
        spawn_local(async move {
            match invoke::<serde_json::Value, _>(
                "get_golden_blob",
                serde_json::json!({ "blobId": bid }),
            ).await {
                Ok(blob) => {
                    let fp = blob["stem_fingerprints"].as_object();
                    let short = |s: &str| s.chars().take(8).collect::<String>();
                    let data = CertData {
                        filename:   blob["id"].as_str()
                                        .map(|s| format!("CERT-{}", &s[..8]))
                                        .unwrap_or_else(|| "UNKNOWN".to_string()),
                        lufs:       blob["loudness"]["integrated_lufs"].as_f64().unwrap_or(0.0) as f32,
                        tp:         blob["loudness"]["true_peak_dbtp"].as_f64().unwrap_or(0.0) as f32,
                        lra:        blob["loudness"]["lra"].as_f64().unwrap_or(0.0) as f32,
                        blob_short: bid.chars().take(8).collect(),
                        date:       chrono_now(),
                        qr_base64:  blob["qr_base64"].as_str().unwrap_or("").to_string(),
                        voice:      fp.and_then(|f| f["voice"].as_str()).map(short).unwrap_or_default(),
                        drums:      fp.and_then(|f| f["drums"].as_str()).map(short).unwrap_or_default(),
                        bass:       fp.and_then(|f| f["bass"].as_str()).map(short).unwrap_or_default(),
                        harmonics:  fp.and_then(|f| f["harmonics"].as_str()).map(short).unwrap_or_default(),
                        ambience:   fp.and_then(|f| f["ambience"].as_str()).map(short).unwrap_or_default(),
                        pipeline:   fp.and_then(|f| f["pipeline"].as_str()).map(short).unwrap_or_default(),
                    };
                    cert.set(Some(data));
                    loading.set(false);
                }
                Err(e) => {
                    err_msg.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div {
            style: "position:fixed;top:0;left:0;width:100vw;height:100vh;
                    background:rgba(0,0,0,0.92);z-index:1000;
                    display:flex;align-items:center;justify-content:center;",

            if *loading.read() {
                div { style: "color:#1d9e75;font-family:monospace;letter-spacing:0.2em;",
                    "GENERATING CERTIFICATE..."
                }
            } else if let Some(e) = err_msg.read().as_ref() {
                div { style: "color:#ff3b30;font-family:monospace;",
                    "ERROR: {e}"
                }
            } else if let Some(c) = cert.read().as_ref() {
                div {
                    style: "width:480px;background:#060e14;
                            border:0.5px solid #1d9e75;border-radius:12px;
                            padding:2rem;font-family:'Share Tech Mono',monospace;
                            position:relative;",

                    // Header
                    div { style: "display:flex;justify-content:space-between;align-items:flex-start;margin-bottom:1.5rem;",
                        div {
                            div { style: "font-size:10px;color:#0f6e56;letter-spacing:0.2em;margin-bottom:6px;",
                                "CREATOR OS · MASTERING CERTIFICATE"
                            }
                            div { style: "font-size:20px;font-weight:500;color:#e8f4f0;letter-spacing:0.06em;",
                                "{c.filename}"
                            }
                            div { style: "font-size:11px;color:#3a5a4a;margin-top:4px;",
                                "{c.date} · blob: {c.blob_short}"
                            }
                        }
                        div { style: "display:flex;flex-direction:column;align-items:center;gap:4px;",
                            div { style: "width:40px;height:40px;border-radius:50%;
                                         border:1.5px solid #1d9e75;
                                         display:flex;align-items:center;justify-content:center;",
                                "✓"
                            }
                            div { style: "font-size:9px;color:#1d9e75;letter-spacing:0.1em;", "CERTIFIED" }
                        }
                    }

                    // Metrics
                    div { style: "display:grid;grid-template-columns:1fr 1fr 1fr;gap:8px;margin-bottom:1.25rem;",
                        div { style: "background:#0a1a12;border:0.5px solid #1d2a22;border-radius:8px;padding:12px;",
                            div { style: "font-size:9px;color:#3a5a4a;margin-bottom:6px;letter-spacing:0.12em;", "INTEGRATED" }
                            div { style: "font-size:20px;font-weight:500;color:#1d9e75;", "{c.lufs:.2}" }
                            div { style: "font-size:9px;color:#0f6e56;margin-top:3px;", "LUFS ✓ EBU R128" }
                        }
                        div { style: "background:#0a1a12;border:0.5px solid #1d2a22;border-radius:8px;padding:12px;",
                            div { style: "font-size:9px;color:#3a5a4a;margin-bottom:6px;letter-spacing:0.12em;", "TRUE PEAK" }
                            div { style: "font-size:20px;font-weight:500;color:#5dcaa5;", "{c.tp:.2}" }
                            div { style: "font-size:9px;color:#0f6e56;margin-top:3px;", "dBTP ✓" }
                        }
                        div { style: "background:#0a1a12;border:0.5px solid #1d2a22;border-radius:8px;padding:12px;",
                            div { style: "font-size:9px;color:#3a5a4a;margin-bottom:6px;letter-spacing:0.12em;", "LRA" }
                            div { style: "font-size:20px;font-weight:500;color:#5dcaa5;", "{c.lra:.2}" }
                            div { style: "font-size:9px;color:#0f6e56;margin-top:3px;", "LU" }
                        }
                    }

                    // Stem DNA + QR side by side
                    div { style: "display:grid;grid-template-columns:1fr auto;gap:12px;margin-bottom:1.25rem;",
                        div { style: "background:#0a1a12;border:0.5px solid #1d2a22;border-radius:8px;padding:12px;",
                            div { style: "font-size:9px;color:#3a5a4a;letter-spacing:0.15em;margin-bottom:10px;",
                                "STEM DNA"
                            }
                            div { style: "display:grid;grid-template-columns:1fr 1fr;gap:5px;",
                                div { style: "display:flex;gap:6px;align-items:center;",
                                    span { style: "font-size:9px;color:#0f6e56;width:24px;", "VOC" }
                                    span { style: "font-size:11px;color:#1d9e75;", "{c.voice}" }
                                }
                                div { style: "display:flex;gap:6px;align-items:center;",
                                    span { style: "font-size:9px;color:#0f6e56;width:24px;", "DRM" }
                                    span { style: "font-size:11px;color:#1d9e75;", "{c.drums}" }
                                }
                                div { style: "display:flex;gap:6px;align-items:center;",
                                    span { style: "font-size:9px;color:#0f6e56;width:24px;", "BSS" }
                                    span { style: "font-size:11px;color:#1d9e75;", "{c.bass}" }
                                }
                                div { style: "display:flex;gap:6px;align-items:center;",
                                    span { style: "font-size:9px;color:#0f6e56;width:24px;", "HRM" }
                                    span { style: "font-size:11px;color:#1d9e75;", "{c.harmonics}" }
                                }
                                div { style: "display:flex;gap:6px;align-items:center;",
                                    span { style: "font-size:9px;color:#0f6e56;width:24px;", "AMB" }
                                    span { style: "font-size:11px;color:#1d9e75;", "{c.ambience}" }
                                }
                            }
                            div { style: "border-top:0.5px solid #1d2a22;margin-top:8px;padding-top:8px;display:flex;gap:8px;align-items:center;",
                                span { style: "font-size:9px;color:#3a5a4a;letter-spacing:0.1em;", "PIPELINE" }
                                span { style: "font-size:11px;color:#5dcaa5;", "{c.pipeline}" }
                            }
                        }
                        // QR Code
                        if !c.qr_base64.is_empty() {
                            div { style: "display:flex;flex-direction:column;align-items:center;gap:6px;",
                                img {
                                    style: "width:90px;height:90px;image-rendering:pixelated;border:2px solid #1d2a22;",
                                    src: "data:image/png;base64,{c.qr_base64}",
                                    alt: "Certificate QR"
                                }
                                div { style: "font-size:8px;color:#3a5a4a;text-align:center;line-height:1.4;",
                                    "SCAN TO VERIFY"
                                }
                            }
                        }
                    }

                    // Tagline
                    div { style: "border:0.5px solid #1d2a22;border-radius:8px;padding:10px 14px;
                                  margin-bottom:1.25rem;text-align:center;",
                        div { style: "font-size:11px;color:#3a5a4a;line-height:1.6;",
                            "Your stems. Your machine. Your sound."
                        }
                        div { style: "font-size:10px;color:#0f6e56;margin-top:2px;",
                            "Privacy by architecture · Zero cloud processing"
                        }
                    }

                    // Buttons
                    div { style: "display:grid;grid-template-columns:1fr 1fr;gap:8px;",
                        button {
                            style: "background:#0a1a12;border:0.5px solid #1d9e75;
                                    border-radius:8px;padding:11px;color:#1d9e75;
                                    font-family:monospace;font-size:11px;cursor:pointer;
                                    letter-spacing:0.08em;",
                            onclick: move |_| {
                                let bid = props.blob_id.clone();
                                spawn_local(async move {
                                    let _ = invoke::<String, _>(
                                        "export_pdf_report",
                                        serde_json::json!({ "blobId": bid }),
                                    ).await;
                                });
                            },
                            "↓ EXPORT PNG"
                        }
                        button {
                            style: "background:transparent;border:0.5px solid #1d2a22;
                                    border-radius:8px;padding:11px;color:#3a5a4a;
                                    font-family:monospace;font-size:11px;cursor:pointer;
                                    letter-spacing:0.08em;",
                            onclick: move |_| props.on_close.call(()),
                            "▶ CONTINUE"
                        }
                    }
                }
            }
        }
    }
}

fn chrono_now() -> String {
    // Wasm-compatible date — use JS Date
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        format!("{}-{:02}-{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        "2026-06-05".to_string()
    }
}
