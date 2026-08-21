use crate::blob_store::StoredBlobV2;
use image::{Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;

pub fn generate_certificate_png(blob: &StoredBlobV2, output_path: &str) {
    // A4 at 150dpi: 1240 x 1754 px
    let mut img = RgbImage::from_pixel(1240, 1754, Rgb([255u8, 255, 255]));

    // Load font
    let font_data = include_bytes!("../assets/fonts/CourierPrime-Regular.ttf");
    let font = ab_glyph::FontRef::try_from_slice(font_data).unwrap();
    let scale_lg = ab_glyph::PxScale::from(48.0);
    let scale_md = ab_glyph::PxScale::from(32.0);
    let scale_sm = ab_glyph::PxScale::from(24.0);

    let black = Rgb([26u8, 26, 26]);
    let green = Rgb([46u8, 125, 50]);
    let gray = Rgb([85u8, 85, 85]);
    let blue = Rgb([44u8, 125, 160]);

    // Header
    draw_text_mut(&mut img, black, 60, 60, scale_lg, &font, "CREATOR OS");
    draw_text_mut(
        &mut img,
        black,
        60,
        120,
        scale_md,
        &font,
        "MASTERING CERTIFICATE",
    );

    // Certificate ID
    draw_text_mut(&mut img, gray, 60, 220, scale_sm, &font, "Certificate ID:");
    draw_text_mut(&mut img, blue, 60, 256, scale_sm, &font, &blob.core.id);

    // EBU Metrics
    let l = blob.loudness()
        .expect("PNG certificate requires certified blob");
    let lufs = l.integrated_lufs;
    let tp = l.true_peak_dbtp;
    let lra = l.lra;
    draw_text_mut(
        &mut img,
        green,
        60,
        380,
        scale_md,
        &font,
        &format!("{:.2} LUFS   {:.2} dBTP   {:.2} LU", lufs, tp, lra),
    );
    draw_text_mut(
        &mut img,
        green,
        60,
        430,
        scale_sm,
        &font,
        "EBU R128 ✓   ITU-BS.1770 ✓   Deterministic ✓",
    );

    // Tagline
    draw_text_mut(
        &mut img,
        gray,
        60,
        1660,
        scale_sm,
        &font,
        "Your stems. Your machine. Your sound.",
    );
    draw_text_mut(
        &mut img,
        gray,
        60,
        1694,
        scale_sm,
        &font,
        "Privacy by architecture. Zero cloud processing.",
    );

    let _ = img.save(output_path);
}

use crate::app_state::AppState;
use crate::handlers::blob::{get_or_rehydrate, RehydrateError};
use axum::{
    extract::{Path, State},
    Json,
};

// POST /cert/:blob_id/png
pub async fn export_cert_png(
    Path(blob_id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let default_dir = format!("{}/certs_render", crate::config::M0Config::from_env().masters_path);
    let default_path_str = format!("{}/certificate.png", default_dir);
    // ΗΤΑΝ CWD — 159 PDFs στη ρίζα του repo, F-067, fixed 2026-08-21
    let output_path = req["output_path"].as_str().unwrap_or(&default_path_str);

    match get_or_rehydrate(&state, &blob_id).await {
        Ok(blob) => {
            if let Some(parent) = std::path::Path::new(output_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            generate_certificate_png(&blob, output_path);
            let abs_path = std::fs::canonicalize(output_path)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| output_path.to_string());
            Json(serde_json::json!({"status": "ok", "path": abs_path}))
        }
        Err(RehydrateError::NotFound) => {
            Json(serde_json::json!({"status": "error", "message": "blob not found"}))
        }
        Err(RehydrateError::Io(e)) => {
            Json(serde_json::json!({"status": "error", "message": format!("blob io error: {e}")}))
        }
        Err(RehydrateError::Corrupt(e)) => {
            Json(serde_json::json!({"status": "error", "message": format!("blob corrupt: {e}")}))
        }
    }
}
