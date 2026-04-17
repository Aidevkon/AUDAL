//! panels/insights.rs — Insights Panel · Phase 12B
//! Authority: Phase 12B P12B-006 · P12B-007 · design-tokens-v1.0.md
//!
//! THE INSIGHTS panel (center MFD):
//!   FM5: LUFS, True Peak, LRA + live LUFS during playback
//!   Spectrum analyzer — 16 real bars from spectral_centroid + flatness
//!   Correlation radar — lissajous orbital SVG from stereo_correlation + width
//!   LUFS/PEAK vertical meter bars
//!
//! A-003 §5: No PCM. No audio kernel imports. PlaybackStateJson metrics only.

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::types::{ComplianceJson, PlaybackStateJson, SessionStateJson};

#[component]
pub fn InsightsPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
    playback_state: Signal<Option<PlaybackStateJson>>,
) -> Element {
    let state    = session_state.read();
    let playback = playback_state.read();

    rsx! {
        div {
            class: "mfd-panel panel-insights",

            // Panel title bar
            div {
                class: "panel-title",
                style: "color:var(--accent-insights);",
                "THE INSIGHTS"
            }

            div {
                style: "flex:1; overflow-y:auto;",
                match state.as_ref() {
                    Some(s) => rsx! {

                        // ── Live LUFS + Peak meters ───────────────────────────
                        div {
                            style: "display:flex; gap:var(--space-4); align-items:flex-start;\
                                    padding:var(--space-2) var(--space-4) var(--space-1);",

                            // Big integrated LUFS readout
                            div {
                                style: "flex:1;",
                                div {
                                    style: "color:var(--text-muted); font-size:0.55rem;\
                                            letter-spacing:var(--tracking-widest);\
                                            text-transform:uppercase; margin-bottom:2px;",
                                    "LUFS"
                                }
                                div {
                                    class: "lufs-readout",
                                    { format!("{:.1}", s.loudness.integrated_lufs) }
                                }
                                div {
                                    style: "color:var(--text-muted); font-size:0.55rem;\
                                            letter-spacing:var(--tracking-widest);\
                                            text-transform:uppercase;",
                                    "INTEGRATED"
                                }
                            }

                            // LUFS + PEAK vertical meter bars
                            div {
                                class: "lufs-meters",

                                // LUFS bar (integrated)
                                LufsBar {
                                    label: "LUFS",
                                    value: s.loudness.integrated_lufs,
                                    // map LUFS -30..0 → 0..100%
                                    fill_pct: ((s.loudness.integrated_lufs + 30.0) / 30.0 * 100.0)
                                        .clamp(0.0, 100.0),
                                }

                                // True Peak bar
                                LufsBar {
                                    label: "PEAK",
                                    value: s.loudness.true_peak_dbtp,
                                    fill_pct: ((s.loudness.true_peak_dbtp + 30.0) / 30.0 * 100.0)
                                        .clamp(0.0, 100.0),
                                }
                            }
                        }

                        // ── Spectrum analyzer (real, Phase 12B) ───────────────
                        SectionLabel { label: "SPECTRUM" }
                        SpectrumAnalyzer {
                            centroid: s.quality.spectral_centroid,
                            flatness:  s.quality.spectral_flatness,
                        }

                        // ── Correlation / Lissajous radar ─────────────────────
                        SectionLabel { label: "STEREO FIELD" }
                        CorrelationRadar {
                            correlation: s.quality.stereo_correlation,
                            width:       s.quality.stereo_width,
                            peak:        s.loudness.true_peak_dbtp,
                            range:       s.loudness.lra,
                        }

                        // ── Loudness metrics ──────────────────────────────────
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

                        // SHORT TERM — live if playback active
                        {
                            let live_lufs = playback.as_ref().filter(|p| p.is_playing)
                                .map(|_| s.loudness.short_term_lufs);
                            rsx! {
                                MetricRow {
                                    label: "SHORT TERM",
                                    value: if let Some(lufs) = live_lufs {
                                        format!("{:.1} LUFS ●", lufs)
                                    } else {
                                        format!("{:.1} LUFS", s.loudness.short_term_lufs)
                                    },
                                    accent: live_lufs.is_some(),
                                }
                            }
                        }

                        // ── Quality metrics ───────────────────────────────────
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

                        // ── Compliance table ──────────────────────────────────
                        SectionLabel { label: "PLATFORM COMPLIANCE" }
                        ComplianceTable { compliance: s.compliance.clone() }
                    },
                    None => rsx! {
                        div {
                            class: "awaiting",
                            "AWAITING MASTERING..."
                        }
                    }
                }
            }
        }
    }
}

// ── Spectrum Analyzer — real bars (P12B-006) ──────────────────────────────────
//
// 16 bars from spectral_centroid (which band peaks) + flatness (how spread).
// centroid: Hz value e.g. 3200 → maps to band index 0..15
// flatness: 0.0 = tonal spike, 1.0 = flat noise
// Bar heights: gaussian around centroid band, scaled by (1-flatness).

#[component]
fn SpectrumAnalyzer(centroid: f32, flatness: f32) -> Element {
    let bars: Vec<u8> = (0..16usize).map(|i| compute_bar(i, centroid, flatness)).collect();

    rsx! {
        div {
            class: "spectrum-analyzer",
            div {
                class: "spectrum-bars-live",
                for (i, h) in bars.iter().enumerate() {
                    div {
                        key: "{i}",
                        class: "spectrum-bar-live",
                        style: format!("height:{}%", h),
                    }
                }
            }
        }
    }
}

/// Compute height (0–95%) for band i given spectral centroid + flatness.
fn compute_bar(band: usize, centroid: f32, flatness: f32) -> u8 {
    // Map centroid Hz (20..20000) to band index (0..15)
    let log_centroid = (centroid.max(20.0) / 20.0).log2();
    let log_max      = (20_000.0_f32 / 20.0).log2();
    let center_band  = (log_centroid / log_max * 15.0) as f32;

    let distance = (band as f32 - center_band).abs();

    // Gaussian peak at centroid band, width controlled by flatness
    let sigma   = 1.5 + flatness * 4.0;
    let peak    = (-distance * distance / (2.0 * sigma * sigma)).exp();
    let flat    = flatness * 0.5;
    let height  = ((1.0 - flatness) * peak + flat) * 90.0 + 5.0;

    height.clamp(5.0, 95.0) as u8
}

// ── Correlation / Lissajous Radar — SVG (P12B-007) ───────────────────────────
//
// Renders a lissajous orbital figure (like the hardware reference image):
//   - Cyan inner orbit from stereo_correlation
//   - Magenta/pink outer orbit from stereo_width
//   - Oval shape on 45° axis — more squashed = less correlated
// No WebGL, no canvas — SVG polyline (Phase 12B constraint).

#[component]
fn CorrelationRadar(
    correlation: f32,
    width:       f32,
    peak:        f32,
    range:       f32,
) -> Element {
    // Lissajous path: parametric ellipse on 45° axis
    // rx: horizontal extent (width), ry: vertical extent (1-|corr|)
    let cx  = 60.0_f32;
    let cy  = 60.0_f32;
    let rx  = (width * 38.0 + 6.0).clamp(4.0, 50.0);
    let ry  = ((1.0 - correlation.abs()) * 28.0 + 4.0).clamp(4.0, 46.0);

    // Generate parametric lissajous points (rotated 45°)
    let n_pts = 72;
    let liss_1: Vec<String> = (0..=n_pts).map(|i| {
        let t  = std::f32::consts::TAU * (i as f32 / n_pts as f32);
        let x0 = rx * t.cos();
        let y0 = ry * t.sin();
        // Rotate 45°
        let cos45 = std::f32::consts::FRAC_1_SQRT_2;
        let x  = cx + (x0 * cos45 - y0 * cos45);
        let y  = cy + (x0 * cos45 + y0 * cos45);
        format!("{:.1},{:.1}", x, y)
    }).collect();

    // Second orbit — slightly larger, opposite wound
    let rx2 = rx * 0.65;
    let ry2 = ry * 0.65;
    let liss_2: Vec<String> = (0..=n_pts).map(|i| {
        let t  = std::f32::consts::TAU * (i as f32 / n_pts as f32);
        let x0 = rx2 * t.cos();
        let y0 = -ry2 * t.sin();  // opposite wind → figure-eight feel
        let cos45 = std::f32::consts::FRAC_1_SQRT_2;
        let x  = cx + (x0 * cos45 - y0 * cos45);
        let y  = cy + (x0 * cos45 + y0 * cos45);
        format!("{:.1},{:.1}", x, y)
    }).collect();

    let pts_1 = liss_1.join(" ");
    let pts_2 = liss_2.join(" ");

    let corr_str  = format!("{:+.2}", correlation);
    let corr_class = if correlation > 0.3 { "positive" } else { "warn" };

    rsx! {
        div {
            class: "radar-display",
            style: "flex-direction:row; gap:var(--space-4); justify-content:flex-start;\
                    padding-left:var(--space-4);",

            // SVG lissajous display
            div { class: "radar-svg-wrap",
                svg {
                    width:    "120",
                    height:   "120",
                    view_box: "0 0 120 120",

                    // Background deep-space circle
                    circle {
                        cx: "60", cy: "60", r: "56",
                        fill:         "var(--bg-overlay)",
                        stroke:       "var(--border-subtle)",
                        stroke_width: "1"
                    }
                    // Subtle concentric guide rings
                    circle {
                        cx: "60", cy: "60", r: "38",
                        fill: "none",
                        stroke: "rgba(0,209,255,0.06)",
                        stroke_width: "1"
                    }
                    circle {
                        cx: "60", cy: "60", r: "20",
                        fill: "none",
                        stroke: "rgba(0,209,255,0.06)",
                        stroke_width: "1"
                    }
                    // Crosshair axis lines
                    line {
                        x1: "4",  y1: "60",
                        x2: "116", y2: "60",
                        stroke: "rgba(0,209,255,0.12)",
                        stroke_width: "1"
                    }
                    line {
                        x1: "60", y1: "4",
                        x2: "60", y2: "116",
                        stroke: "rgba(0,209,255,0.12)",
                        stroke_width: "1"
                    }
                    // 45° axis guides
                    line {
                        x1: "14", y1: "14", x2: "106", y2: "106",
                        stroke: "rgba(0,209,255,0.06)",
                        stroke_width: "1"
                    }
                    line {
                        x1: "106", y1: "14", x2: "14", y2: "106",
                        stroke: "rgba(0,209,255,0.06)",
                        stroke_width: "1"
                    }

                    // Outer lissajous — cyan (stereo width)
                    polyline {
                        class:        "lissajous-path",
                        points:       "{pts_1}",
                        fill:         "none",
                        stroke:       "var(--accent-insights)",
                        stroke_width: "1.5",
                        opacity:      "0.85"
                    }

                    // Inner lissajous — magenta (correlation tightness)
                    polyline {
                        class:        "lissajous-path-2",
                        points:       "{pts_2}",
                        fill:         "none",
                        stroke:       "#e879f9",
                        stroke_width: "1.2",
                        opacity:      "0.7"
                    }

                    // Center dot
                    circle {
                        cx: "60", cy: "60", r: "2",
                        fill: "var(--accent-insights)"
                    }
                }
            }

            // Numerical readouts: PEAK / RANGE / CORR
            div { class: "radar-readouts",
                div { class: "radar-readout-item",
                    div { class: "radar-readout-label", "PEAK" }
                    div { class: "radar-readout-value", { format!("{:.1}", peak) } }
                }
                div { class: "radar-readout-item",
                    div { class: "radar-readout-label", "RANGE" }
                    div { class: "radar-readout-value", { format!("{:.0}", range) } }
                }
                div { class: "radar-readout-item",
                    div { class: "radar-readout-label", "CORR" }
                    div {
                        class: format!("radar-readout-value {corr_class}"),
                        "{corr_str}"
                    }
                }
            }
        }
    }
}

// ── LUFS vertical bar meter ───────────────────────────────────────────────────

#[component]
fn LufsBar(label: &'static str, value: f32, fill_pct: f32) -> Element {
    rsx! {
        div { class: "lufs-meter",
            div { class: "lufs-meter-label", "{label}" }
            div { class: "lufs-meter-bar-wrap",
                div {
                    class: "lufs-meter-fill",
                    style: format!("height:{}%", fill_pct.clamp(0.0, 100.0)),
                }
            }
            div { class: "lufs-meter-value", { format!("{:.0}", value) } }
        }
    }
}

// ── Sub-components ────────────────────────────────────────────────────────────

#[component]
fn SectionLabel(label: &'static str) -> Element {
    rsx! {
        div {
            class: "section-label",
            "{label}"
        }
    }
}

#[component]
fn MetricRow(label: &'static str, value: String, accent: bool) -> Element {
    let color = if accent { "var(--text-primary)" } else { "var(--status-warn)" };
    rsx! {
        div { class: "metric-row",
            div { class: "metric-label", "{label}" }
            div {
                class: "metric-value",
                style: "color:{color};",
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
        div { class: "compliance-grid",
            for (name, ok) in platforms {
                div {
                    key: "{name}",
                    class: "compliance-row",
                    div {
                        class: if ok { "compliance-dot ok" } else { "compliance-dot err" },
                    }
                    div { class: "compliance-name", "{name}" }
                }
            }
        }
    }
}
