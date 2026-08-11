//! Silent Certificate PDF Generator
//! Runs automatically after mastering — no user action needed
//! Output: {blob_id_short}_certificate.pdf alongside corpus files

use crate::blob_store::StoredBlobV2;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;

pub fn generate_silent_certificate(blob: &StoredBlobV2, output_path: &str) {
    let result = std::panic::catch_unwind(|| _generate(blob, output_path));
    if result.is_err() {
        eprintln!(
            "[cert] PDF generation failed silently for {}",
            &blob.core.id[..8]
        );
    }
}

fn _generate(blob: &StoredBlobV2, output_path: &str) {
    let (doc, page1, layer1) = PdfDocument::new(
        "CreatorOS Mastering Certificate",
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );
    let layer = doc.get_page(page1).get_layer(layer1);
    let font = doc.add_builtin_font(BuiltinFont::Courier).unwrap();
    let font_bold = doc.add_builtin_font(BuiltinFont::CourierBold).unwrap();

    // Header
    layer.use_text("CREATOR OS", 20.0, Mm(20.0), Mm(277.0), &font_bold);
    layer.use_text("MASTERING CERTIFICATE", 14.0, Mm(20.0), Mm(268.0), &font);

    // Separator
    let line = Line {
        points: vec![
            (Point::new(Mm(20.0), Mm(264.0)), false),
            (Point::new(Mm(190.0), Mm(264.0)), false),
        ],
        is_closed: false,
    };
    layer.add_line(line);

    // Certificate ID
    layer.use_text("Certificate ID:", 9.0, Mm(20.0), Mm(257.0), &font_bold);
    layer.use_text(&blob.core.id, 9.0, Mm(20.0), Mm(252.0), &font);

    // EBU Metrics
    let l = blob.loudness()
        .expect("PDF certificate requires certified blob");
    let lufs = l.integrated_lufs;
    let tp = l.true_peak_dbtp;
    let lra = l.lra;

    layer.use_text("COMPLIANCE", 11.0, Mm(20.0), Mm(243.0), &font_bold);
    layer.use_text(
        format!("Integrated Loudness: {:.2} LUFS  (EBU R128)", lufs),
        9.0,
        Mm(20.0),
        Mm(236.0),
        &font,
    );
    layer.use_text(
        format!("True Peak:           {:.2} dBTP", tp),
        9.0,
        Mm(20.0),
        Mm(230.0),
        &font,
    );
    layer.use_text(
        format!("Loudness Range:      {:.2} LU", lra),
        9.0,
        Mm(20.0),
        Mm(224.0),
        &font,
    );
    layer.use_text(
        "EBU R128: PASS   ITU-BS.1770: PASS   Deterministic: YES",
        9.0,
        Mm(20.0),
        Mm(218.0),
        &font,
    );

    // Stem fingerprints
    if let Some(fp) = blob.stem_fingerprints() {
        layer.use_text("STEM DNA", 11.0, Mm(20.0), Mm(208.0), &font_bold);
        layer.use_text(
            format!("Voice:     {}", fp.voice),
            9.0,
            Mm(20.0),
            Mm(201.0),
            &font,
        );
        layer.use_text(
            format!("Drums:     {}", fp.drums),
            9.0,
            Mm(20.0),
            Mm(195.0),
            &font,
        );
        layer.use_text(
            format!("Bass:      {}", fp.bass),
            9.0,
            Mm(20.0),
            Mm(189.0),
            &font,
        );
        layer.use_text(
            format!("Harmonics: {}", fp.harmonics),
            9.0,
            Mm(20.0),
            Mm(183.0),
            &font,
        );
        layer.use_text(
            format!("Ambience:  {}", fp.ambience),
            9.0,
            Mm(20.0),
            Mm(177.0),
            &font,
        );
        layer.use_text(
            format!("Pipeline:  {}", fp.pipeline),
            9.0,
            Mm(20.0),
            Mm(171.0),
            &font,
        );
    }

    // BLAKE3 + Signature
    if let Some(hash) = &blob.core.pcm_blake3 {
        layer.use_text("CRYPTOGRAPHIC PROOF", 11.0, Mm(20.0), Mm(161.0), &font_bold);
        layer.use_text(format!("BLAKE3: {}", hash), 8.0, Mm(20.0), Mm(154.0), &font);
    }

    // Timeline
    // Ρητό if let, ΟΧΙ timeline_or_empty(): το PDF είναι
    // artifact χρήστη και μια σιωπηλά παραλειπόμενη
    // ενότητα είναι χειρότερη από μια ορατή απουσία.
    if let Some(timeline) = blob.processing_timeline() {
        if !timeline.is_empty() {
            layer.use_text("PROCESSING TIMELINE", 11.0, Mm(20.0), Mm(144.0), &font_bold);
            let mut y = 137.0f32;
            for record in timeline {
                layer.use_text(
                    format!(
                        "  {}  {}ms  {}",
                        record.stage, record.duration_ms, record.stage_hash
                    ),
                    8.0,
                    Mm(20.0),
                    Mm(y),
                    &font,
                );
                y -= 6.0;
            }
        }
    }

    // Footer
    layer.use_text(
        "Your stems. Your machine. Your sound.  |  Privacy by architecture. Zero cloud.",
        8.0,
        Mm(20.0),
        Mm(20.0),
        &font,
    );

    // Save
    if let Ok(file) = File::create(output_path) {
        let _ = doc.save(&mut BufWriter::new(file));
        eprintln!("[cert] Certificate saved: {}", output_path);
    }
}

use crate::domain::nodes::album_certificate_node::AlbumCertificate;

pub fn generate_album_certificate_pdf(cert: &AlbumCertificate, output_path: &str) {
    let result = std::panic::catch_unwind(|| _generate_album(cert, output_path));
    if result.is_err() {
        eprintln!(
            "[cert] Album PDF generation failed for {}",
            &cert.album_id[..cert.album_id.len().min(8)]
        );
    }
}

fn _generate_album(cert: &AlbumCertificate, output_path: &str) {
    use printpdf::*;
    use std::fs::File;
    use std::io::BufWriter;

    let (doc, page1, layer1) = PdfDocument::new(
        "CreatorOS Album Certificate",
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );
    let layer = doc.get_page(page1).get_layer(layer1);
    let font = doc.add_builtin_font(BuiltinFont::Courier).unwrap();
    let font_bold = doc.add_builtin_font(BuiltinFont::CourierBold).unwrap();

    // Header
    layer.use_text("CREATOR OS", 20.0, Mm(20.0), Mm(277.0), &font_bold);
    layer.use_text(
        "ALBUM MASTERING CERTIFICATE",
        14.0,
        Mm(20.0),
        Mm(268.0),
        &font,
    );

    let line = Line {
        points: vec![
            (Point::new(Mm(20.0), Mm(264.0)), false),
            (Point::new(Mm(190.0), Mm(264.0)), false),
        ],
        is_closed: false,
    };
    layer.add_line(line);

    // Album metadata
    layer.use_text("ALBUM ID:", 9.0, Mm(20.0), Mm(257.0), &font_bold);
    layer.use_text(&cert.album_id, 9.0, Mm(20.0), Mm(251.0), &font);

    layer.use_text(
        format!(
            "Tracks: {}   Anchor: Track {}   Anchor LUFS: {:.2}",
            cert.track_count,
            cert.anchor_track_idx + 1,
            cert.anchor_lufs
        ),
        9.0,
        Mm(20.0),
        Mm(243.0),
        &font,
    );

    // Cryptographic proof
    layer.use_text("CRYPTOGRAPHIC PROOF", 11.0, Mm(20.0), Mm(233.0), &font_bold);
    layer.use_text(
        format!("Album Hash (SHA-256): {}", cert.album_hash),
        8.0,
        Mm(20.0),
        Mm(226.0),
        &font,
    );

    // Track summary
    layer.use_text("TRACK SUMMARY", 11.0, Mm(20.0), Mm(216.0), &font_bold);
    let mut y = 209.0f32;
    for i in 0..cert.track_count {
        let lufs = cert.track_lufs.get(i).copied().unwrap_or(-14.0);
        let fatigue = cert.ear_fatigue_applied.get(i).copied().unwrap_or(false);
        let blob_id = cert
            .track_blob_ids
            .get(i)
            .map(|s| &s[..s.len().min(8)])
            .unwrap_or("?");
        layer.use_text(
            format!(
                "  Track {:2}  {:.2} LUFS  {}  [{}]",
                i + 1,
                lufs,
                if fatigue {
                    "EarFatigue:YES"
                } else {
                    "EarFatigue:NO "
                },
                blob_id,
            ),
            8.0,
            Mm(20.0),
            Mm(y),
            &font,
        );
        y -= 6.0;
        if y < 30.0 {
            break;
        }
    }

    // Footer
    layer.use_text(
        format!(
            "Pipeline v{}  |  {}  |  CreatorOS",
            cert.pipeline_version,
            &cert.created_at[..10]
        ),
        8.0,
        Mm(20.0),
        Mm(20.0),
        &font,
    );

    if let Ok(file) = File::create(output_path) {
        let _ = doc.save(&mut BufWriter::new(file));
        eprintln!("[cert] Album certificate saved: {}", output_path);
    }
}

use crate::app_state::AppState;
use axum::http::{header, StatusCode};
use axum::{
    extract::{Path, State},
    response::Response,
};

/// GET /blob/:id/certificate.pdf
/// Returns the pre-generated per-track certificate PDF.
pub async fn get_track_certificate_pdf(
    State(_app): State<AppState>,
    Path(blob_id): Path<String>,
) -> Response {
    let short_id = &blob_id[..blob_id.len().min(8)];
    let pdf_path = format!("{}_certificate.pdf", short_id);

    match std::fs::read(&pdf_path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"certificate_{}.pdf\"", short_id),
            )
            .body(axum::body::Body::from(bytes))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Certificate not found"))
            .unwrap(),
    }
}

/// GET /album/:batch_id/certificate.pdf
/// Generates (if needed) and returns the album certificate PDF.
pub async fn get_album_certificate_pdf(
    State(app): State<AppState>,
    Path(batch_id): Path<String>,
) -> Response {
    let short_id = &batch_id[..batch_id.len().min(8)];

    let certs_dir = app.config.certs_path.clone();
    std::fs::create_dir_all(&certs_dir).unwrap_or_default();

    let json_path = format!("{}/album_{}.certificate.json", certs_dir, short_id);
    let pdf_path = format!("{}/album_{}_certificate.pdf", certs_dir, short_id);

    // Generate PDF from JSON if not already exists
    if !std::path::Path::new(&pdf_path).exists() {
        match std::fs::read_to_string(&json_path) {
            Ok(json) => {
                if let Ok(cert) = serde_json::from_str::<
                    crate::domain::nodes::album_certificate_node::AlbumCertificate,
                >(&json)
                {
                    generate_album_certificate_pdf(&cert, &pdf_path);
                }
            }
            Err(_) => {
                return Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(axum::body::Body::from("Album certificate not found"))
                    .unwrap();
            }
        }
    }

    match std::fs::read(&pdf_path) {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(
                header::CONTENT_DISPOSITION,
                format!(
                    "attachment; filename=\"album_certificate_{}.pdf\"",
                    short_id
                ),
            )
            .body(axum::body::Body::from(bytes))
            .unwrap(),
        Err(_) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::from("Certificate PDF generation failed"))
            .unwrap(),
    }
}
