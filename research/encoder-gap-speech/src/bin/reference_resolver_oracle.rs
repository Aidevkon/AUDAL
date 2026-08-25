//! ΜΕΤΡΗΣΗ 2026-08-24 — τρέχει τον ΣΗΜΕΡΙΝΟ ReferenceResolver πάνω
//! στις ΙΔΙΕΣ εισόδους του Python oracle fixture (44f22f9, 2026-07-02)
//! και συγκρίνει, ανά περίπτωση, ανά πεδίο. Καλεί το ΠΡΑΓΜΑΤΙΚΟ
//! `aether_bridge::reference_resolver::ReferenceResolver`, όχι
//! re-implementation.
//!
//! usage: reference_resolver_oracle <fixture.json>

use aether_bridge::reference_resolver::{ProfileId, ReferenceProfile, ReferenceResolver};
use serde::Deserialize;
use std::io::Write;

#[derive(Deserialize)]
struct TestCase {
    id: String,
    input_profile_db: [f32; 8],
    expected_gains_db: [f32; 8],
    expected_delta_sbr_db: f32,
    expected_delta_sbr_after_db: f32,
}

#[derive(Deserialize)]
struct Fixture {
    test_cases: Vec<TestCase>,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 1, "usage: reference_resolver_oracle <fixture.json>");
    let fixture_path = &args[0];

    let raw = std::fs::read_to_string(fixture_path).expect("read fixture");
    let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

    let profile = ReferenceProfile::load(ProfileId::PodcastV1);
    eprintln!(
        "profile: id={} g_max_db={} dead_zone_db={:?}",
        profile.id, profile.g_max_db, profile.dead_zone_db
    );
    eprintln!();

    let mut any_unexplained = false;

    for case in &fixture.test_cases {
        eprintln!("=== {} ===", case.id);

        let gains = ReferenceResolver::resolve(&case.input_profile_db, &profile);
        let delta_before = ReferenceResolver::compute_sbr_delta(&case.input_profile_db, &profile.spectral_target);
        let mut output = case.input_profile_db;
        for k in 0..8 {
            output[k] += gains[k];
        }
        let delta_after = ReferenceResolver::compute_sbr_delta(&output, &profile.spectral_target);

        let mut case_diff = false;
        for k in 0..8 {
            let diff = gains[k] - case.expected_gains_db[k];
            if diff.abs() > 1e-4 {
                case_diff = true;
                eprintln!(
                    "  band[{k}]  fixture={:+.4}  σήμερα={:+.4}  Δ={:+.4}  dead_zone[{k}]={:.1}",
                    case.expected_gains_db[k], gains[k], diff, profile.dead_zone_db[k]
                );
            }
        }
        let d1 = delta_before - case.expected_delta_sbr_db;
        let d2 = delta_after - case.expected_delta_sbr_after_db;
        if d1.abs() > 1e-4 {
            case_diff = true;
            eprintln!(
                "  delta_sbr_before  fixture={:+.4}  σήμερα={:+.4}  Δ={:+.4}",
                case.expected_delta_sbr_db, delta_before, d1
            );
        }
        if d2.abs() > 1e-4 {
            case_diff = true;
            eprintln!(
                "  delta_sbr_after   fixture={:+.4}  σήμερα={:+.4}  Δ={:+.4}",
                case.expected_delta_sbr_after_db, delta_after, d2
            );
        }
        if !case_diff {
            eprintln!("  ΤΑΥΤΟΣΗΜΟ σε όλα τα πεδία (εντός 1e-4)");
        } else {
            any_unexplained = true;
        }
        eprintln!();
    }

    let _ = std::io::stderr().flush();
    eprintln!(
        "=== ΣΥΝΟΛΟ: {} ===",
        if any_unexplained {
            "ΥΠΑΡΧΟΥΝ ΔΙΑΦΟΡΕΣ — δες παραπάνω ανά περίπτωση"
        } else {
            "καμία διαφορά σε καμία περίπτωση"
        }
    );
}
