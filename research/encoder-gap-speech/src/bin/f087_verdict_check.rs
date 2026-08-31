//! F-087 open question (β): with rms_db = −57.05, what does the VERDICT
//! say? Does the system break VISIBLY (FAIL) or does it sign silence as
//! compliant (PASS)?
//!
//! Calls the REAL production chain in-process (execute_streaming_plan →
//! export_mp3_acx) and then the REAL verdict functions
//! (AcxCheckReport::passes_acx / passes_acx_with_margin / margin_checks —
//! sp314_dsp, the SAME ones deliver.rs:471 and deliver.rs:420 call to fill
//! ManifestEntry.passes_acx and StoredLoudness.delivery_checks).
//! No reimplementation. Read-only: writes only to temp dir.

use m0d::blob_store::BlobVariant;

fn main() {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/w-speech/corpus/2277-149896-0000.flac".to_string());
    let input_abs = std::fs::canonicalize(&input).expect("resolve input");

    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: input_abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "f087-verdict-check".to_string(),
    };

    let (output, blob) = m0d::agents::executor::execute_streaming_plan(&plan, None)
        .expect("execute_streaming_plan failed");

    // ── What the STREAMING certificate itself carries (before any export) ──
    println!("=== STREAMING CERTIFICATE (what gets signed + written to sidecar) ===");
    if let BlobVariant::Certified { loudness, .. } = &blob.variant {
        println!("MEASURED cert.input_acx_compliant       = {:?}", loudness.input_acx_compliant);
        println!("MEASURED cert.delivery_checks           = {:?}", loudness.delivery_checks);
        println!("MEASURED cert.delivery_profile          = {:?}", loudness.delivery_profile);
        println!("MEASURED cert.output_delivery_rms_db    = {:?}", loudness.output_delivery_rms_db);
        println!("MEASURED cert.output_delivery_peak_db   = {:?}", loudness.output_delivery_peak_db);
        println!("MEASURED cert.output_delivery_noise_floor_db = {:?}", loudness.output_delivery_noise_floor_db);
        println!("MEASURED cert.integrated_lufs           = {}", loudness.integrated_lufs);
    } else {
        println!("blob is NOT Certified: {:?}", blob.uncertified_reason());
    }

    // ── Now the export, and the verdict computed from it ──
    let mp3_path = std::env::temp_dir().join(format!("f087-verdict-{}.mp3", output.blob_id));
    let outcome = m0d::handlers::export::export_mp3_acx(&blob, &mp3_path)
        .expect("export_mp3_acx failed");
    let report = outcome.report;

    println!();
    println!("=== EXPORT REPORT (AcxCheckReport from the real export_mp3_acx) ===");
    println!("MEASURED sample_peak_db  = {}", report.sample_peak_db);
    println!("MEASURED rms_db          = {}", report.rms_db);
    println!("MEASURED noise_floor_db  = {:?}", report.noise_floor_db);

    println!();
    println!("=== THE VERDICT (real sp314_dsp fns — same ones deliver.rs calls) ===");
    println!("MEASURED passes_acx()             = {}", report.passes_acx());
    println!("MEASURED passes_acx_with_margin() = {}   <-- this is ManifestEntry.passes_acx (deliver.rs:472)",
        report.passes_acx_with_margin());

    println!();
    println!("=== delivery_checks AS run_deliver_core WOULD WRITE THEM (deliver.rs:420) ===");
    for c in report.margin_checks() {
        println!(
            "  metric={:<12} bound={:<4} measured={:>12.4} required={:>8.2} margin={:>5.2} verdict={}",
            c.metric,
            c.bound,
            c.measured_db,
            c.required_db,
            c.margin_applied_db,
            if c.verdict { "PASS" } else { "FAIL" }
        );
    }

    println!();
    println!("DONE");
}
