use m0d::db::schema::Track;
use m0d::handlers::deliver::{
    run_deliver_core, sanitize_title, validate_and_plan, write_manifest_file, DeliverEntry,
    DeliverRequest,
};
use std::io::Write;

fn make_fake_track(id: &str, dur: u64, path: &str) -> Track {
    Track {
        id: Some(id.to_string()),
        project_id: "proj_1".into(),
        track_id: id.to_string(),
        audio_path: path.into(),
        blob_id: "blob_1".into(),
        blob_path: "/tmp/blob_1.json".into(),
        lufs: -14.0,
        true_peak: -1.0,
        flavour_id: "test".into(),
        created_at: "now".into(),
        duration_ms: dur,
    }
}

#[test]
fn test_deliver_validation() {
    let t1 = make_fake_track("t1", 1000, "/tmp/fake1");
    let t2 = make_fake_track("t2", 121 * 60 * 1000, "/tmp/fake2"); // >120min

    let db_tracks = vec![t1, t2];

    let req = DeliverRequest {
        output_dir: Some("/tmp/out".into()),
        book_title: "My Book".into(),
        entries: vec![
            DeliverEntry {
                track_id: "t1".into(),
                index: 1,
                title: "Opening".into(),
                role: "opening_credits".into(),
            },
            DeliverEntry {
                track_id: "t1".into(),
                index: 1,
                title: "Dup".into(),
                role: "opening_credits".into(),
            },
            DeliverEntry {
                track_id: "t2".into(),
                index: 4,
                title: "Long Chap".into(),
                role: "chapter".into(),
            },
            DeliverEntry {
                track_id: "t3".into(),
                index: 5,
                title: "Missing".into(),
                role: "chapter".into(),
            },
            DeliverEntry {
                track_id: "t1".into(),
                index: 6,
                title: "!!!".into(),
                role: "chapter".into(),
            },
        ],
    };

    let result = validate_and_plan(&req, &db_tracks);
    assert!(result.is_err());
    let errs = result.unwrap_err();
    let errs_str = errs.join(" | ");

    assert!(errs_str.contains("indices must be contiguous"));
    assert!(errs_str.contains("duplicate index 1"));
    assert!(errs_str.contains("at most one opening_credits"));
    assert!(errs_str.contains("exceeds 120min limit"));
    assert!(errs_str.contains("not found in project"));
    assert!(errs_str.contains("empty after sanitization"));
}

#[test]
fn test_deliver_naming() {
    assert_eq!(
        sanitize_title("Chapter 1: The Beginning!"),
        "Chapter_1_The_Beginning"
    );
    assert_eq!(sanitize_title("   Spaces   "), "Spaces");
    assert_eq!(sanitize_title("!!!_---_!!!"), "-");
}

fn generate_test_pcm(path: &std::path::Path) {
    let mut file = std::fs::File::create(path).unwrap();
    let mut phase = 0.0f32;
    for frame in 0..(48000 * 4) {
        // 4 seconds
        let sec = frame as f32 / 48000.0;
        let is_burst = (sec % 2.0) < 1.0;

        let mut sample = (phase * std::f32::consts::TAU).sin();
        phase += 440.0 / 48000.0;
        if phase >= 1.0 {
            phase -= 1.0;
        }

        if is_burst {
            sample *= 0.1;
        } else {
            sample *= 0.000316;
        }

        let bytes = sample.to_le_bytes();
        file.write_all(&bytes).unwrap();
        file.write_all(&bytes).unwrap(); // left and right
    }
}

#[test]
fn test_deliver_e2e_small() {
    let pcm1 = std::path::PathBuf::from("/tmp/deliver_test_1.pcm");
    let pcm2 = std::path::PathBuf::from("/tmp/deliver_test_2.pcm");
    generate_test_pcm(&pcm1);
    generate_test_pcm(&pcm2);

    let t1 = make_fake_track("t1", 4000, pcm1.to_str().unwrap());
    let t2 = make_fake_track("t2", 4000, pcm2.to_str().unwrap());
    let db_tracks = vec![t1, t2];

    let req = DeliverRequest {
        output_dir: Some("/tmp/deliver_out".into()),
        book_title: "My Book".into(),
        entries: vec![
            DeliverEntry {
                track_id: "t1".into(),
                index: 1,
                title: "Opening".into(),
                role: "opening_credits".into(),
            },
            DeliverEntry {
                track_id: "t2".into(),
                index: 2,
                title: "Chap 1".into(),
                role: "chapter".into(),
            },
        ],
    };

    let plan = validate_and_plan(&req, &db_tracks).unwrap();
    let mut resp = run_deliver_core(&req, plan).unwrap();
    // ΤΟ run_deliver_core πια ΔΕΝ γράφει το manifest.json — επιστρέφει τη
    // δήλωση ως τιμή (resp.manifest). Το τεστ γράφει τώρα το ίδιο πριν
    // διαβάσει το αρχείο, αφού δεν χρειάζεται sidecar εδώ.
    write_manifest_file(&mut resp);

    let book_dir = resp.book_dir.unwrap();
    assert!(std::path::Path::new(&book_dir).exists());
    assert_eq!(resp.files.len(), 2);

    let f1 = std::path::Path::new(&book_dir).join(&resp.files[0]);
    assert!(f1.exists());

    let manifest_path = resp.manifest_path.unwrap();
    let manifest_str = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest_str.contains("noise_floor_db"));
    assert!(manifest_str.contains("rms_spread_db"));
    assert!(manifest_str.contains("head_quiet_secs"));
    assert!(manifest_str.contains("tail_quiet_secs"));
    // ΑΛΛΑΞΕ 2026-08-25: η κρίση του spacing έπαψε να είναι ελεύθερο
    // κείμενο («head room tone …» σε `warnings`) και έγινε ΔΟΜΗΜΕΝΗ
    // εγγραφή §5.3 στο `spacing_checks` κάθε manifest entry.
    // Η ΠΡΟΘΕΣΗ του assert ΔΕΝ αλλάζει — «η κρίση του spacing φτάνει
    // στο manifest» — αλλά τώρα ελέγχεται σε μορφή που διαβάζεται από
    // μηχανή. Το test ΕΠΙΑΣΕ πραγματικό κενό: χωρίς αυτό το πεδίο, η
    // αφαίρεση των warnings άφηνε τη διαδρομή fallback χωρίς κρίση.
    assert!(manifest_str.contains("spacing_checks"));
    assert!(manifest_str.contains("head_spacing"));
    assert!(manifest_str.contains("tail_spacing"));
    // ΚΑΙ ότι η τρίτη κατάσταση ταξιδεύει: το fixture έχει σχεδόν
    // μηδενικό room tone στα άκρα ⇒ ΣΥΣΤΑΣΗ, ΟΧΙ παραβίαση.
    assert!(manifest_str.contains("advisory"));
    assert!(!manifest_str.contains("\"verdict\": \"fail\""));

    let _ = std::fs::remove_file(pcm1);
    let _ = std::fs::remove_file(pcm2);
    let _ = std::fs::remove_dir_all(book_dir);
}

#[test]
#[ignore = "θέλει ffprobe στο PATH (Command::new :207) — εξωτερικό εργαλείο, όχι fixture. ~1.3s. Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn test_deliver_acx_ffprobe() {
    let pcm1 = std::path::PathBuf::from("/tmp/deliver_test_ffprobe.pcm");
    generate_test_pcm(&pcm1);
    let t1 = make_fake_track("t1", 4000, pcm1.to_str().unwrap());
    let db_tracks = vec![t1];

    let req = DeliverRequest {
        output_dir: Some("/tmp/deliver_out_ffprobe".into()),
        book_title: "Ffprobe Book".into(),
        entries: vec![DeliverEntry {
            track_id: "t1".into(),
            index: 1,
            title: "Opening".into(),
            role: "chapter".into(),
        }],
    };

    let plan = validate_and_plan(&req, &db_tracks).unwrap();
    let resp = run_deliver_core(&req, plan).unwrap();

    let book_dir = resp.book_dir.unwrap();
    let f1 = std::path::Path::new(&book_dir).join(&resp.files[0]);

    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=sample_rate,bit_rate",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            f1.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines[0], "44100");
    assert_eq!(lines[1], "192000");

    let _ = std::fs::remove_file(pcm1);
    let _ = std::fs::remove_dir_all(book_dir);
}
