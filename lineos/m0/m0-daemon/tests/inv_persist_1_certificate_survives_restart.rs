//! INV-PERSIST-1: το certificate επιβιώνει restart.
//!
//! Το §Θ ζητούσε ΤΡΕΙΣ ελέγχους, όχι έναν:
//!   1. first-write ντετερμινιστικό   ← INV-DET-1
//!   2. read-back ντετερμινιστικό     ← ΕΔΩ
//!   3. ΙΣΟΔΥΝΑΜΙΑ των δύο            ← ΕΔΩ
//!
//! Το §Π το έλεγε από την αρχή: «έχουμε βάση και δεν γράφουμε
//! εκεί». Χωρίς αυτό, το git-for-mastering δεν στέκει — η
//! ιστορία σβήνει όταν κλείσει ο daemon.
//!
//! ΠΡΟΣΟΜΟΙΩΣΗ RESTART: νέο BlobStore::new(). Καθαρό instance,
//! ΙΔΙΟΣ δίσκος. Είναι το καθαρότερο simulation που υπάρχει —
//! ό,τι επιβιώνει, επιβιώνει ΑΠΟ ΤΟΝ ΔΙΣΚΟ και μόνο.
//!
//! ⚠ ΔΕΝ είναι μόνο το restart. Το BlobStore έχει MAX_BLOBS=16 με
//! eviction: το 17ο master σβήνει το certificate του 1ου ΜΕ ΤΟΝ
//! DAEMON ΖΩΝΤΑΝΟ. Το sidecar κλείνει και τις δύο τρύπες.

use arc_swap::ArcSwap;
use m0d::blob_store::{blob_storage_path, find_sidecar, read_sidecar, sanitize_path_component, BlobStore};
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use xaak::repo::DspState;

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")
}

/// ⚠ ΒΡΟΜΙΚΟ ΓΙΑ ΠΑΝΤΑ — **BUG-DECODE-1 ΔΙΟΡΘΩΘΗΚΕ ΠΛΗΡΩΣ**
/// (uncommitted: ο hash μπαίνει εδώ στο σφράγισμα).
///
/// ΠΡΙΝ: ένα `track_id` με slash ΔΕΝ έφτανε καν στο §Π — το render
/// έσκαγε στο decode, πριν αρχίσει το mastering (`m0d-raw-track/1.pcm`
/// — κατάλογος που δεν υπάρχει). Διορθώθηκε πρώτα το decode· τότε
/// αποκαλύφθηκε ΟΛΟΚΛΗΡΗ αλυσίδα από αδέρφια με την ΙΔΙΑ νόσο:
/// `m0d-mastering-`, `m0d-scratch-l/r-`, `m0d-premaster-l/r-`,
/// `m0d-mastered-`, `vad-trace-` — εννέα ακόμα σημεία, ίδιο σχήμα
/// `spool_dir().join(format!("{prefix}-{blob_id}...", ...))` με
/// ασανιτάριστο `blob_id`.
///
/// ΤΩΡΑ: `blob_store.rs` έχει το SPOOL NAMING REGISTRY — μία
/// συνάρτηση ανά οικογένεια (`raw_dump_path`, `raw_dump_spatial_path`,
/// `mastering_path`, `scratch_l_path`, `scratch_r_path`,
/// `premaster_l_path`, `premaster_r_path`, `mastered_path`,
/// `vad_trace_path`), όλες πάνω στην ίδια `sanitize_path_component`
/// με το FLAC persist / `blob_storage_path`. ΚΑΝΕΝΑ inline
/// `format!(...)` πάνω σε `spool_dir()` δεν επιζεί σε production
/// κώδικα — θεσμικός έλεγχος:
///   `grep -rnE 'm0d-(raw|mastering|scratch|premaster|mastered)|
///              vad-trace' --include=*.rs lineos`
///   → ΜΟΝΟ blob_store.rs (το σπίτι) + tests + σχόλια/legacy sweep
///     filter (lib.rs, prefix-match σε υπάρχοντα ονόματα, όχι
///     κατασκευή path).
///
/// Το `TRACK_ID` ΜΕΝΕΙ βρόμικο ΓΙΑ ΠΑΝΤΑ — όχι μεταβατικά. Είναι ο
/// θεσμικός φρουρός: αν αύριο κάποιο νέο scratch-family σημείο
/// ξαναγράψει inline `format!`, αυτό το gate θα ξανασκάσει.
const TRACK_ID: &str = "track/persist-1";

fn request(project_id: &str, track_id: &str) -> MasterRequest {
    MasterRequest {
        audio_path: fixture_path().to_str().unwrap().to_string(),
        preset_id: "spotify".to_string(),
        project_id: Some(project_id.to_string()),
        track_id: Some(track_id.to_string()),
        flavour_id: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        intent_tone: None,
        intent_dynamics: None,
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    }
}

#[test]
#[ignore = "full render (~15s measured, release) — run explicitly on any certificate-touching wire"]
fn inv_persist_1_certificate_survives_restart() {
    let input = fixture_path();
    if !input.exists() {
        panic!(
            "INV-PERSIST-1 fixture missing — the gate must not pass vacuously: {}",
            input.display()
        );
    }

    let masters = tempfile::TempDir::new().unwrap();
    let state_tmp = tempfile::TempDir::new().unwrap();
    std::env::set_var("M0_IDENTITY_PATH", state_tmp.path().join("identity"));
    let masters_root = masters.path().to_str().unwrap().to_string();

    // ── RENDER ────────────────────────────────────────────────
    let req = request("proj-persist", TRACK_ID);
    let (blob_ram, _, _, _, _, artifacts) = run_dsp(
        &req,
        std::time::Instant::now(),
        Arc::new(ArcSwap::from_pointee(DspState::default())),
        None,
        None,
        "inv-persist-1".to_string(),
        state_tmp.path().to_str().unwrap(),
        &masters_root,
    )
    .expect("run_dsp failed");

    // Το ΠΡΟΪΟΝ
    let master = artifacts
        .persisted_master
        .expect("master must be persisted");
    assert!(master.exists(), "FLAC must exist on disk");

    // Η ΑΠΟΔΕΙΞΗ — δηλωμένη, όχι υποτιθέμενη.
    // Αυτό είναι το κομμάτι 2 του §Π: ένα ERROR στο log δεν αρκεί·
    // ο καταναλωτής πρέπει να ΒΛΕΠΕΙ ότι υπάρχει.
    let cert = artifacts
        .persisted_certificate
        .expect("persisted_certificate must be Some on the happy path");
    assert!(cert.exists(), "sidecar must exist on disk");

    // ── Η ΣΥΜΜΕΤΡΙΑ ΤΟΥ ΖΕΥΓΟΥΣ ───────────────────────────────
    // Ίδιος κατάλογος, ίδιο basename, διαφορετική κατάληξη —
    // ΜΕ το slash καθαρισμένο και στα δύο.
    assert_eq!(
        master.parent(),
        cert.parent(),
        "product and proof must live in the SAME directory"
    );
    assert_eq!(
        master.file_stem(),
        cert.file_stem(),
        "product and proof must share a basename — the pair is homonymous"
    );
    // Το αναμενόμενο όνομα ΠΑΡΑΓΕΤΑΙ από τον ίδιο κανόνα, ώστε αυτές
    // οι γραμμές να μη χρειαστούν αλλαγή όταν λυθεί το BUG-DECODE-1.
    let expected_stem = sanitize_path_component(TRACK_ID);
    assert!(
        master.ends_with(format!("proj-persist/{expected_stem}.flac")),
        "FLAC sanitisation: got {}",
        master.display()
    );
    assert!(
        cert.ends_with(format!("proj-persist/{expected_stem}.json")),
        "sidecar sanitisation must MATCH the FLAC's: got {}",
        cert.display()
    );

    // ── RESTART ───────────────────────────────────────────────
    // Καθαρό store. Ό,τι επιβιώσει, ήρθε από τον δίσκο.
    let fresh_store = BlobStore::new();
    assert!(
        fresh_store.get(&blob_ram.core.id).is_none(),
        "the simulated restart must start empty"
    );

    // Ο εντοπισμός γίνεται ΜΟΝΟ από blob_id — όπως τον έχει το get_blob.
    let found = find_sidecar(&masters_root, &blob_ram.core.id)
        .expect("scanning the disk must not fail")
        .expect("certificate must be findable by blob_id alone");
    assert_eq!(found, cert, "find_sidecar must locate the written sidecar");

    let blob_disk = read_sidecar(&found).expect("sidecar must read and verify");

    // ── ΙΣΟΔΥΝΑΜΙΑ ────────────────────────────────────────────
    // ΙΔΙΟ serde και στα δύο ⇒ ίδια #[serde(skip)] πεδία πέφτουν.
    // Συγκρίνουμε ΤΙ ΠΑΡΑΓΕΙ ο τύπος, όχι το raw string του αρχείου:
    // το αρχείο κουβαλάει και envelope (written_at κ.λπ.) που
    // ΔΕΝ είναι το certificate.
    let ram_bytes = serde_json::to_vec(&blob_ram).expect("serialize ram blob");
    let disk_bytes = serde_json::to_vec(&blob_disk).expect("serialize disk blob");
    assert_eq!(
        ram_bytes, disk_bytes,
        "certificate is NOT byte-identical across the restart"
    );
    println!("payload bytes: {}", ram_bytes.len());
    println!(
        "payload sha256 ram : {}",
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&ram_bytes))
    );
    println!(
        "payload sha256 disk: {}",
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&disk_bytes))
    );

    // ── ΤΑ ΤΡΙΑ ΤΕΧΝΙΚΑ ───────────────────────────────────────
    // #[serde(skip)] τα πετάει· το envelope τα ξαναφέρνει. Χωρίς
    // αυτά, το ΔΗΜΟΣΙΟ BlobResponse θα ανέφερε μηδενικά ΣΑΝ
    // ΜΕΤΡΗΜΕΝΑ — δόγμα Ι, μέσα από τη σύμβαση.
    assert_eq!(
        blob_disk.core.sample_rate, blob_ram.core.sample_rate,
        "sample_rate lost across restart"
    );
    assert_eq!(
        blob_disk.core.channels, blob_ram.core.channels,
        "channels lost across restart"
    );
    assert_eq!(
        blob_disk.core.num_frames, blob_ram.core.num_frames,
        "num_frames lost across restart"
    );
    assert!(
        blob_disk.core.sample_rate > 0 && blob_disk.core.num_frames > 0,
        "technical fields restored as zeros — the envelope did not do its job"
    );

    // Ταυτότητα: το αρχείο απαντάει στην ΕΡΩΤΗΣΗ που του κάναμε.
    assert_eq!(blob_disk.core.id, blob_ram.core.id, "identity mismatch");

    // Και μπαίνει στο φρέσκο store κάτω από το σωστό κλειδί.
    fresh_store.insert(blob_disk);
    assert!(
        fresh_store.get(&blob_ram.core.id).is_some(),
        "restored certificate must be reachable after cache fill"
    );
}

/// Η ΣΥΜΜΕΤΡΙΑ ΤΗΣ SANITISATION — ΤΟ ΦΘΗΝΟ ΔΟΝΤΙ.
///
/// Μετά το BUG-DECODE-1 fix (πλήρες, SPOOL NAMING REGISTRY) το
/// end-to-end test με βρόμικο id ΤΡΕΧΕΙ ΚΑΙ ΠΕΡΝΑΕΙ ολόκληρο. Αυτό
/// εδώ όμως κοστίζει μηδέν και δεν είναι #[ignore]: πιάνει τη
/// διάσταση του κανόνα σε ΚΑΘΕ `cargo test`, χωρίς render, και
/// φρουρεί ακριβώς αυτό που θα έσπαγε σιωπηλά: το `.flac` να πάει σε
/// ένα όνομα και το `.json` σε άλλο.
///
/// Ο κανόνας του FLAC persist είναι `replace('/',"").replace('\\',"")`
/// (dsp_pipeline). Η `blob_storage_path` ΠΡΕΠΕΙ να συμφωνεί.
#[test]
fn blob_storage_path_sanitises_like_the_flac_persist() {
    // Ο κανόνας του FLAC persist, αυτούσιος από το dsp_pipeline.
    let flac_rule = |s: &str| s.replace('/', "").replace('\\', "");

    for (project, track) in [
        ("proj/1", "track/1"),
        ("proj\\1", "track\\1"),
        ("pr/oj\\1", "tr\\ack/1"),
        ("clean", "clean"),
    ] {
        let sidecar = blob_storage_path("/masters", project, track);

        // 1. Ο ίδιος κανόνας, το ίδιο αποτέλεσμα.
        assert_eq!(
            sanitize_path_component(track),
            flac_rule(track),
            "sanitize_path_component diverged from the FLAC persist rule"
        );

        // 2. ΚΑΝΕΝΑ separator δεν επιβιώνει στα components.
        let expected = std::path::Path::new("/masters")
            .join(flac_rule(project))
            .join(format!("{}.json", flac_rule(track)));
        assert_eq!(
            sidecar, expected,
            "sidecar path must be the FLAC's path with a .json extension"
        );

        // 3. ΤΟ ΖΕΥΓΟΣ ΕΙΝΑΙ ΟΜΩΝΥΜΟ — αυτό ακριβώς που σπάει σιωπηλά.
        let flac = std::path::Path::new("/masters")
            .join(flac_rule(project))
            .join(format!("{}.flac", flac_rule(track)));
        assert_eq!(sidecar.parent(), flac.parent(), "pair must share a directory");
        assert_eq!(sidecar.file_stem(), flac.file_stem(), "pair must share a stem");
    }
}
