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

pub fn blake3_pcm(pcm: &[f32]) -> String {
    let bytes: Vec<u8> = pcm.iter()
        .flat_map(|s| s.to_le_bytes())
        .collect();
    blake3::hash(&bytes).to_hex().to_string()
}

pub fn sign_certificate(
    cert_id:    &str,
    pcm_hash:   &str,
    lufs:       f32,
    fingerprints: &StemFingerprints,
) -> String {
    // Deterministic signing key from pipeline hash
    // NOT random — INV-AB-1: same input → same signature
    use ed25519_dalek::SigningKey;
    let seed = fingerprints.pipeline.as_bytes();
    let mut key_bytes = [0u8; 32];
    for (i, &b) in seed.iter().take(32).enumerate() {
        key_bytes[i] = b;
    }
    let signing_key = SigningKey::from_bytes(&key_bytes);
    let payload = format!("{cert_id}:{pcm_hash}:{lufs:.2}");
    use ed25519_dalek::Signer;
    let sig = signing_key.sign(payload.as_bytes());
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    format!("eyJhbGciOiJFZERTQSJ9.{}.{}",
        URL_SAFE_NO_PAD.encode(payload.as_bytes()),
        URL_SAFE_NO_PAD.encode(sig.to_bytes()))
}
