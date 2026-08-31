//! F-085 wiring: ΕΞΩΤΕΡΙΚΟΣ ΕΝΟΡΚΟΣ.
//!
//! Τρέχει τη ζωντανή διαδρομή ΜΙΑ φορά, τυπώνει τα cert νούμερα, και
//! αντιγράφει το ΙΔΙΟ master wav ώστε να το μετρήσει το ffmpeg. Η
//! σύγκριση είναι cert-vs-ffmpeg ΣΤΟ ΙΔΙΟ ΑΡΧΕΙΟ — όχι με τον εαυτό μας.
//!
//! Η αντιγραφή ΠΡΕΠΕΙ να γίνει μέσα στο τρέξιμο: το ManagedPcm::drop
//! σβήνει το wav μόλις πέσει το blob.

use m0d::blob_store::BlobVariant;

fn main() {
    let input = std::env::args().nth(1).expect("usage: quality_vs_ffmpeg <audio>");
    let out = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "/tmp/quality_juror_master.wav".to_string());
    let abs = std::fs::canonicalize(&input).expect("resolve");

    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "quality-juror".to_string(),
    };
    let (_o, blob) =
        m0d::agents::executor::execute_streaming_plan(&plan, None).expect("live path failed");

    match std::fs::copy(blob.core.audio_path.path(), &out) {
        Ok(n) => println!("MEASURED master copied -> {out} ({n} bytes)"),
        Err(e) => println!("FAILED copy: {e}"),
    }

    if let BlobVariant::Certified { quality, loudness, .. } = &blob.variant {
        println!("MEASURED cert.stereo_correlation = {:.6}", quality.stereo_correlation);
        println!("MEASURED cert.dynamic_range_db   = {:.6}", quality.dynamic_range_db);
        println!("MEASURED cert.rms_db             = {:.6}", quality.rms_db);
        println!("MEASURED cert.integrated_lufs    = {:.6}", loudness.integrated_lufs);
        println!(
            "MEASURED lufs+3.0 (παλιό fallback) = {:.6}  ⇒ διαφορά {:.6} dB",
            loudness.integrated_lufs + 3.0,
            quality.rms_db - (loudness.integrated_lufs + 3.0)
        );
        println!("--- ΣΤΑΘΕΡΕΣ (δεν αγγίχτηκαν) ---");
        println!("MEASURED cert.phase_coherence    = {}", quality.phase_coherence);
        println!("MEASURED cert.stereo_width       = {}", quality.stereo_width);
        println!("MEASURED cert.spectral_centroid  = {}", quality.spectral_centroid);
        println!("MEASURED cert.spectral_flatness  = {}", quality.spectral_flatness);
        println!("MEASURED cert.clips_detected     = {}", quality.clips_detected);
        println!("MEASURED cert.clip_free          = {}", quality.clip_free);
    }
    println!("DONE");
}
