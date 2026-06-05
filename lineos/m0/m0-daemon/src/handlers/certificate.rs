use qrcode::{QrCode, EcLevel};
use image::Luma;
use base64::{Engine, engine::general_purpose::STANDARD};
use crate::blob_store::StemFingerprints;

pub fn generate_qr_base64(
    cert_id:    &str,
    filename:   &str,
    lufs:       f32,
    tp:         f32,
    lra:        f32,
    fingerprints: &StemFingerprints,
) -> Option<String> {
    let payload = serde_json::json!({
        "cert": &cert_id[..8],
        "file": filename,
        "lufs": (lufs * 100.0).round() / 100.0,
        "tp":   (tp   * 100.0).round() / 100.0,
        "lra":  (lra  * 100.0).round() / 100.0,
        "stems": {
            "v": &fingerprints.voice[..8],
            "d": &fingerprints.drums[..8],
            "b": &fingerprints.bass[..8],
            "h": &fingerprints.harmonics[..8],
            "a": &fingerprints.ambience[..8],
        },
        "pipe": &fingerprints.pipeline[..8],
        "date": chrono::Utc::now().format("%Y-%m-%d").to_string(),
        "by":   "CreatorOS poc-v3.0",
    }).to_string();

    let code = QrCode::with_error_correction_level(
        payload.as_bytes(), EcLevel::M
    ).ok()?;

    let image = code.render::<Luma<u8>>()
        .min_dimensions(200, 200)
        .build();

    let mut png_bytes: Vec<u8> = Vec::new();
    use image::ImageEncoder;
    image::codecs::png::PngEncoder::new(&mut png_bytes)
        .write_image(
            image.as_raw(),
            image.width(),
            image.height(),
            image::ExtendedColorType::L8,
        ).ok()?;

    Some(STANDARD.encode(&png_bytes))
}
