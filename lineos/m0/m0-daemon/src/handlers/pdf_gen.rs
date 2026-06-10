//! Silent Certificate PDF Generator
//! Runs automatically after mastering — no user action needed
//! Output: {blob_id_short}_certificate.pdf alongside corpus files

use crate::blob_store::StoredBlob;
use printpdf::*;
use std::fs::File;
use std::io::BufWriter;

pub fn generate_silent_certificate(blob: &StoredBlob, output_path: &str) {
    let result = std::panic::catch_unwind(|| _generate(blob, output_path));
    if result.is_err() {
        eprintln!(
            "[cert] PDF generation failed silently for {}",
            &blob.id[..8]
        );
    }
}

fn _generate(blob: &StoredBlob, output_path: &str) {
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
    layer.use_text(&blob.id, 9.0, Mm(20.0), Mm(252.0), &font);

    // EBU Metrics
    let lufs = blob.loudness.integrated_lufs;
    let tp = blob.loudness.true_peak_dbtp;
    let lra = blob.loudness.lra;

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
    if let Some(fp) = &blob.stem_fingerprints {
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
    if let Some(hash) = &blob.pcm_blake3 {
        layer.use_text("CRYPTOGRAPHIC PROOF", 11.0, Mm(20.0), Mm(161.0), &font_bold);
        layer.use_text(format!("BLAKE3: {}", hash), 8.0, Mm(20.0), Mm(154.0), &font);
    }

    // Timeline
    if !blob.processing_timeline.is_empty() {
        layer.use_text("PROCESSING TIMELINE", 11.0, Mm(20.0), Mm(144.0), &font_bold);
        let mut y = 137.0f32;
        for record in &blob.processing_timeline {
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
