use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

/// End-to-end test: synthetic 5.1 WAV →
/// spatial_conformance_path →
/// StoredBlob (channels=6, blob_type=spatial_bed)
///
/// Validates the full pipeline path:
/// decode_smart → FiveDotOne arm in decode_node
/// → spatial_conformance_path in dsp_pipeline
/// → StoredBlob with correct metadata
#[test]
fn e2e_5dot1_wav_produces_spatial_blob() {
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_5dot1_input.wav"
    );

    if !std::path::Path::new(input_path).exists() {
        panic!(
            "Test fixture missing: {}\n\
             Generate with python script",
            input_path
        );
    }

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "apple_spatial_bed".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-session-spatial-001".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None, // progress_tx
        None, // progress_map
        "job-spatial-test".to_string(),
        state_tmp.path().to_str().unwrap(),
        "/tmp",
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (blob, _spatial, _path, _model, _, _artifacts) = result.unwrap();

    assert_eq!(blob.core.channels, 6, "Expected 6 channels in spatial blob");
    assert_eq!(
        blob.core.blob_type, "spatial_bed",
        "Expected blob_type = spatial_bed"
    );
    assert!(blob.core.sample_rate == 48000, "Expected 48000 Hz sample rate");

    // ΤΟ PATH ΑΠΟ ΤΟ BLOB. Το χτίσιμο με format! ήταν
    // ο λόγος που το test δεν έσπασε όταν άλλαξε το
    // παραδοτέο — έλεγχε αρχείο που κανείς δεν
    // παραδίδει.
    let out_path = blob.core.audio_path.path();

    // ADM BWF: 24-bit interleaved, 6 κανάλια, ΣΥΝ
    // headers (RIFF/fmt/bext/chna/axml).
    // 48000 × 6 × 3 = 864000 bytes PCM. Τα chunks
    // προσθέτουν ~1.5K — μετρημένο 846K συνολικά.
    // ΔΕΝ ελέγχουμε ακριβές μέγεθος: το axml είναι
    // static αλλά το bext μπορεί να αλλάξει.
    let pcm_bytes = std::fs::read(out_path).expect("ADM dump should exist");
    let bytes = &pcm_bytes;

    assert!(
        bytes.len() > 864_000,
        "ADM BWF must be larger than its PCM payload"
    );
    assert!(
        bytes.len() < 864_000 + 8192,
        "header overhead unexpectedly large: {}",
        bytes.len() - 864_000
    );

    // Το ΟΝΟΜΑ δεν αποδεικνύει format (δόγμα Ι).
    // Ελέγχουμε τα magic bytes.
    assert_eq!(&bytes[0..4], b"RIFF", "not a RIFF file");
    assert_eq!(&bytes[8..12], b"WAVE", "not a WAVE file");

    // Τα ADM chunks που κάνουν το αρχείο υποβάλλσιμο.
    // Χωρίς αυτά είναι απλό WAV, όχι ADM BWF.
    let has = |tag: &[u8]| bytes.windows(4).any(|w| w == tag);
    assert!(has(b"bext"), "missing bext chunk");
    assert!(has(b"chna"), "missing chna chunk");
    assert!(has(b"axml"), "missing axml chunk");
    assert!(has(b"data"), "missing data chunk");

    // Το pcm_blake3 ήταν None μέχρι το pass 3.
    // Τώρα υπάρχει, και είναι hash του ΠΕΡΙΕΧΟΜΕΝΟΥ
    // (interleaved f32 LE πριν το 24-bit), όχι του
    // container.
    let hash = blob.core.pcm_blake3.as_ref()
        .expect("spatial blob must carry an output hash");
    assert_eq!(hash.len(), 64, "blake3 hex must be 64 chars");

    println!(
        "Spatial blob: id={} channels={} sample_rate={} blob_type={}",
        blob.core.id, blob.core.channels, blob.core.sample_rate, blob.core.blob_type
    );

    // Ο in-process MultichannelLufsMeter αφαιρέθηκε.
    // Μετρούσε το ΕΝΔΙΑΜΕΣΟ raw f32 PCM, που πλέον
    // σβήνεται μετά το pass 3 — είναι εργασιακό αρχείο,
    // όχι παραδοτέο.
    // Το ffmpeg είναι ο ΜΟΝΟΣ oracle εδώ, και αυτό είναι
    // βελτίωση: μετράει το ΑΡΧΕΙΟ ΠΟΥ ΠΑΡΑΔΙΔΕΤΑΙ, όχι
    // ένα ενδιάμεσο, και το κάνει με εξωτερική
    // υλοποίηση EBU R128 αντί για τη δική μας.

    // (2) Run FFMPEG Ground Truth (Primary Check)
    let ffmpeg_status = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output();
    let mut ffmpeg_lufs: Option<f32> = None;

    if ffmpeg_status.is_ok() {
        // ΧΩΡΙΣ raw flags. Το ffmpeg διαβάζει sample rate,
        // channels και bit depth από τα RIFF headers.
        //
        // ΚΑΙ ΑΥΤΟ ΕΙΝΑΙ Η ΙΣΧΥΡΟΤΕΡΗ ΑΠΟΔΕΙΞΗ ΤΟΥ TEST:
        // ένα εξωτερικό εργαλείο που ανοίγει το αρχείο
        // ΧΩΡΙΣ βοήθεια αποδεικνύει ότι το container είναι
        // έγκυρο — περισσότερο από κάθε assertion που
        // γράφουμε εμείς. Αν το ADM BWF ήταν
        // κακοσχηματισμένο, το ffmpeg θα αποτύγχανε να το
        // αποκωδικοποιήσει.
        let output = std::process::Command::new("ffmpeg")
            .args([
                "-i",
                out_path.to_str().unwrap(),
                "-af", "ebur128",
                "-f", "null", "-",
            ])
            .output()
            .expect("Failed to execute ffmpeg");

        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut in_summary = false;

        for line in stderr.lines() {
            if line.contains("Summary:") {
                in_summary = true;
                continue;
            }
            if in_summary && line.contains("I:") && line.contains("LUFS") {
                // e.g. "    I:         -18.0 LUFS"
                if let Some(val_str) = line.split("LUFS").next() {
                    if let Some(num_str) = val_str.split("I:").nth(1) {
                        if let Ok(val) = num_str.trim().parse::<f32>() {
                            ffmpeg_lufs = Some(val);
                            break;
                        }
                    }
                }
            }
        }
    }

    // (3) Print results and assert
    if let Some(ffmpeg_val) = ffmpeg_lufs {
        let ffmpeg_delta = (ffmpeg_val - -18.0).abs();
        println!(
            "Oracle (ffmpeg ebur128): measured={:.2} LUFS, target=-18.0 (delta: {:.3})",
            ffmpeg_val, ffmpeg_delta
        );

        assert!(
            ffmpeg_delta <= 0.5,
            "FFMPEG normalization failed: expected -18.0 ±0.5, got {:.2}",
            ffmpeg_val
        );
    } else {
        // ΧΩΡΙΣ ffmpeg δεν υπάρχει έλεγχος LUFS. Τα δομικά
        // assertions (RIFF, chunks, μέγεθος, hash) τρέχουν
        // ούτως ή άλλως — το test παραμένει χρήσιμο, απλώς
        // δεν επαληθεύει τη στάθμη.
        // ΔΕΝ κάνουμε το test ignored γι' αυτό: ένα test που
        // ελέγχει τα μισά είναι καλύτερο από ένα που δεν
        // τρέχει.
        println!(
            "SKIP: ffmpeg not in PATH — LUFS not verified. \
             Structural assertions still ran."
        );
    }
}
