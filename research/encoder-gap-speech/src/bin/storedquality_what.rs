//! ΑΠΟΔΕΙΞΗ 2026-08-24 — StoredQuality σε ΔΥΟ διαφορετικά αρχεία,
//! ζωντανή διαδρομή (execute_streaming_plan, /master/streaming).
//! Πρόβλεψη: σταθερές θα βγουν ΤΑΥΤΟΣΗΜΕΣ, μετρημένα θα διαφέρουν.
//!
//! usage: storedquality_what <file_a> <file_b>

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::blob_store::BlobVariant;

fn run_one(path: &str, label: &str) -> Option<m0d::blob_store::StoredQuality> {
    let plan = StreamingPlan {
        audio_path: path.to_string(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: format!("storedquality-{label}"),
    };
    match execute_streaming_plan(&plan, None) {
        Ok((_output, blob)) => match blob.variant {
            BlobVariant::Certified { quality, .. } => {
                eprintln!("[{label}] {path}");
                eprintln!("  {quality:?}");
                Some(quality)
            }
            _ => {
                eprintln!("[{label}] {path} — not Certified variant");
                None
            }
        },
        Err(e) => {
            eprintln!("[{label}] {path} — FAILED: {e:?}");
            None
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: storedquality_what <file_a> <file_b>");

    let qa = run_one(&args[0], "A");
    let qb = run_one(&args[1], "B");

    if let (Some(a), Some(b)) = (qa, qb) {
        eprintln!();
        eprintln!("=== ΣΥΓΚΡΙΣΗ, πεδίο-προς-πεδίο ===");
        macro_rules! cmp {
            ($field:ident) => {
                let same = a.$field == b.$field;
                eprintln!(
                    "  {:22} A={:?}  B={:?}  {}",
                    stringify!($field),
                    a.$field,
                    b.$field,
                    if same { "ΤΑΥΤΟΣΗΜΟ" } else { "ΔΙΑΦΕΡΕΙ" }
                );
            };
        }
        cmp!(stereo_correlation);
        cmp!(phase_coherence);
        cmp!(stereo_width);
        cmp!(dynamic_range_db);
        cmp!(rms_db);
        cmp!(spectral_centroid);
        cmp!(spectral_flatness);
        cmp!(clips_detected);
        cmp!(clip_free);
    }
}
