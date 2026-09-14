//! ΦΡΟΥΡΟΣ: το παραδοτέο κουβαλάει ΔΙΚΟ ΤΟΥ υπογεγραμμένο cert, δεμένο
//! στο ΑΡΧΕΙΟ που παραδίδεται.
//!
//! ΓΙΑΤΙ ΥΠΑΡΧΕΙ (F-090, μετρημένο 25/08): ο χρήστης έπαιρνε mp3 44.1k
//! mono και δίπλα του ανυπόγραφο έγγραφο που περιέγραφε τον master —
//! 48k stereo, peak πριν τον encoder. ΚΑΝΕΝΑ μέγεθος του master cert δεν
//! ισχύει για το εξαγόμενο αρχείο· τα στερεοφωνικά πεδία είναι ΑΝΕΥ
//! ΝΟΗΜΑΤΟΣ σε mono.
//!
//! ⚠ ΤΟ ΚΡΙΣΙΜΟ ΠΟΥ ΦΡΟΥΡΕΙΤΑΙ ΕΔΩ: το `deliverable_sha256` δένει στο
//! ΠΡΑΓΜΑΤΙΚΟ αρχείο. Ένα cert που λέει «αυτό είναι το αρχείο σου» χωρίς
//! να δαγκώνει όταν το αρχείο αλλάξει είναι διακοσμητικό.

use lineos_types::audio::ManagedPcm;
use m0d::blob_store::{
    BlobVariant, StoredBlobCore, StoredBlobV2, StoredLoudness, StoredProvenance, StoredQuality,
    StoredSpatial,
};
use m0d::dsp::signal_health::DeadAirSummary;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn generate_test_pcm(path: &Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    for frame in 0..(48000 * 12) {
        let sec = frame as f32 / 48000.0;
        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        // Πυκνό σήμα με ήσυχα άκρα, ώστε το spacing να έχει τι να μετρήσει.
        sample *= if (2.0..10.0).contains(&sec) { 0.1 } else { 0.000_316 };
        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap();
    }
}

fn make_blob(pcm_path: PathBuf, preset_id: &str) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: format!("deliverable_cert_{preset_id}"),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "".into(),
            input_path_hash: "".into(),
            input_pcm_sha256: None,
            seed: 1,
            pipeline_version: "".into(),
            schema_version: 1,
            preset_id: preset_id.into(),
            pcm_blake3: None,
            cert_signature: None,
            audio_path: Arc::new(ManagedPcm::new(pcm_path)),
            sample_rate: 48000,
            channels: 2,
            num_frames: 48000 * 12,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness::default(),
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: DeadAirSummary::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
            corrections: Vec::new(),
        },
    }
}

/// Τρέχει τον ΠΡΑΓΜΑΤΙΚΟ handler — η μόνη pub είσοδος που φτάνει στο
/// ιδιωτικό `export_mp3_routed`, άρα και στον writer του cert.
async fn export_through_the_real_handler(preset_id: &str, out: &Path) -> serde_json::Value {
    let tmp = tempfile::TempDir::new().unwrap();
    let pcm = tmp.path().join("in.pcm");
    generate_test_pcm(&pcm);
    let blob = make_blob(pcm, preset_id);
    let blob_id = blob.core.id.clone();

    let config = Arc::new(m0d::config::M0Config::from_env());
    let audit = Arc::new(m0d::audit::AuditLog::open(&config.audit_log_dir).expect("audit"));
    let (state, _handles) = m0d::app_state::AppState::new_for_test(audit, config).await;
    state.blob_store.insert(blob);

    let req = m0d::handlers::export::ExportRequest {
        blob_id,
        format: "mp3".into(),
        output_path: out.to_string_lossy().into_owned(),
    };
    let axum::Json(resp) =
        m0d::handlers::export::export_audio(axum::extract::State(state), axum::Json(req)).await;
    assert_eq!(resp.status, "ok", "export απέτυχε: {:?}", resp.message);
    serde_json::to_value(&resp).unwrap()
}

fn read_cert(mp3: &Path) -> serde_json::Value {
    let cert_path = mp3.with_extension("deliverable.json");
    assert!(
        cert_path.exists(),
        "ΔΕΝ γράφτηκε deliverable cert στο {}",
        cert_path.display()
    );
    let raw = std::fs::read_to_string(&cert_path).unwrap();
    serde_json::from_str(&raw).expect("το cert πρέπει να είναι έγκυρο JSON")
}

// ── (1) ΤΟ ΑΡΧΕΙΟ ΚΑΙ ΤΟ ΕΓΓΡΑΦΟ ΤΟΥ ────────────────────────────────

#[test]
#[ignore = "θέλει ffprobe στο PATH — επαληθεύει τα delivered_* ΜΕ ΕΞΩΤΕΡΙΚΟ όργανο, όχι με τον εαυτό μας. ~10s. Το ξυπνά: scripts/run-ignored.sh"]
fn the_cert_agrees_with_an_outside_tool() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mp3 = tmp.path().join("out.mp3");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(export_through_the_real_handler("acx", &mp3));

    let cert = read_cert(&mp3);
    let tech = &cert["technical"];

    let probe = |stream_field: &str| -> String {
        let o = std::process::Command::new("ffprobe")
            .args([
                "-v", "error",
                "-select_streams", "a:0",
                "-show_entries", &format!("stream={stream_field}"),
                "-of", "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(&mp3)
            .output()
            .expect("ffprobe execution failed");
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    };

    assert_eq!(
        tech["sample_rate"].as_u64().unwrap().to_string(),
        probe("sample_rate"),
        "το cert διαφωνεί με το ffprobe στο sample_rate"
    );
    assert_eq!(
        tech["channels"].as_u64().unwrap().to_string(),
        probe("channels"),
        "το cert διαφωνεί με το ffprobe στα channels"
    );

    // Το bitrate είναι μέσος όρος bytes×8/secs — δεν απαιτείται ταύτιση
    // στο ψηφίο με το ffprobe, αλλά ΠΡΕΠΕΙ να είναι στην ίδια τάξη.
    let ours = tech["bitrate_kbps"].as_f64().unwrap();
    let theirs: f64 = probe("bit_rate").parse::<f64>().unwrap_or(0.0) / 1000.0;
    assert!(
        theirs > 0.0 && (ours - theirs).abs() < 16.0,
        "bitrate: εμείς {ours:.1} kbps, ffprobe {theirs:.1} kbps"
    );
}

// ── (3) Ο ORACLE ΠΟΥ ΔΑΓΚΩΝΕΙ ───────────────────────────────────────

/// ΕΝΑ byte αλλάζει στο mp3 ⇒ το `deliverable_sha256` ΔΕΝ ταιριάζει πια.
///
/// Χωρίς αυτό, το `deliverable_sha256` θα μπορούσε να είναι οτιδήποτε —
/// ακόμα και hash του master — και κανείς δεν θα το πρόσεχε.
#[test]
fn tampering_one_byte_breaks_the_bond() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mp3 = tmp.path().join("out.mp3");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(export_through_the_real_handler("acx", &mp3));

    let cert = read_cert(&mp3);
    let declared = cert["deliverable_sha256"].as_str().unwrap().to_string();
    assert_eq!(declared.len(), 64, "SHA-256 σε hex = 64 χαρακτήρες");

    let sha_of = |p: &Path| -> String {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(p).unwrap();
        format!("{:x}", Sha256::digest(&bytes))
    };

    assert_eq!(
        declared,
        sha_of(&mp3),
        "το cert δεν περιγράφει το αρχείο που γράφτηκε δίπλα του"
    );

    // ── Η ΑΛΛΟΙΩΣΗ: ΕΝΑ byte, στη μέση, ώστε να μην είναι header ──
    let mut bytes = std::fs::read(&mp3).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0x01;
    std::fs::write(&mp3, &bytes).unwrap();

    assert_ne!(
        declared,
        sha_of(&mp3),
        "ΤΟ ΔΕΣΜΟ ΔΕΝ ΔΑΓΚΩΝΕΙ: ένα byte άλλαξε και το hash έμεινε ίδιο"
    );
}

// ── ΤΟ ΣΧΗΜΑ: ΜΟΝΟ Ο,ΤΙ ΜΕΤΡΗΘΗΚΕ ΣΤΟ ΠΑΡΑΔΟΤΕΟ ────────────────────

/// ⚠ Ο ΚΑΝΟΝΑΣ ΑΠΟΥΣΙΑΣ §5.2, ΣΕ ΕΠΙΠΕΔΟ ΕΓΓΡΑΦΟΥ.
///
/// Αυτός ο oracle είναι ΑΡΝΗΤΙΚΟΣ επίτηδες: απαριθμεί ό,τι ΔΕΝ πρέπει
/// να υπάρχει. Το θετικό («έχει sample_rate») δεν θα έπιανε ποτέ την
/// επιστροφή ενός LUFS από τον master — που είναι ακριβώς το F-090.
#[test]
fn no_master_measurement_leaks_into_the_deliverable_cert() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mp3 = tmp.path().join("out.mp3");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(export_through_the_real_handler("acx", &mp3));

    let raw = std::fs::read_to_string(mp3.with_extension("deliverable.json")).unwrap();

    for forbidden in [
        "integrated_lufs",
        "short_term_lufs",
        "momentary_lufs",
        "true_peak_dbtp",
        "lra",
        "stereo_correlation",
        "stereo_width",
        "dynamic_range_db",
        "spectral_centroid",
        "spectral_flatness",
        "spotify_compliant",
        "apple_music_compliant",
        "broadcast_compliant",
    ] {
        assert!(
            !raw.contains(forbidden),
            "μέγεθος ΤΟΥ MASTER διέρρευσε στο cert του παραδοτέου: {forbidden}"
        );
    }

    let cert: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // Και τα δύο κλειδιά του δεσμού ΠΑΡΟΝΤΑ, έστω και null: «δεσμός προς
    // None ΔΕΝ είναι δεσμός» — αλλά η απουσία γράφεται, δεν σιωπά.
    assert!(
        cert["master"].get("master_sha256").is_some(),
        "το master_sha256 πρέπει να υπάρχει ως κλειδί, έστω null"
    );
    assert!(
        cert["master"].get("master_pcm_blake3").is_some(),
        "το master_pcm_blake3 πρέπει να υπάρχει ως κλειδί, έστω null"
    );
    assert_eq!(cert["audio_origin"], "self_produced");
    assert_eq!(cert["format"], "creator-os-deliverable-cert");
    assert!(
        cert["delivery_checks"].as_array().unwrap().len() >= 4,
        "οι γραμμές §5.3 πρέπει να ταξιδεύουν μέσα στο cert"
    );
    assert!(cert["delivery_verdict"].get("complies").is_some());
}

// ── Η ΔΙΑΔΡΟΜΗ ΠΟΥ ΔΕΝ ΜΕΤΡΑΕΙ ΔΕΝ ΠΙΣΤΟΠΟΙΕΙ ──────────────────────

/// ΑΠΟΦΑΣΗ, ΔΗΛΩΜΕΝΗ: ΚΑΝΕΝΑ deliverable cert για μη-ACX preset.
///
/// Στη διαδρομή `spotify` δεν γίνεται decode-back, δεν μετριέται
/// spacing, δεν παράγονται γραμμές §5.3 — δηλαδή ΔΕΝ ΥΠΑΡΧΕΙ ΜΕΤΡΗΣΗ
/// ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ. Ένα cert εκεί θα ήταν φάκελος με υπογραφή γύρω από
/// το τίποτα. Ο κανόνας απουσίας §5.2: καμία μέτρηση ⇒ καμία εγγραφή.
#[test]
fn a_route_that_does_not_measure_does_not_certify() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mp3 = tmp.path().join("out.mp3");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(export_through_the_real_handler("spotify", &mp3));

    assert!(mp3.exists(), "το mp3 ΓΡΑΦΤΗΚΕ — το export πέτυχε");
    assert!(
        !mp3.with_extension("deliverable.json").exists(),
        "γράφτηκε deliverable cert σε διαδρομή που ΔΕΝ μετράει το παραδοτέο"
    );
}
