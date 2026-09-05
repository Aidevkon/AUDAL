//! ΦΡΟΥΡΟΣ: η ετυμηγορία του `/export` ΔΕΝ είναι πάντα-πράσινη.
//!
//! ΓΙΑΤΙ ΥΠΑΡΧΕΙ: στις 25/08 το `/export` απέκτησε `delivery_checks` — τη
//! μετρημένη ετυμηγορία του παραδοτέου δίπλα στο `status`. Η προφανής
//! αποτυχία μιας τέτοιας δυνατότητας είναι να λέει **πάντα pass**, και να
//! μην το προσέξει κανείς επειδή τα πραγματικά αρχεία περνάνε.
//!
//! Η προσπάθεια να παραχθεί κόκκινο μέσω ΗΧΟΥ (fixture με θόρυβο −50 dBFS)
//! ΑΠΕΤΥΧΕ ΝΑ ΚΟΚΚΙΝΙΣΕΙ — ΜΕΤΡΗΜΕΝΟ 25/08, και είναι δικό του εύρημα:
//!     είσοδος (πιο ήσυχο 500 ms)   −57.830
//!     μετά τον render (noise gate) −65.949   ⇒ το gate κατέβασε 8.12 dB
//!     μετά τη διόρθωση RMS +3.219  −62.473   ⇒ ΠΕΡΝΑΕΙ με 2.47 dB περιθώριο
//! Το gate της restoration δουλεύει ακριβώς εκεί που μετριέται το noise
//! floor (το πιο ήσυχο παράθυρο 500 ms), οπότε ένα ρεαλιστικό noise bed δεν
//! αρκεί. Το να κουρδιστεί το fixture μέχρι να κοκκινίσει θα ήταν
//! κατασκευή αποτελέσματος.
//!
//! Άρα ο φρουρός χτυπάει τον ΜΗΧΑΝΙΣΜΟ κατευθείαν: η
//! `DeliveryCheck::from_margin_checks` είναι καθαρή συνάρτηση ενός
//! `AcxCheckReport`. Δίνεται report εκτός προδιαγραφής και απαιτείται
//! `verdict == "fail"`, με το ΜΕΤΡΗΜΕΝΟ νούμερο να επιβιώνει στην εγγραφή.

use m0d::blob_store::DeliveryCheck;
use lineos_types::presets::{ACX, SPOTIFY};
use sp314_dsp::analysis::acx_check::AcxCheckReport;

fn find<'a>(checks: &'a [DeliveryCheck], metric: &str, bound: &str) -> &'a DeliveryCheck {
    checks
        .iter()
        .find(|c| c.metric == metric && c.bound == bound)
        .unwrap_or_else(|| panic!("λείπει εγγραφή {metric}/{bound}"))
}

/// ORACLE 1 — ΤΟ ΚΡΙΣΙΜΟ: report εκτός προδιαγραφής ⇒ "fail", όχι "pass".
/// Τρεις ανεξάρτητοι τρόποι αποτυχίας ταυτόχρονα, ώστε ένα always-pass να
/// μην μπορεί να κρυφτεί πίσω από μία μετρική.
#[test]
fn a_failing_report_produces_fail_records_with_the_measured_numbers() {
    let report = AcxCheckReport {
        sample_peak_db: -0.5,          // > -3.0  ⇒ fail
        rms_db: -30.0,                 // < -23.0 ⇒ fail (min), pass (max)
        noise_floor_db: Some(-45.0),   // > -60.0 ⇒ fail
        quietest_window_start_frame: Some(0),
    };
    let checks = DeliveryCheck::from_margin_checks(&report);

    assert_eq!(find(&checks, "peak", "max").verdict, "fail");
    assert_eq!(find(&checks, "rms", "min").verdict, "fail");
    assert_eq!(find(&checks, "noise_floor", "max").verdict, "fail");

    // Το ΝΟΥΜΕΡΟ επιβιώνει — μια ετυμηγορία χωρίς τη μέτρησή της δεν
    // λέει στον χρήστη ΠΟΣΟ έξω είναι.
    assert_eq!(find(&checks, "noise_floor", "max").measured, -45.0);
    assert_eq!(find(&checks, "noise_floor", "max").required, -60.0);
    assert_eq!(find(&checks, "peak", "max").measured, -0.5);
    assert!(checks.iter().all(|c| c.unit == "db"));
}

/// ORACLE 2 — και το αντίστροφο: συμμορφούμενο report ⇒ όλα "pass".
/// Χωρίς αυτό, ένα always-fail θα περνούσε τον ORACLE 1.
/// Οι τιμές είναι ΤΟΥ ΠΡΑΓΜΑΤΙΚΟΥ πρώτου ACX αρχείου (ΜΕΤΡΗΜΕΝΟ 25/08).
#[test]
fn the_real_first_acx_deliverable_produces_four_passes() {
    let report = AcxCheckReport {
        sample_peak_db: -4.768217,
        rms_db: -22.500002,
        noise_floor_db: Some(-90.42713),
        quietest_window_start_frame: Some(119_070),
    };
    let checks = DeliveryCheck::from_margin_checks(&report);

    assert_eq!(checks.len(), 4, "rms×2 (min+max) + peak + noise_floor");
    assert!(
        checks.iter().all(|c| c.verdict == "pass"),
        "το πρώτο πραγματικό παραδοτέο πρέπει να περνάει: {checks:#?}"
    );
}

/// ORACLE 3 — ΚΑΝΟΝΑΣ ΑΠΟΥΣΙΑΣ (§5.2): μετρική που ΔΕΝ μετρήθηκε δεν
/// παράγει εγγραφή, ΟΥΤΕ κατασκευασμένο "fail". Αρχείο < 1 s δίνει
/// `noise_floor_db: None`.
#[test]
fn an_unmeasured_metric_produces_no_record_at_all() {
    let report = AcxCheckReport {
        sample_peak_db: -6.0,
        rms_db: -20.0,
        noise_floor_db: None,
        quietest_window_start_frame: None,
    };
    let checks = DeliveryCheck::from_margin_checks(&report);

    assert_eq!(checks.len(), 3, "χωρίς noise_floor εγγραφή");
    assert!(
        !checks.iter().any(|c| c.metric == "noise_floor"),
        "απουσία μέτρησης ⇒ ΚΑΜΙΑ εγγραφή, όχι κατασκευασμένη"
    );
}

/// ORACLE 4 — ΜΙΑ ΥΛΟΠΟΙΗΣΗ: η εγγραφή που πάει στο sidecar/απόκριση
/// συμφωνεί δείγμα-προς-δείγμα με τον κανόνα από τον οποίο προέρχεται.
/// Αν κάποιος γράψει δεύτερο κανόνα, αυτό σπάει.
#[test]
fn the_record_agrees_with_the_single_rule_it_came_from() {
    for report in [
        AcxCheckReport {
            sample_peak_db: -0.5,
            rms_db: -30.0,
            noise_floor_db: Some(-45.0),
            quietest_window_start_frame: Some(0),
        },
        AcxCheckReport {
            sample_peak_db: -4.768217,
            rms_db: -22.500002,
            noise_floor_db: Some(-90.42713),
            quietest_window_start_frame: Some(119_070),
        },
    ] {
        let rule = report.margin_checks();
        let records = DeliveryCheck::from_margin_checks(&report);
        assert_eq!(rule.len(), records.len());
        for (r, d) in rule.iter().zip(records.iter()) {
            assert_eq!(d.metric, r.metric);
            assert_eq!(d.measured, r.measured_db);
            assert_eq!(d.required, r.required_db);
            assert_eq!(d.bound, r.bound);
            assert_eq!(d.margin_applied, r.margin_applied_db);
            assert_eq!(d.verdict, if r.verdict { "pass" } else { "fail" });
        }
        // ΚΑΙ η συνολική κρίση συμφωνεί με το levels_within_limits_with_margin().
        assert_eq!(
            records.iter().all(|c| c.verdict == "pass") && report.noise_floor_db.is_some(),
            report.levels_within_limits_with_margin()
        );
    }
}

// ── ΤΡΙΤΗ ΚΑΤΑΣΤΑΣΗ: "advisory" (προσθήκη 2026-08-25) ────────────────
//
// Η πηγή διακρίνει ΑΠΑΙΤΗΣΗ («must not exceed 5 seconds») από ΣΥΣΤΑΣΗ
// («we recommend between 1 and 5»). Οι δύο oracles παρακάτω πινάρουν
// ότι η διάκριση επιβιώνει στην εγγραφή — και, ΤΟ ΚΡΙΣΙΜΟ, ότι το
// advisory ΔΕΝ αγγίζει τη συνολική κρίση.

/// ORACLE 5 — ΣΥΣΤΑΣΗ εκτός ⇒ "advisory", ΟΧΙ "fail".
/// Το σχήμα του σημερινού παραδοτέου: head 0.10 s (κάτω από τη
/// σύσταση), tail 1.70 s (μέσα στο παράθυρο).
#[test]
fn a_recommendation_missed_is_advisory_not_failure() {
    let checks = DeliveryCheck::from_spacing(&ACX, 0.10, 1.70);
    assert_eq!(checks.len(), 4, "head×2 + tail×2");

    // ΣΥΣΤΑΣΗ αστοχεί ⇒ advisory
    let h_min = find(&checks, "head_spacing", "min");
    assert_eq!(h_min.verdict, "advisory");
    assert_eq!(h_min.required, 1.0);
    assert_eq!(h_min.measured, 0.10);
    assert_eq!(h_min.unit, "seconds");
    // ΜΗΔΕΝ margin — δηλωμένο (F-077: ΜΕΤΡΗΣΗ ΟΧΙ GATE)
    assert_eq!(h_min.margin_applied, 0.0);

    // ΑΠΑΙΤΗΣΗ τηρείται ⇒ pass
    assert_eq!(find(&checks, "head_spacing", "max").verdict, "pass");
    // tail εντός παραθύρου ⇒ pass ΚΑΙ στα δύο
    assert_eq!(find(&checks, "tail_spacing", "min").verdict, "pass");
    assert_eq!(find(&checks, "tail_spacing", "max").verdict, "pass");

    // ΚΑΝΕΝΑ "fail" πουθενά — σύσταση δεν είναι αποτυχία.
    assert!(
        !checks.iter().any(|c| c.verdict == "fail"),
        "σύσταση εκτός ΔΕΝ παράγει fail: {checks:#?}"
    );
}

/// ORACLE 6 — ΑΠΑΙΤΗΣΗ εκτός ⇒ "fail". Και το κάτω όριο τηρείται
/// ταυτόχρονα, ώστε το fail να μην μπορεί να αποδοθεί στη σύσταση.
#[test]
fn a_requirement_breached_is_fail() {
    let checks = DeliveryCheck::from_spacing(&ACX, 2.0, 6.0);

    assert_eq!(find(&checks, "tail_spacing", "max").verdict, "fail");
    assert_eq!(find(&checks, "tail_spacing", "max").required, 5.0);
    assert_eq!(find(&checks, "tail_spacing", "min").verdict, "pass");
    assert_eq!(find(&checks, "head_spacing", "max").verdict, "pass");
    assert_eq!(find(&checks, "head_spacing", "min").verdict, "pass");
}

/// ORACLE 7 — ΤΟ ΚΡΙΣΙΜΟΤΕΡΟ: το spacing ΔΕΝ μπαίνει στη συνολική
/// κρίση. Το `levels_within_limits_with_margin()` τρέχει πάνω στα
/// `margin_checks()`, που περιέχουν rms×2 + peak + noise_floor και
/// ΤΙΠΟΤΑ άλλο — άρα κανένα advisory ή spacing-fail δεν μπορεί να το
/// μολύνει. Αν κάποιος μελλοντικά χώσει το spacing μέσα στο
/// `margin_checks()`, ΑΥΤΟ ΕΔΩ ΣΠΑΕΙ.
#[test]
fn spacing_never_enters_the_overall_judgement() {
    let report = AcxCheckReport {
        sample_peak_db: -4.768217,
        rms_db: -22.500002,
        noise_floor_db: Some(-90.42713),
        quietest_window_start_frame: Some(119_070),
    };
    // Το ΙΔΙΟ report, με spacing που ΚΑΙ συστήνεται-εκτός ΚΑΙ παραβιάζει.
    let spacing = DeliveryCheck::from_spacing(&ACX, 0.10, 6.0);
    assert!(spacing.iter().any(|c| c.verdict == "advisory"));
    assert!(spacing.iter().any(|c| c.verdict == "fail"));

    // Η συνολική κρίση παραμένει TRUE — δομικά, όχι κατά τύχη.
    assert!(
        report.levels_within_limits_with_margin(),
        "το spacing ΔΕΝ πρέπει να επηρεάζει τη συνολική κρίση"
    );
    // Και τα margin_checks δεν περιέχουν καμία εγγραφή spacing.
    assert!(
        !report
            .margin_checks()
            .iter()
            .any(|c| c.metric.contains("spacing")),
        "το margin_checks() ΔΕΝ πρέπει να περιέχει spacing"
    );
}

/// ORACLE 8 — ΣΥΜΒΑΤΟΤΗΤΑ ΠΡΟΣ ΤΑ ΠΙΣΩ: sidecar γραμμένο ΠΡΙΝ την
/// 25/08 δεν περιέχει "advisory" και διαβάζεται αμετάβλητο.
/// Το JSON παρακάτω είναι στη ΠΑΛΙΑ μορφή (κλειδιά `*_db`, ΧΩΡΙΣ
/// `unit`) — ίδιο ιδίωμα με το test συμβατότητας του blob_store.
/// ΑΠΟΔΕΙΞΗ ΜΕ ΤΡΕΞΙΜΟ, όχι υπόθεση.
#[test]
fn a_pre_advisory_sidecar_still_deserialises() {
    let old = r#"[
        {"metric":"rms","measured_db":-22.9,"required_db":-23.0,
         "bound":"min","margin_applied_db":0.35,"verdict":"pass"},
        {"metric":"peak","measured_db":-4.7,"required_db":-3.0,
         "bound":"max","margin_applied_db":0.20,"verdict":"fail"}
    ]"#;
    let checks: Vec<DeliveryCheck> = serde_json::from_str(old)
        .unwrap_or_else(|e| panic!("παλιό sidecar ΔΕΝ διαβάζεται: {e}"));

    assert_eq!(checks.len(), 2);
    assert_eq!(checks[0].verdict, "pass");
    assert_eq!(checks[1].verdict, "fail");
    // Το `unit` απουσιάζει στο παλιό ⇒ default "db" (F-074).
    assert!(checks.iter().all(|c| c.unit == "db"));
    // Και ΚΑΝΕΝΑ advisory — η τρίτη κατάσταση είναι ΠΡΟΣΘΕΤΙΚΗ.
    assert!(!checks.iter().any(|c| c.verdict == "advisory"));
}

// ── ΟΙ ΤΡΕΙΣ ΤΗΣ ΜΟΡΦΗΣ (προσθήκη 2026-08-25) ───────────────────────
//
// sample_rate · channels · bitrate. Τα δύο πρώτα ΜΕΤΡΙΟΥΝΤΑΙ από το
// decode-back του symphonia· το τρίτο από το μέγεθος του αρχείου (το
// symphonia δεν εκθέτει bitrate).

/// ORACLE 9 — ΤΟ ΚΡΙΣΙΜΟ ΤΟΥ ΚΑΤΩ ΟΡΙΟΥ: το bitrate ΔΕΝ είναι ισότητα.
/// Η πηγή λέει «192 kbps **or higher**» — 256 και 320 ΠΕΡΝΑΝΕ ρητά.
/// Αν αυτό σπάσει, ο έλεγχος γράφτηκε ως `== 192` και είναι ΛΑΘΟΣ.
#[test]
fn bitrate_is_a_floor_not_an_equality() {
    for kbps in [192.0_f32, 256.0, 320.0, 192.2] {
        let checks = DeliveryCheck::from_format(&ACX, 44_100, 1, kbps);
        assert_eq!(
            find(&checks, "bitrate", "min").verdict,
            "pass",
            "{kbps} kbps πρέπει να ΠΕΡΝΑΕΙ — η πηγή λέει «192 or higher»"
        );
    }
    // ...και κάτω από το πάτωμα κόβει.
    for kbps in [128.0_f32, 191.9, 64.0] {
        let checks = DeliveryCheck::from_format(&ACX, 44_100, 1, kbps);
        assert_eq!(find(&checks, "bitrate", "min").verdict, "fail", "{kbps} kbps");
    }
}

/// ORACLE 10 — ΝΑ ΜΠΟΡΕΙ ΝΑ ΠΕΙ FAIL: λάθος ρυθμός, λάθος κανάλια,
/// λάθος bitrate — και τα τρία ταυτόχρονα, ώστε ένα always-pass να μην
/// κρύβεται πίσω από μία μετρική.
#[test]
fn a_wrong_format_produces_three_fails_with_the_measured_numbers() {
    let checks = DeliveryCheck::from_format(&ACX, 48_000, 2, 128.0);
    assert_eq!(checks.len(), 3);

    assert_eq!(find(&checks, "sample_rate", "max").verdict, "fail");
    assert_eq!(find(&checks, "channels", "max").verdict, "fail");
    assert_eq!(find(&checks, "bitrate", "min").verdict, "fail");

    // Το ΝΟΥΜΕΡΟ επιβιώνει, και οι μονάδες είναι ρητές.
    assert_eq!(find(&checks, "sample_rate", "max").measured, 48_000.0);
    assert_eq!(find(&checks, "sample_rate", "max").required, 44_100.0);
    assert_eq!(find(&checks, "sample_rate", "max").unit, "hz");
    assert_eq!(find(&checks, "channels", "max").measured, 2.0);
    assert_eq!(find(&checks, "channels", "max").unit, "count");
    assert_eq!(find(&checks, "bitrate", "min").measured, 128.0);
    assert_eq!(find(&checks, "bitrate", "min").unit, "kbps");
    // Μηδέν margin σε όλα — δηλωμένο.
    assert!(checks.iter().all(|c| c.margin_applied == 0.0));
}

/// ORACLE 11 — και το αντίστροφο: το ΠΡΑΓΜΑΤΙΚΟ παραδοτέο περνάει.
/// Χωρίς αυτό, ένα always-fail θα περνούσε τον ORACLE 10.
#[test]
fn the_real_acx_format_produces_three_passes() {
    let checks = DeliveryCheck::from_format(&ACX, 44_100, 1, 192.0);
    assert!(
        checks.iter().all(|c| c.verdict == "pass"),
        "44.1k mono 192 kbps πρέπει να περνάει: {checks:#?}"
    );
}

/// ORACLE 12 — Ο ΚΑΝΟΝΑΣ ΑΠΟΥΣΙΑΣ ΣΤΟ ΕΠΙΠΕΔΟ ΤΟΥ ΠΑΡΑΓΩΓΟΥ.
///
/// Προορισμός που ΔΕΝ δηλώνει τα όρια (κάθε μη-ACX σήμερα: το SPOTIFY
/// έχει `room_tone_max_s: None`, `required_sample_rate_hz: None`, …)
/// παράγει **ΚΑΜΙΑ ΕΓΓΡΑΦΗ** — όχι ψευδές "pass", όχι κατασκευασμένο
/// "fail". «Δεν το ορίζει ο οίκος» ≠ «το ελέγξαμε και πέρασε».
///
/// ΑΥΤΟ ΕΙΝΑΙ Η ΠΡΟΫΠΟΘΕΣΗ ΤΟΥ ΣΥΝΘΕΤΗ: το fold θα ρωτάει το
/// `DeliverySpec` ποιες γραμμές περιμένει· αν ένας παραγωγός σιωπούσε
/// ΕΝΩ το spec δηλώνει το όριο, η σιωπή θα διαβαζόταν ως pass.
#[test]
fn a_destination_that_declares_nothing_produces_no_rows() {
    // Τιμές που θα ΑΠΟΤΥΓΧΑΝΑΝ αν κρίνονταν με τα όρια του ACX:
    // tail 9 s (> 5), sample rate 48k, stereo, 96 kbps.
    let spacing = DeliveryCheck::from_spacing(&SPOTIFY, 0.0, 9.0);
    let format = DeliveryCheck::from_format(&SPOTIFY, 48_000, 2, 96.0);

    assert!(spacing.is_empty(), "spotify δεν ορίζει room tone: {spacing:#?}");
    assert!(format.is_empty(), "spotify δεν ορίζει μορφή: {format:#?}");

    // ΚΑΙ ΤΟ ΑΝΤΙΣΤΡΟΦΟ, στο ίδιο test: ο ACX ΤΑ ΟΡΙΖΕΙ, άρα μιλάει.
    assert_eq!(DeliveryCheck::from_spacing(&ACX, 0.0, 9.0).len(), 4);
    assert_eq!(DeliveryCheck::from_format(&ACX, 48_000, 2, 96.0).len(), 3);
}

/// ORACLE 13 — ΤΟ ΚΡΙΤΗΡΙΟ ΤΟΥ REFACTOR: ο αριθμός ζει ΜΙΑ φορά.
/// Οι παραγωγοί ΔΙΑΒΑΖΟΥΝ από το συμβόλαιο· δεν κρατούν αντίγραφο.
/// Αν κάποιος ξαναβάλει const, οι τιμές θα αποκλίνουν από το spec και
/// αυτό σπάει.
#[test]
fn the_producers_read_the_contract_they_do_not_copy_it() {
    let checks = DeliveryCheck::from_spacing(&ACX, 3.0, 3.0);
    assert_eq!(find(&checks, "head_spacing", "max").required, ACX.room_tone_max_s.unwrap());
    assert_eq!(
        find(&checks, "head_spacing", "min").required,
        ACX.room_tone_recommend_min_s.unwrap()
    );

    let f = DeliveryCheck::from_format(&ACX, 44_100, 1, 192.0);
    assert_eq!(find(&f, "sample_rate", "max").required, ACX.required_sample_rate_hz.unwrap() as f32);
    assert_eq!(find(&f, "channels", "max").required, ACX.emitted_channels.unwrap() as f32);
    assert_eq!(find(&f, "bitrate", "min").required, ACX.min_bitrate_kbps.unwrap() as f32);
}

// ── Ο ΣΥΝΘΕΤΗΣ (προσθήκη 2026-08-25) ────────────────────────────────
//
// Το fold ΕΙΝΑΙ ΠΟΛΙΤΙΚΗ: «advisory δεν μετράει» είναι απόφαση, όχι
// ταυτότητα. Γι' αυτό έχει δικό του oracle, όπως κάθε παραγωγός.

use m0d::blob_store::DeliveryVerdict;

/// Οι πλήρεις γραμμές ενός ACX παραδοτέου με δοσμένο spacing.
fn acx_rows(report: AcxCheckReport, head: f32, tail: f32) -> Vec<DeliveryCheck> {
    let mut v = DeliveryCheck::from_margin_checks(&report);
    v.extend(DeliveryCheck::from_spacing(&ACX, head, tail));
    v.extend(DeliveryCheck::from_format(&ACX, 44_100, 1, 192.0));
    v
}

fn good_report() -> AcxCheckReport {
    AcxCheckReport {
        sample_peak_db: -4.768217,
        rms_db: -22.500002,
        noise_floor_db: Some(-90.42713),
        quietest_window_start_frame: Some(119_070),
    }
}

/// ORACLE 14 — ΤΟ ΚΡΙΣΙΜΟ: `tail 6 s` ⇒ fail ⇒ **ΔΕΝ ΣΥΜΜΟΡΦΩΝΕΤΑΙ**.
///
/// ΑΥΤΟ ΑΚΡΙΒΩΣ θα είχε πιάσει την αντίφαση της 25/08 την ώρα που
/// γεννήθηκε: τότε το ίδιο αρχείο έγραφε "fail" στις γραμμές ΚΑΙ
/// `passes_acx: true` στο manifest.
#[test]
fn a_requirement_breach_makes_the_whole_file_non_compliant() {
    let v = DeliveryVerdict::compose(&ACX, &acx_rows(good_report(), 1.5, 6.0));

    assert!(!v.complies, "tail 6 s παραβιάζει το «must not exceed 5»: {v:#?}");
    assert!(v.failed.contains(&"tail_spacing/max".to_string()));
    assert!(v.missing.is_empty());
}

/// ORACLE 15 — ΣΥΣΤΑΣΗ εκτός ⇒ **ΣΥΜΜΟΡΦΩΝΕΤΑΙ**, με το advisory
/// ΟΡΑΤΟ. Χωρίς το δεύτερο σκέλος, η τρίτη κατάσταση θα
/// υπολογιζόταν και θα πεταγόταν στη σύνοψη.
#[test]
fn a_recommendation_missed_still_complies_but_is_reported() {
    let v = DeliveryVerdict::compose(&ACX, &acx_rows(good_report(), 0.1, 1.7));

    assert!(v.complies, "σύσταση εκτός ΔΕΝ είναι παραβίαση: {v:#?}");
    assert!(v.failed.is_empty());
    assert_eq!(v.advisory, vec!["head_spacing/min".to_string()]);
}

/// ORACLE 16 — όλα εντός ⇒ συμμορφώνεται, **μηδέν advisory**.
/// Χωρίς αυτό, ένα always-advisory θα περνούσε τον 15.
#[test]
fn a_fully_conformant_file_has_no_advisories() {
    let v = DeliveryVerdict::compose(&ACX, &acx_rows(good_report(), 2.0, 3.0));

    assert!(v.complies);
    assert!(v.failed.is_empty());
    assert!(v.advisory.is_empty(), "{v:#?}");
    assert!(v.missing.is_empty());
}

/// ORACLE 17 — ΛΕΙΠΕΙ γραμμή που το συμβόλαιο δήλωνε ⇒ **ΟΧΙ pass**.
/// Ένας παραγωγός που σιωπά θα διαβαζόταν αλλιώς ως «όλα καλά».
#[test]
fn a_missing_expected_row_is_not_a_pass() {
    let full = acx_rows(good_report(), 2.0, 3.0);
    assert!(DeliveryVerdict::compose(&ACX, &full).complies, "βάση");

    // Ο παραγωγός της μορφής σιωπά — καμία γραμμή sample_rate/channels/bitrate.
    let silent: Vec<DeliveryCheck> = full
        .iter()
        .filter(|c| !matches!(c.metric.as_str(), "sample_rate" | "channels" | "bitrate"))
        .cloned()
        .collect();
    let v = DeliveryVerdict::compose(&ACX, &silent);

    assert!(!v.complies, "σιωπή παραγωγού ΔΕΝ είναι συμμόρφωση: {v:#?}");
    assert!(v.failed.is_empty(), "δεν είναι fail — ΛΕΙΠΕΙ");
    assert_eq!(v.missing.len(), 3);
    assert!(v.missing.contains(&"bitrate/min".to_string()));
}

/// ORACLE 18 — ΤΟ ΠΡΑΓΜΑΤΙΚΟ παραδοτέο συμμορφώνεται.
/// Χωρίς αυτό, ένα always-fail θα περνούσε τον 14 και τον 17.
/// Οι τιμές είναι ΜΕΤΡΗΜΕΝΕΣ 25/08 στην πλήρη διαδρομή.
#[test]
fn the_real_deliverable_composes_to_compliant() {
    let v = DeliveryVerdict::compose(&ACX, &acx_rows(good_report(), 0.1, 1.7));
    assert!(v.complies);
    assert_eq!(v.failed.len(), 0);
    assert_eq!(v.missing.len(), 0);
}

/// ORACLE 19 — ΤΟ ΑΠΟΘΗΚΕΥΜΕΝΟ ΣΥΜΦΩΝΕΙ ΜΕ ΤΙΣ ΓΡΑΜΜΕΣ.
///
/// Η ετυμηγορία γράφεται ως πεδίο ώστε ο τρίτος να μη χρειάζεται να
/// υλοποιήσει το fold (και να το κάνει λάθος — έγινε). Το τίμημα είναι
/// ότι μπορεί να αποκλίνει. Αυτό το test ΞΑΝΑΚΑΝΕΙ το fold πάνω στις
/// σειριοποιημένες γραμμές και απαιτεί συμφωνία: **η απόκλιση γίνεται
/// αδύνατη σιωπηλά.**
#[test]
fn the_stored_verdict_agrees_with_the_stored_rows() {
    for (head, tail) in [(0.1_f32, 1.7_f32), (1.5, 6.0), (2.0, 3.0)] {
        let rows = acx_rows(good_report(), head, tail);
        let stored = DeliveryVerdict::compose(&ACX, &rows);

        // Round-trip μέσα από JSON, όπως θα το διάβαζε ο τρίτος.
        let rows_json = serde_json::to_string(&rows).unwrap();
        let verdict_json = serde_json::to_string(&stored).unwrap();
        let rows_back: Vec<DeliveryCheck> = serde_json::from_str(&rows_json).unwrap();
        let verdict_back: DeliveryVerdict = serde_json::from_str(&verdict_json).unwrap();

        assert_eq!(
            verdict_back,
            DeliveryVerdict::compose(&ACX, &rows_back),
            "η αποθηκευμένη ετυμηγορία απέκλινε από τις αποθηκευμένες γραμμές \
             (head {head}, tail {tail})"
        );
    }
}
