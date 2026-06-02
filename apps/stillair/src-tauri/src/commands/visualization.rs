//! commands/visualization.rs — Tauri command: get_visualization_data
//! Authority: Phase 14 P14-001 · UI Agent Context v2.1 §2
//!
//! LAW: All SVG path strings computed HERE. UI receives String values, renders only.
//!
//! VisualizationDataJson (sent to Cockpit):
//!   spectrum_svg_path      — Gaussian spectrum curve, 400×160 viewBox
//!   lissajous_path_outer   — SVG path string: outer figure-8 orbit (cyan)
//!   lissajous_path_inner   — SVG path string: inner figure-8 orbit (magenta)
//!   lissajous_path_detail1 — Additional trace for visual richness (opacity 0.3)
//!   lissajous_path_detail2 — Additional trace for visual richness (opacity 0.15)
//!   waveform_before_svg    — Phase 15 placeholder waveform
//!   waveform_after_svg     — Phase 15 placeholder waveform
//!
//! All math: libm only. No std::f32 methods. No UI-side computation.

use crate::ipc::m0_client::M0Client;

/// Complete visualization snapshot sent to Cockpit via IPC.
/// UI renders these strings and f32 values — computes nothing itself.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VisualizationDataJson {
    /// Spectrum — SVG path, 400×160 viewBox. Closed fill path for gradient.
    pub spectrum_svg_path:    String,

    /// Lissajous outer orbit — SVG path, 120×120 viewBox. Rendered cyan.
    pub lissajous_path_outer:   String,
    /// Lissajous inner orbit — SVG path, 120×120 viewBox. Rendered magenta.
    pub lissajous_path_inner:   String,
    /// Additional detail traces for visual richness — rendered at low opacity.
    pub lissajous_path_detail1: String,
    pub lissajous_path_detail2: String,

    /// Waveform placeholders — Phase 15 replaces with real PCM snapshots.
    pub waveform_before_svg: String,
    pub waveform_after_svg:  String,
}

/// Tauri command: get_visualization_data
///
/// Fetches Golden Blob metrics from M0, computes ALL SVG paths and visualization
/// data. Returns VisualizationDataJson.
///
/// UI Agent Context §2: Only this command produces visualization data.
/// Zero computation in Dioxus components.
#[tauri::command]
pub async fn get_visualization_data(
    blob_id: String,
    client: tauri::State<'_, M0Client>,
) -> Result<VisualizationDataJson, String> {
    let blob   = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // ── Spectrum ──────────────────────────────────────────────────────────────
    let spectrum_path = compute_spectrum_path(
        blob.quality.spectral_centroid,
        blob.quality.spectral_flatness,
    );

    // ── Lissajous / Goniometer paths ──────────────────────────────────────────
    // Phase ratios driven by stereo correlation and width.
    // a=1, b=2 → figure-8 (standard goniometer shape).
    // Phase offset derived from correlation: high correlation = tight knot.
    let corr   = blob.quality.stereo_correlation.clamp(-1.0, 1.0);
    let width  = blob.quality.stereo_width.clamp(0.0, 1.0);

    // Outer orbit: a=1 b=2, phase = corr-derived offset, radius=stereo_width
    let outer_rx = (width * 44.0 + 6.0).clamp(6.0, 50.0);
    let outer_ry = 50.0_f32;
    let outer_phase = libm::acosf(corr) as f32;  // 0=mono, π/2=wide, π=out-of-phase

    let lissajous_outer   = compute_lissajous_path(outer_rx, outer_ry, 1.0, 2.0, outer_phase, 200);
    let lissajous_inner   = compute_lissajous_path(outer_rx * 0.55, outer_ry * 0.45, 2.0, 3.0, outer_phase * 0.7, 180);
    let lissajous_detail1 = compute_lissajous_path(outer_rx * 0.30, outer_ry * 0.65, 1.0, 3.0, outer_phase + 0.5, 120);
    let lissajous_detail2 = compute_lissajous_path(outer_rx * 0.70, outer_ry * 0.25, 3.0, 2.0, outer_phase * 1.3, 120);

    // ── Waveform placeholders ─────────────────────────────────────────────────
    let before_path = compute_waveform_placeholder(blob.quality.rms_db, false);
    let after_path  = compute_waveform_placeholder(blob.loudness.integrated_lufs, true);

    eprintln!("[visualization] blob_id={} corr={:.2} width={:.2}", blob.id, corr, width);

    Ok(VisualizationDataJson {
        spectrum_svg_path:    spectrum_path,
        lissajous_path_outer:   lissajous_outer,
        lissajous_path_inner:   lissajous_inner,
        lissajous_path_detail1: lissajous_detail1,
        lissajous_path_detail2: lissajous_detail2,
        waveform_before_svg:  before_path,
        waveform_after_svg:   after_path,
    })
}

// ── SVG Math — libm only, no std::f32 methods ─────────────────────────────────

/// Parametric Lissajous curve: x(t) = rx·sin(a·t + phase), y(t) = ry·sin(b·t)
///
/// Centered at (60,60) in 120×120 viewBox. Returns SVG polyline path string.
/// All math: libm::sinf only. n_points = sampling density.
fn compute_lissajous_path(rx: f32, ry: f32, a: f32, b: f32, phase: f32, n_points: usize) -> String {
    let cx = 60.0_f32;
    let cy = 60.0_f32;
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(n_points + 1);

    for i in 0..=n_points {
        // t ∈ [0, 2π] — full period
        let t = i as f32 * 2.0 * 3.141592653589793 / n_points as f32;
        let x = cx + rx * libm::sinf(a * t + phase);
        let y = cy + ry * libm::sinf(b * t);
        points.push((x, y));
    }

    // Build M x,y L x,y L x,y ... (open, not closed — Lissajous loops naturally)
    let mut path = format!("M {:.2},{:.2}", points[0].0, points[0].1);
    for (x, y) in &points[1..] {
        path.push_str(&format!(" L {:.2},{:.2}", x, y));
    }
    path
}

/// Gaussian spectrum curve from spectral features.
///
/// 64 points across 400px width. Centroid determines peak location.
/// Flatness drives curve width and high-frequency texture detail.
///
/// Output: closed SVG path (fill area). viewBox 0 0 400 160.
/// All math: libm — no std::f32 methods.
fn compute_spectrum_path(centroid: f32, flatness: f32) -> String {
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(64);

    // Spectral centroid maps 0–20kHz → x 10–390 (log-ish scale)
    let center_x = (centroid / 20000.0 * 380.0 + 10.0).clamp(10.0, 390.0);

    for i in 0usize..64 {
        let x = 10.0 + i as f32 * 380.0 / 63.0;
        // Gaussian peak at centroid frequency
        let sigma    = flatness * 80.0 + 40.0;
        let distance = (x - center_x) / sigma;
        let base     = libm::expf(-distance * distance * 0.5_f32);
        // High-frequency texture: ripple driven by spectral flatness
        let detail   = flatness * 0.22_f32
            * libm::sinf(x * 0.3_f32)
            * libm::expf(-x / 300.0_f32);
        // Tilt: natural roll-off at high end (high shelf emulation)
        let rolloff  = libm::expf(-(x - 10.0) * 0.003_f32);
        let height   = ((base + detail) * rolloff).clamp(0.0_f32, 1.0_f32);
        let y        = 148.0_f32 - height * 132.0_f32;
        points.push((x, y));
    }

    // Closed fill path (baseline at y=150)
    let mut path = format!("M {:.1},{:.1}", points[0].0, points[0].1);
    for (x, y) in &points[1..] {
        path.push_str(&format!(" L {:.1},{:.1}", x, y));
    }
    path.push_str(" L 390,150 L 10,150 Z");
    path
}

/// Placeholder waveform path — sine envelope from loudness level.
///
/// Phase 15 replaces with real before/after PCM snapshots (A-003 §11).
/// All math: libm — no std::f32 methods.
fn compute_waveform_placeholder(lufs_or_rms: f32, is_mastered: bool) -> String {
    let amplitude = ((-lufs_or_rms / 30.0_f32) * 50.0_f32).clamp(5.0_f32, 55.0_f32);
    let freq      = if is_mastered { 0.04_f32 } else { 0.03_f32 };

    let mut path = String::from("M 0,60");
    for i in 1usize..=800 {
        let x        = i as f32;
        let envelope = libm::sinf(x * 0.002_f32) * 0.5_f32 + 0.5_f32;
        let y        = 60.0_f32 - libm::sinf(x * freq) * amplitude * envelope;
        path.push_str(&format!(" L {:.0},{:.1}", x, y));
    }
    path
}
