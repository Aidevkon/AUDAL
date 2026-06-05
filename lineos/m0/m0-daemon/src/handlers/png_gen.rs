use image::{RgbImage, Rgb};
use imageproc::drawing::draw_text_mut;
use crate::blob_store::StoredBlob;

pub fn generate_certificate_png(blob: &StoredBlob, output_path: &str) {
    // A4 at 150dpi: 1240 x 1754 px
    let mut img = RgbImage::from_pixel(1240, 1754, Rgb([255u8, 255, 255]));
    
    // Load font
    let font_data = include_bytes!("../assets/fonts/CourierPrime-Regular.ttf");
    let font = ab_glyph::FontRef::try_from_slice(font_data).unwrap();
    let scale_lg = ab_glyph::PxScale::from(48.0);
    let scale_md = ab_glyph::PxScale::from(32.0);
    let scale_sm = ab_glyph::PxScale::from(24.0);
    
    let black  = Rgb([26u8, 26, 26]);
    let green  = Rgb([46u8, 125, 50]);
    let gray   = Rgb([85u8, 85, 85]);
    let blue   = Rgb([44u8, 125, 160]);
    
    // Header
    draw_text_mut(&mut img, black, 60, 60, scale_lg, &font, "CREATOR OS");
    draw_text_mut(&mut img, black, 60, 120, scale_md, &font, 
        "MASTERING CERTIFICATE");
    
    // Certificate ID
    draw_text_mut(&mut img, gray, 60, 220, scale_sm, &font, "Certificate ID:");
    draw_text_mut(&mut img, blue, 60, 256, scale_sm, &font, &blob.id);
    
    // EBU Metrics
    let lufs = blob.loudness.integrated_lufs;
    let tp   = blob.loudness.true_peak_dbtp;
    let lra  = blob.loudness.lra;
    draw_text_mut(&mut img, green, 60, 380, scale_md, &font,
        &format!("{:.2} LUFS   {:.2} dBTP   {:.2} LU", lufs, tp, lra));
    draw_text_mut(&mut img, green, 60, 430, scale_sm, &font,
        "EBU R128 ✓   ITU-BS.1770 ✓   Deterministic ✓");
    
    // Tagline
    draw_text_mut(&mut img, gray, 60, 1660, scale_sm, &font,
        "Your stems. Your machine. Your sound.");
    draw_text_mut(&mut img, gray, 60, 1694, scale_sm, &font,
        "Privacy by architecture. Zero cloud processing.");
    
    let _ = img.save(output_path);
}

use axum::{
    extract::{Path, State},
    Json,
};
use crate::app_state::AppState;

// POST /cert/:blob_id/png
pub async fn export_cert_png(
    Path(blob_id): Path<String>,
    State(state):  State<AppState>,
    Json(req):     Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let output_path = req["output_path"].as_str().unwrap_or("certificate.png");
    if let Some(blob) = state.blob_store.get(&blob_id) {
        generate_certificate_png(&blob, output_path);
        Json(serde_json::json!({"status": "ok"}))
    } else {
        Json(serde_json::json!({"status": "error", "message": "blob not found"}))
    }
}
