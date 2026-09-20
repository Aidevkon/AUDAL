//! certificate_node — the parts that stay in the server.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P5
//!
//! `run`/`run_streaming`/`assemble_blob`/`CertificateOutput`/
//! `StreamingCertData`/`MIN_MOBILE_PLAYBACK_LUFS` moved to conformance
//! 21/09 (§7 απόφαση 3, F-137) — the unsigned declaration as a value.
//! `sign_and_render` stays: it needs identity.rs, handlers/
//! certificate.rs and handlers/pdf_gen.rs, all axum/key-adjacent and
//! staying with the server.

pub use conformance::certificate_node::{
    run, run_streaming, CertificateOutput, StreamingCertData, MIN_MOBILE_PLAYBACK_LUFS,
};

/// Storage-layer work `assemble_blob` no longer does itself: the
/// Ed25519 signature, the QR PNG, and the PDF archival copy are all
/// adapters outside the core (DECISIONS.md §7, απόφαση 3 — «Η δήλωση
/// είναι τιμή. Αποθήκευση, απόδοση και επαλήθευση είναι προσαρμογείς
/// έξω από τον πυρήνα»). Called by the three sites that call
/// `run`/`run_streaming` (execute_streaming_plan, run_dsp_internal
/// ×2), right after, using the unsigned value they got back — same
/// shape as `apply_sidecar_updates` in `handlers/deliver.rs`, same
/// order, same error text.
pub fn sign_and_render(output: &mut CertificateOutput) -> Result<(), String> {
    let blob_id = output.blob.core.id.clone();
    let pcm_blake3 = output.blob.core.pcm_blake3.clone().unwrap_or_default();
    let lufs = output
        .blob
        .loudness()
        .map(|l| l.integrated_lufs)
        .unwrap_or_default();

    let identity = crate::identity::load_or_generate_default().map_err(|e| e.to_string())?;
    let cert_sig =
        crate::handlers::certificate::sign_certificate(&blob_id, &pcm_blake3, lufs, &identity);
    output.blob.core.cert_signature = Some(cert_sig);

    let filename = output
        .file_path
        .path()
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let true_peak = output
        .blob
        .loudness()
        .map(|l| l.true_peak_dbtp)
        .unwrap_or_default();
    let lra = output.blob.loudness().map(|l| l.lra).unwrap_or_default();
    let qr = output.blob.stem_fingerprints().and_then(|fp| {
        crate::handlers::certificate::generate_qr_base64(
            &blob_id, &filename, lufs, true_peak, lra, fp,
        )
    });
    if let crate::blob_store::BlobVariant::Certified { qr_base64, .. } = &mut output.blob.variant
    {
        *qr_base64 = qr;
    }

    // Generate PDF certificate — silent, never blocks pipeline.
    // F-067 (2026-08-21): writer ευθυγραμμισμένος με τον reader του
    // GET /blob/:id/certificate.pdf — ΗΤΑΝ temp_dir()/m0d-cert-{short}.pdf
    // ενώ ο reader ζητούσε {masters}/certs_render/{short}_certificate.pdf:
    // το endpoint ήταν νεκρό (mismatch φακέλου ΚΑΙ ονόματος).
    let certs_dir = std::path::PathBuf::from(format!(
        "{}/certs_render",
        crate::config::M0Config::from_env().masters_path
    ));
    let _ = std::fs::create_dir_all(&certs_dir);
    let pdf_path = certs_dir.join(format!(
        "{}_certificate.pdf",
        &blob_id[..blob_id.len().min(8)]
    ));
    let pdf_path_str = pdf_path.to_string_lossy();
    crate::handlers::pdf_gen::generate_silent_certificate(&output.blob, &pdf_path_str);

    Ok(())
}
