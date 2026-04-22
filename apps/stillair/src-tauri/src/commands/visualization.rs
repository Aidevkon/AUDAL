//! commands/visualization.rs — Tauri command: get_visualization_data
//! Authority: Phase 14 P14-001 · UI Agent Context v2.1 §2
//!
//! Computes ALL visualization data in the backend. The UI receives strings and
//! f32 values — it renders only. Zero computation in the Cockpit.
//!
//! VisualizationDataJson:
//!   spectrum_svg_path   — gaussian SVG curve, 400×200 viewBox
//!   lissajous_outer_*   — stereo width ellipse parameters
//!   lissajous_inner_*   — correlation tightness ellipse parameters
//!   waveform_before_svg — placeholder waveform path (Phase 15: real PCM)
//!   waveform_after_svg  — placeholder waveform path (Phase 15: real PCM)
//!
//! All math uses libm — no std::f32 methods (UI Agent Context §1).

use crate::ipc::m0_client::M0Client;

/// Precomputed visualization data delivered to the Cockpit.
/// UI renders from these values — computes nothing itself.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VisualizationDataJson {
    /// Spectrum waveform — SVG path string, 400×200 viewBox.
    /// Closed fill path: M x,y L ... L 390,190 L 10,190 Z
    pub spectrum_svg_path:   String,

    /// Lissajous outer ellipse rx/ry — derived from stereo_width.
    pub lissajous_outer_rx:  f32,
    pub lissajous_outer_ry:  f32,

    /// Lissajous inner ellipse rx/ry — derived from stereo_correlation.
    pub lissajous_inner_rx:  f32,
    pub lissajous_inner_ry:  f32,

    /// Waveform placeholders — Phase 15: real before/after from PCM cache.
    pub waveform_before_svg: String,
    pub waveform_after_svg:  String,
}

/// Tauri command: get_visualization_data
///
/// Fetches Golden Blob metrics from M0, computes all SVG paths and
/// ellipse parameters, returns VisualizationDataJson.
///
/// UI Agent Context rule: only this command produces visualization data.
/// No computation in Dioxus components.
#[tauri::command]
pub async fn get_visualization_data(
    blob_id: String,
) -> Result<VisualizationDataJson, String> {
    let client = M0Client::new();
    let blob   = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // ── Spectrum path ─────────────────────────────────────────────────────────
    let spectrum_path = compute_spectrum_path(
        blob.quality.spectral_centroid,
        blob.quality.spectral_flatness,
    );

    // ── Lissajous ellipse parameters ──────────────────────────────────────────
    // Outer: stereo width → rx. Larger width = wider ellipse.
    let outer_rx = (blob.quality.stereo_width * 45.0 + 5.0).clamp(5.0, 50.0);
    let outer_ry = 50.0_f32;
    // Inner: correlation abs → rx (tight correlation = narrow). ry = looseness.
    let inner_rx = (libm::fabsf(blob.quality.stereo_correlation) * 30.0 + 5.0)
        .clamp(3.0, 35.0);
    let inner_ry = ((1.0_f32 - libm::fabsf(blob.quality.stereo_correlation)) * 25.0 + 3.0)
        .clamp(3.0, 28.0);

    // ── Waveform placeholders ─────────────────────────────────────────────────
    let before_path = compute_waveform_placeholder(blob.quality.rms_db, false);
    let after_path  = compute_waveform_placeholder(blob.loudness.integrated_lufs, true);

    eprintln!("[visualization] getVisualizationData: blob_id={}", blob.id);

    Ok(VisualizationDataJson {
        spectrum_svg_path:   spectrum_path,
        lissajous_outer_rx:  outer_rx,
        lissajous_outer_ry:  outer_ry,
        lissajous_inner_rx:  inner_rx,
        lissajous_inner_ry:  inner_ry,
        waveform_before_svg: before_path,
        waveform_after_svg:  after_path,
    })
}

/// Gaussian spectrum curve from spectral features.
///
/// 64 points across 400px width. Center driven by spectral_centroid.
/// Width driven by spectral_flatness. Flatness also drives texture detail.
///
/// Output: closed SVG path string for gradient fill rendering.
/// All math: libm — no std::f32 methods (UI Agent Context §1).
fn compute_spectrum_path(centroid: f32, flatness: f32) -> String {
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(64);

    // Map spectral centroid (0–20kHz) to screen x coordinate (10–390)
    let center_x = (centroid / 20000.0 * 380.0 + 10.0).clamp(10.0, 390.0);

    for i in 0usize..64 {
        let x = i as f32 * 400.0 / 63.0;
        // Gaussian: broader = flatter spectrum (higher flatness → wider peak)
        let sigma = flatness * 80.0 + 40.0;
        let distance = (x - center_x) / sigma;
        let base = libm::expf(-distance * distance * 0.5_f32);
        // Flatness-derived texture: ripple at high end for bright/flat spectra
        let detail = flatness * 0.25_f32 * libm::sinf(x * 0.25_f32) * libm::expf(-x / 250.0_f32);
        let y = 180.0_f32 - (base + detail).clamp(0.0_f32, 1.0_f32) * 160.0_f32;
        points.push((x, y));
    }

    // Build closed fill path: curve + bottom baseline closure
    let mut path = format!("M {:.1},{:.1}", points[0].0, points[0].1);
    for (x, y) in &points[1..] {
        path.push_str(&format!(" L {:.1},{:.1}", x, y));
    }
    path.push_str(" L 390,190 L 10,190 Z");
    path
}

/// Placeholder waveform path — sine envelope from loudness level.
///
/// Phase 15 will replace with real before/after PCM snapshots (A-003 §11).
/// For Phase 14: visual stand-in that responds to actual loudness metrics.
///
/// All math: libm — no std::f32 methods.
fn compute_waveform_placeholder(lufs_or_rms: f32, is_mastered: bool) -> String {
    // Map loudness to amplitude — louder signal = taller waveform
    let amplitude = ((-lufs_or_rms / 30.0_f32) * 50.0_f32).clamp(5.0_f32, 55.0_f32);
    let freq = if is_mastered { 0.04_f32 } else { 0.03_f32 };

    let mut path = String::from("M 0,60");
    for i in 1usize..=800 {
        let x = i as f32;
        // Slow amplitude envelope 0→1→0 over the 800px width
        let envelope = libm::sinf(x * 0.002_f32) * 0.5_f32 + 0.5_f32;
        let y = 60.0_f32 - libm::sinf(x * freq) * amplitude * envelope;
        path.push_str(&format!(" L {:.0},{:.1}", x, y));
    }
    path
}
