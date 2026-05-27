//! commands/report.rs — Tauri command: export_pdf_report
//! Authority: Phase 13 P13-005 · BMR-128 Compliance Report
//!
//! Generates a BMR-128 PDF compliance report from the Golden Blob metadata.
//! Uses printpdf (pure Rust, MIT) — no LaTeX, no subprocess, no FFI.
//!
//! PDF contents:
//!   - Title + blob ID (GoldenBlobJson.id) + preset ID
//!   - Loudness metrics (LUFS, True Peak, LRA, Short Term, Momentary)
//!   - Platform compliance table (Spotify, YouTube, Apple, Tidal, Broadcast)
//!   - Quality metrics (Stereo Correlation, Dynamic Range, RMS, Clip Free)
//!
//! No re-running DSP during export — reads stored metrics from GoldenBlobJson.

use base64::{Engine, engine::general_purpose::STANDARD};
use printpdf::*;
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::ipc::m0_client::{GoldenBlobJson, M0Client};

#[tauri::command]
pub async fn export_pdf_report(
    blob_id: String,
    app:     tauri::AppHandle,
) -> Result<String, String> {
    // Fetch the Golden Blob metrics from M0 (no PCM — only the JSON metadata)
    let client = M0Client::new();
    let blob   = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // Native save dialog (blocking — must run outside async context)
    let path_result = tokio::task::spawn_blocking({
        let app = app.clone();
        move || {
            app.dialog()
                .file()
                .set_file_name("mastered-bmr128.pdf")
                .add_filter("PDF Document", &["pdf"])
                .blocking_save_file()
        }
    }).await.map_err(|e| e.to_string())?;

    let output_path = match path_result {
        Some(FilePath::Path(p)) => p.to_string_lossy().to_string(),
        Some(FilePath::Url(u))  => u.to_string(),
        None                    => return Err("Cancelled".into()),
    };

    generate_bmr128_pdf(&blob, &output_path)?;

    eprintln!("[report] BMR-128 PDF saved: blob_id={} path={}", blob.id, output_path);

    Ok(output_path)
}

/// Returns PDF as base64 string for in-app preview.
/// Saves to temp file, reads back as base64.
/// Frontend renders it via <embed> or <iframe> with data URI.
#[tauri::command]
pub async fn preview_pdf_report(
    blob_id: String,
) -> Result<String, String> {
    let client = M0Client::new();
    let blob   = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // Write to temp file
    let tmp_path = std::env::temp_dir()
        .join(format!("bmr128_{}.pdf", &blob_id[..8]));
    let tmp_str  = tmp_path.to_string_lossy().to_string();

    generate_bmr128_pdf(&blob, &tmp_str)?;

    let pdf_bytes = tokio::fs::read(&tmp_path).await
        .map_err(|e| format!("Failed to read temp PDF: {e}"))?;
    
    // Optionally delete temp file here
    let _ = tokio::fs::remove_file(&tmp_path).await;

    let base64_str = STANDARD.encode(&pdf_bytes);
    Ok(base64_str)
}

/// Generate a BMR-128 PDF compliance report and write to `path`.
///
/// Uses printpdf (pure Rust, MIT). A4 portrait (210mm × 297mm).
/// All data from GoldenBlobJson — no DSP re-run, no PCM.
fn generate_bmr128_pdf(blob: &GoldenBlobJson, path: &str) -> Result<(), String> {
    let (doc, page1, layer1) = PdfDocument::new(
        "BMR-128 Compliance Report",
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );

    let page  = doc.get_page(page1);
    let layer = page.get_layer(layer1);

    let font      = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font error: {e}"))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| format!("PDF bold font error: {e}"))?;

    // ── Header ────────────────────────────────────────────────────────────────

    layer.use_text("BMR-128 COMPLIANCE REPORT", 18.0, Mm(20.0), Mm(277.0), &font_bold);
    layer.use_text("Still Air — Creator OS",     10.0, Mm(20.0), Mm(269.0), &font);

    layer.use_text(
        &format!("Blob ID : {}", blob.id),
        8.0, Mm(20.0), Mm(263.0), &font,
    );
    layer.use_text(
        &format!("Preset  : {}", blob.preset_id),
        8.0, Mm(20.0), Mm(258.0), &font,
    );

    layer.use_text(
        "---------------------------------------------------------------------",
        8.0, Mm(20.0), Mm(254.0), &font,
    );

    // ── Loudness metrics ──────────────────────────────────────────────────────

    layer.use_text("LOUDNESS METRICS", 13.0, Mm(20.0), Mm(247.0), &font_bold);

    let loudness_data: [(&str, String); 6] = [
        ("Integrated LUFS",  format!("{:.1} LUFS", blob.loudness.integrated_lufs)),
        ("True Peak",        format!("{:.1} dBTP", blob.loudness.true_peak_dbtp)),
        ("Loudness Range",   format!("{:.1} LU",   blob.loudness.lra)),
        ("Short Term LUFS",  format!("{:.1} LUFS", blob.loudness.short_term_lufs)),
        ("Momentary LUFS",   format!("{:.1} LUFS", blob.loudness.momentary_lufs)),
        ("EBU R128 Target",  format!("{:.0} LUFS", blob.loudness.ebu_r128_target_lufs)),
    ];
    for (i, (label, value)) in loudness_data.iter().enumerate() {
        let y = Mm(239.0 - i as f32 * 8.0);
        layer.use_text(*label, 10.0, Mm(25.0), y, &font);
        layer.use_text(value.as_str(), 10.0, Mm(130.0), y, &font_bold);
    }

    // ── Platform compliance ───────────────────────────────────────────────────

    let comp_y_start = 239.0 - loudness_data.len() as f32 * 8.0 - 10.0;
    layer.use_text("PLATFORM COMPLIANCE", 13.0, Mm(20.0), Mm(comp_y_start), &font_bold);

    let compliance_data: [(&str, bool); 7] = [
        ("Spotify (-14 LUFS)",        blob.loudness.spotify_compliant),
        ("YouTube (-14 LUFS)",        blob.loudness.youtube_compliant),
        ("Apple Music (-16 LUFS)",    blob.loudness.apple_music_compliant),
        ("Apple Podcasts (-16 LUFS)", blob.loudness.apple_podcasts_compliant),
        ("Tidal (-14 LUFS)",          blob.loudness.tidal_compliant),
        ("Broadcast (-23 LUFS)",      blob.loudness.broadcast_compliant),
        ("EBU R128",                  blob.loudness.ebu_r128_compliant),
    ];
    for (i, (platform, compliant)) in compliance_data.iter().enumerate() {
        let y = Mm(comp_y_start - 8.0 - i as f32 * 8.0);
        layer.use_text(*platform, 10.0, Mm(25.0), y, &font);
        layer.use_text(
            if *compliant { "PASS" } else { "FAIL" },
            10.0, Mm(130.0), y, &font_bold,
        );
    }

    // ── Quality metrics ───────────────────────────────────────────────────────

    let qual_y_start = comp_y_start - 8.0 - compliance_data.len() as f32 * 8.0 - 10.0;
    layer.use_text("QUALITY METRICS", 13.0, Mm(20.0), Mm(qual_y_start), &font_bold);

    let clip_str = if blob.quality.clip_free {
        "YES — clip free".to_string()
    } else {
        format!("NO — {} clips", blob.quality.clips_detected)
    };

    let quality_data: [(&str, String); 5] = [
        ("Stereo Correlation", format!("{:.3}",    blob.quality.stereo_correlation)),
        ("Dynamic Range",      format!("{:.1} dB", blob.quality.dynamic_range_db)),
        ("RMS Level",          format!("{:.1} dB", blob.quality.rms_db)),
        ("Stereo Width",       format!("{:.2}",    blob.quality.stereo_width)),
        ("Clip Free",          clip_str),
    ];
    for (i, (label, value)) in quality_data.iter().enumerate() {
        let y = Mm(qual_y_start - 8.0 - i as f32 * 8.0);
        layer.use_text(*label, 10.0, Mm(25.0), y, &font);
        layer.use_text(value.as_str(), 10.0, Mm(130.0), y, &font_bold);
    }

    // ── Footer ────────────────────────────────────────────────────────────────

    layer.use_text(
        "---------------------------------------------------------------------",
        8.0, Mm(20.0), Mm(22.0), &font,
    );
    layer.use_text(
        "Generated by Still Air — Creator OS | LineOS v1.0 | BMR-128 Standard",
        8.0, Mm(20.0), Mm(17.0), &font,
    );
    layer.use_text(
        "This report is generated from stored analysis data. No audio re-processing was performed.",
        7.0, Mm(20.0), Mm(12.0), &font,
    );

    // ── Save ──────────────────────────────────────────────────────────────────

    doc.save(&mut std::io::BufWriter::new(
        std::fs::File::create(path)
            .map_err(|e| format!("PDF create failed: {e}"))?
    )).map_err(|e| format!("PDF save failed: {e}"))
}
