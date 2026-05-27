use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

#[derive(Props, Clone, PartialEq)]
pub struct PdfPreviewProps {
    pub blob_id:    String,
    pub on_close:   EventHandler<()>,
}

#[component]
pub fn PdfPreviewModal(props: PdfPreviewProps) -> Element {
    let mut pdf_data  = use_signal(|| Option::<String>::None);
    let mut loading   = use_signal(|| true);
    let mut error_msg = use_signal(|| Option::<String>::None);
    let blob_id       = props.blob_id.clone();

    // Load PDF on mount
    use_effect(move || {
        let bid = blob_id.clone();
        spawn_local(async move {
            match crate::ipc::invoke::<String, _>(
                "preview_pdf_report",
                serde_json::json!({ "blobId": bid }),
            ).await {
                Ok(b64) => {
                    pdf_data.set(Some(b64));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    loading.set(false);
                }
            }
        });
    });

    let blob_id_dl = props.blob_id.clone();

    rsx! {
        // Backdrop
        div {
            style: "position:fixed; top:0; left:0; width:100vw;
                    height:100vh; background:rgba(0,0,0,0.85);
                    z-index:1000; display:flex; flex-direction:column;
                    align-items:center; justify-content:center;",

            // Modal container
            div {
                style: "background:var(--surface-panel);
                        border:1px solid var(--border-subtle);
                        border-radius:4px; width:680px; height:85vh;
                        display:flex; flex-direction:column;
                        overflow:hidden;",

                // Header bar
                div {
                    style: "display:flex; align-items:center;
                            justify-content:space-between;
                            padding:0.75rem 1rem;
                            border-bottom:1px solid var(--border-subtle);
                            background:var(--surface-raised);",

                    span {
                        style: "color:var(--text-primary); font-size:0.75rem;
                                letter-spacing:0.15em; text-transform:uppercase;
                                font-weight:600;",
                        "BMR-128 COMPLIANCE REPORT"
                    }

                    div {
                        style: "display:flex; gap:0.5rem;",

                        // Download button
                        button {
                            style: "background:var(--accent-amber);
                                    color:#000; border:none;
                                    border-radius:3px; padding:0.4rem 1rem;
                                    font-size:0.7rem; letter-spacing:0.1em;
                                    text-transform:uppercase; cursor:pointer;
                                    font-weight:700;",
                            onclick: move |_| {
                                let bid = blob_id_dl.clone();
                                spawn_local(async move {
                                    let _ = crate::ipc::invoke::<String, _>(
                                        "export_pdf_report",
                                        serde_json::json!({ "blobId": bid }),
                                    ).await;
                                });
                            },
                            "DOWNLOAD"
                        }

                        // Close button
                        button {
                            style: "background:transparent;
                                    color:var(--text-muted);
                                    border:1px solid var(--border-subtle);
                                    border-radius:3px; padding:0.4rem 0.75rem;
                                    font-size:0.7rem; cursor:pointer;",
                            onclick: move |_| props.on_close.call(()),
                            "✕"
                        }
                    }
                }

                // PDF viewer area
                div {
                    style: "flex:1; overflow:hidden; padding:0;",

                    if *loading.read() {
                        div {
                            style: "display:flex; align-items:center;
                                    justify-content:center; height:100%;
                                    color:var(--text-muted); font-size:0.8rem;
                                    letter-spacing:0.1em;",
                            "LOADING REPORT..."
                        }
                    } else if let Some(err) = error_msg.read().as_ref() {
                        div {
                            style: "display:flex; align-items:center;
                                    justify-content:center; height:100%;
                                    color:#ff3b30; font-size:0.8rem;",
                            "ERROR: {err}"
                        }
                    } else if let Some(b64) = pdf_data.read().as_ref() {
                        iframe {
                            style: "width:100%; height:100%; border:none;",
                            src: "data:application/pdf;base64,{b64}",
                        }
                    }
                }
            }
        }
    }
}
