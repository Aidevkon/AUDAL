//! panels/insights.rs — Insights Panel (FM5 metrics display) · P11-006
//! Authority: Phase 11 task-decomposition P11-006 · design-tokens-v1.0.md
//!
//! THE INSIGHTS panel (center MFD):
//!   FM5: LUFS, True Peak, LRA, dynamic range, compliance table
//!   A-003 §3: SpectrumAnalyzer = static placeholder (no canvas)

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{ComplianceJson, SessionStateJson};

#[component]
pub fn InsightsPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let state = session_state.read();

    rsx! {
        div {
            class: "mfd-panel panel-insights",
            style: "border-right:1px solid var(--border-subtle); display:flex; flex-direction:column; overflow:hidden;",

            // Panel title bar
            div {
                class: "panel-title",
                style: "color:var(--accent-insights);
                        border-bottom:2px solid var(--accent-insights);
                        padding:1rem 1.5rem 0.5rem;
                        font-size:0.7rem; letter-spacing:0.2em;
                        text-transform:uppercase; font-weight:600;
                        flex-shrink:0;",
                "THE INSIGHTS"
            }

            div {
                style: "flex:1; overflow-y:auto;",
                match state.as_ref() {
                    Some(s) => rsx! {
                        // ── Loudness metrics ─────────────────────────────────
                        SectionLabel { label: "LOUDNESS" }
                        MetricRow {
                            label: "INTEGRATED",
                            value: format!("{:.1} LUFS", s.loudness.integrated_lufs),
                            accent: s.loudness.spotify_compliant,
                        }
                        MetricRow {
                            label: "TRUE PEAK",
                            value: format!("{:.1} dBTP", s.loudness.true_peak_dbtp),
                            accent: s.loudness.true_peak_dbtp <= -1.0,
                        }
                        MetricRow {
                            label: "LOUDNESS RANGE",
                            value: format!("{:.1} LU", s.loudness.lra),
                            accent: true,
                        }
                        MetricRow {
                            label: "SHORT TERM",
                            value: format!("{:.1} LUFS", s.loudness.short_term_lufs),
                            accent: true,
                        }

                        // ── Quality metrics ──────────────────────────────────
                        SectionLabel { label: "QUALITY" }
                        MetricRow {
                            label: "STEREO CORR",
                            value: format!("{:.2}", s.quality.stereo_correlation),
                            accent: s.quality.stereo_correlation >= 0.85,
                        }
                        MetricRow {
                            label: "DYNAMIC RANGE",
                            value: format!("{:.1} dB", s.quality.dynamic_range_db),
                            accent: s.quality.dynamic_range_db >= 8.0,
                        }
                        MetricRow {
                            label: "CLIP FREE",
                            value: if s.quality.clip_free { "✓ CLEAN".into() } else {
                                format!("{} CLIPS", s.quality.clips_detected)
                            },
                            accent: s.quality.clip_free,
                        }

                        // ── Compliance table ─────────────────────────────────
                        SectionLabel { label: "PLATFORM COMPLIANCE" }
                        ComplianceTable { compliance: s.compliance.clone() }

                        // ── Spectrum placeholder (A-003 §3) ─────────────────
                        SectionLabel { label: "SPECTRUM" }
                        SpectrumPlaceholder {}
                    },
                    None => rsx! {
                        div {
                            style: "color:var(--text-muted); padding:2rem;
                                    text-align:center; font-size:0.75rem;
                                    letter-spacing:0.1em;",
                            "AWAITING MASTERING..."
                        }
                    }
                }
            }
        }
    }
}

// ── Sub-components ─────────────────────────────────────────────────────────────

#[component]
fn SectionLabel(label: &'static str) -> Element {
    rsx! {
        div {
            style: "padding:0.75rem 1.5rem 0.25rem;
                    color:var(--text-muted); font-size:0.6rem;
                    letter-spacing:0.2em; text-transform:uppercase;
                    border-top:1px solid var(--border-subtle);",
            "{label}"
        }
    }
}

#[component]
fn MetricRow(label: &'static str, value: String, accent: bool) -> Element {
    let color = if accent { "var(--text-primary)" } else { "var(--status-warn)" };
    rsx! {
        div {
            style: "display:flex; justify-content:space-between; align-items:baseline;
                    padding:0.4rem 1.5rem; border-bottom:1px solid rgba(255,255,255,0.03);",
            div {
                style: "color:var(--text-muted); font-size:0.65rem; letter-spacing:0.1em;
                        text-transform:uppercase;",
                "{label}"
            }
            div {
                style: "color:{color}; font-size:0.85rem; font-weight:500;
                        font-variant-numeric:tabular-nums;",
                "{value}"
            }
        }
    }
}

#[component]
fn ComplianceTable(compliance: ComplianceJson) -> Element {
    let platforms = [
        ("Spotify",   compliance.spotify),
        ("YouTube",   compliance.youtube),
        ("Apple",     compliance.apple),
        ("Tidal",     compliance.tidal),
        ("Broadcast", compliance.broadcast),
        ("EBU R128",  compliance.ebu_r128),
    ];

    rsx! {
        div {
            style: "padding:0.5rem 1.5rem;",
            div {
                style: "display:grid; grid-template-columns:1fr 1fr;
                        gap:0.3rem;",
                for (name, ok) in platforms {
                    div {
                        key: "{name}",
                        style: "display:flex; align-items:center; gap:0.4rem;
                                padding:0.3rem 0;",
                        div {
                            style: if ok {
                                "width:6px; height:6px; border-radius:50%;
                                 background:var(--status-ok); flex-shrink:0;"
                            } else {
                                "width:6px; height:6px; border-radius:50%;
                                 background:var(--status-err); flex-shrink:0;"
                            },
                        }
                        div {
                            style: "font-size:0.65rem; color:var(--text-secondary);",
                            "{name}"
                        }
                    }
                }
            }
        }
    }
}

/// SpectrumAnalyzer placeholder — A-003 §3: no canvas, no live audio in Phase 11.
#[component]
fn SpectrumPlaceholder() -> Element {
    rsx! {
        div {
            style: "margin:0.5rem 1.5rem 1rem;
                    border:1px solid var(--border-subtle); border-radius:4px;
                    padding:1.5rem; text-align:center;",
            div {
                style: "display:flex; gap:2px; align-items:flex-end;
                        justify-content:center; height:48px; margin-bottom:0.5rem;",
                for h in [30, 60, 80, 95, 70, 55, 40, 65, 85, 75, 50, 35] {
                    div {
                        key: "{h}",
                        style: "width:4px; height:{h}%; background:var(--accent-insights);
                                opacity:0.3; border-radius:1px 1px 0 0;",
                    }
                }
            }
            div {
                style: "color:var(--text-muted); font-size:0.6rem; letter-spacing:0.1em;",
                "SPECTRUM · PHASE 12"
            }
        }
    }
}
