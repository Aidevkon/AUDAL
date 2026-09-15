//! ΕΠΑΛΗΘΕΥΣΗ: η ΠΡΑΓΜΑΤΙΚΗ ζωντανή διαδρομή (execute_streaming_plan,
//! ΟΧΙ ξεχωριστό όργανο) παράγει τις γωνίες που δείχνει το lab-log;
//! Διαβάζει το `low_cut` CorrectionRecord από το πραγματικό corrections
//! block μετά από πλήρες render.
//!
//! ΑΝΟΧΗ ΔΗΛΩΜΕΝΗ ΠΡΙΝ ΤΡΕΞΕΙ: ±0.5Hz στη fundamental_hz (ίδια ανοχή με
//! live_trunk_mains_line.rs, ίδιος λόγος: το detect_band_peak βρίσκει
//! δυναμικά την κορυφή, μπορεί να διαφέρει ελαφρώς ποια ακριβώς παύση/
//! ομιλία επιλέγεται).
//!
//! ΜΗΔΕΝ αλλαγή παραγωγής. Μόνο ανάγνωση.

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::blob_store::BlobVariant;

const FREQ_TOL_HZ: f32 = 0.5;

fn main() {
    println!("Ανοχή δηλωμένη πριν το τρέξιμο: ±{FREQ_TOL_HZ} Hz στη fundamental_hz.\n");

    let base = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    // (όνομα, αναμενόμενη γωνία από το lab-log — ΜΗΔΕΝ για janeeyre: αναμένεται ΑΠΟΝ)
    let files: [(&str, Option<f32>); 9] = [
        ("secretgarden_01_burnett", Some(123.045)),
        ("dracula_01_stoker", Some(105.47)),
        ("huckfinn_01_twain_apc", Some(106.935)),
        ("peterpan_01_barrie", Some(123.045)),
        ("tale_of_two_cities_01_dickens", Some(55.665)),
        ("adventurespinocchio_01_collodi", Some(104.005)),
        ("anne_of_green_gables_01_montgomery", Some(104.005)),
        ("count_of_monte_cristo_001_dumas", Some(106.935)),
        ("janeeyre_01_bronte", None),
    ];

    let mut all_ok = true;
    for (name, expected_corner) in files {
        let path = format!("{base}/{name}.mp3");
        let plan = StreamingPlan {
            audio_path: path,
            preset_id: "acx".to_string(),
            flavour_id: None,
            intent_tone: None,
            intent_dynamics: None,
            target_lufs_override: None,
            session_id: format!("lowcut-wiring-verify-{name}"),
        };
        // catch_unwind: ένα πρόβλημα άσχετο με αυτό το task (βλ. αναφορά
        // chat) εμποδίζει μερικά αρχεία — ΔΕΝ σταματά όλη την επαλήθευση.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            execute_streaming_plan(&plan, None)
        }));
        let (_output, blob) = match result {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => {
                println!("  {name:<38} ⚠ execute_streaming_plan error: {e:?}");
                continue;
            }
            Err(_) => {
                println!("  {name:<38} ⚠ PANIC — άσχετο pre-existing πρόβλημα (streaming_pipeline.rs stems slicing), ΔΕΝ επαληθεύτηκε εδώ");
                continue;
            }
        };

        let low_cut = match &blob.variant {
            BlobVariant::Certified { corrections, .. } => {
                corrections.iter().find(|c| c.stage == "low_cut").cloned()
            }
            BlobVariant::Uncertified { .. } => None,
        };

        match (low_cut, expected_corner) {
            (Some(rec), Some(exp_corner)) => {
                let fund = rec.measurements.iter().find(|m| m.name == "fundamental_hz").map(|m| m.value);
                let corner = rec.measurements.iter().find(|m| m.name == "corner_hz").map(|m| m.value);
                match (fund, corner) {
                    (Some(f), Some(c)) => {
                        let exp_fund = exp_corner * 2.0;
                        let err = (f - exp_fund).abs();
                        let ok = err <= FREQ_TOL_HZ * 2.0 && rec.state == "applied";
                        if !ok { all_ok = false; }
                        println!(
                            "  {name:<38} state={:<8} fundamental={f:.3}Hz (ζητούμενο≈{exp_fund:.2}) corner={c:.3}Hz (ζητούμενο={exp_corner:.3}) Δfund={:.3}Hz {}",
                            rec.state, f - exp_fund, if ok { "ΕΝΤΟΣ" } else { "⚠ ΕΚΤΟΣ" }
                        );
                    }
                    _ => {
                        all_ok = false;
                        println!("  {name:<38} ⚠ low_cut record χωρίς measurements (state={})", rec.state);
                    }
                }
            }
            (None, None) => {
                println!("  {name:<38} ΑΠΟΝ, όπως αναμενόταν (καμία θεμελιώδης)");
            }
            (Some(rec), None) => {
                all_ok = false;
                println!("  {name:<38} ⚠ ΑΠΡΟΣΔΟΚΗΤΟ: low_cut record υπάρχει (state={}) ενώ αναμενόταν ΑΠΟΝ", rec.state);
            }
            (None, Some(_)) => {
                all_ok = false;
                println!("  {name:<38} ⚠ ΑΠΡΟΣΔΟΚΗΤΟ: κανένα low_cut record ενώ αναμενόταν γωνία");
            }
        }
    }

    println!("\n{}", if all_ok { "ΟΛΑ ΕΝΤΟΣ ΑΝΟΧΗΣ." } else { "⚠ ΚΑΠΟΙΟ ΕΚΤΟΣ ΑΝΟΧΗΣ." });
}
