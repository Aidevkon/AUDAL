//! §5.1α κεφάλι/πλοκάμι: τα ACX πεδία του StoredLoudness έγιναν ουδέτερα
//! (input_delivery_*, output_delivery_*). Παλιά sidecars στον δίσκο
//! κουβαλούν ΤΑ ΠΑΛΙΑ ΚΛΕΙΔΙΑ — ο φρουρός εδώ είναι ότι διαβάζονται
//! ΤΑΥΤΟΣΗΜΑ με τα νέα (μοτίβο F-074/ec47696: serde alias, ποτέ migration).

use m0d::blob_store::StoredLoudness;

const COMMON: &str = r#"
    "integrated_lufs": -20.5, "true_peak_dbtp": -3.1, "lra": 4.2,
    "k_weighted": true, "ebu_r128_target_lufs": -23.0,
    "ebu_r128_compliant": false, "spotify_compliant": false,
    "youtube_compliant": false, "apple_music_compliant": false,
    "apple_podcasts_compliant": false, "broadcast_compliant": false,
    "tidal_compliant": false
"#;

#[test]
fn old_and_new_delivery_keys_deserialize_identically() {
    let old = format!(
        r#"{{{COMMON},
        "input_acx_sample_peak_db": -3.25,
        "input_acx_rms_db": -20.75,
        "input_acx_noise_floor_db": -64.5,
        "input_acx_quietest_window_start_frame": 96000,
        "input_acx_compliant": true,
        "output_acx_sample_peak_db": -3.11,
        "output_acx_rms_db": -20.40,
        "output_acx_noise_floor_db": -63.9,
        "output_acx_quietest_window_start_frame": 88200
        }}"#
    );
    let new = format!(
        r#"{{{COMMON},
        "input_delivery_peak_db": -3.25,
        "input_delivery_rms_db": -20.75,
        "input_delivery_noise_floor_db": -64.5,
        "input_delivery_quietest_window_start_frame": 96000,
        "input_acx_compliant": true,
        "output_delivery_peak_db": -3.11,
        "output_delivery_rms_db": -20.40,
        "output_delivery_noise_floor_db": -63.9,
        "output_delivery_quietest_window_start_frame": 88200
        }}"#
    );

    let a: StoredLoudness = serde_json::from_str(&old).expect("OLD keys must still parse");
    let b: StoredLoudness = serde_json::from_str(&new).expect("NEW keys must parse");

    // Και το ΠΡΩΤΟΤΥΠΟ όνομα, πριν καν το input_ prefix (f2c488c aliases):
    let ancient = format!(
        r#"{{{COMMON},
        "acx_sample_peak_db": -3.25, "acx_rms_db": -20.75,
        "acx_noise_floor_db": -64.5,
        "acx_quietest_window_start_frame": 96000,
        "acx_compliant": true }}"#
    );
    let c: StoredLoudness =
        serde_json::from_str(&ancient).expect("PRE-input_ keys must still parse");

    for (label, v) in [("old", &a), ("new", &b)] {
        assert_eq!(v.input_delivery_peak_db, Some(-3.25), "{label} input peak");
        assert_eq!(v.input_delivery_rms_db, Some(-20.75), "{label} input rms");
        assert_eq!(v.input_delivery_noise_floor_db, Some(-64.5), "{label} input floor");
        assert_eq!(
            v.input_delivery_quietest_window_start_frame,
            Some(96000),
            "{label} input window"
        );
        assert_eq!(v.input_acx_compliant, Some(true), "{label} compliant");
        assert_eq!(v.output_delivery_peak_db, Some(-3.11), "{label} output peak");
        assert_eq!(v.output_delivery_rms_db, Some(-20.40), "{label} output rms");
        assert_eq!(v.output_delivery_noise_floor_db, Some(-63.9), "{label} output floor");
        assert_eq!(
            v.output_delivery_quietest_window_start_frame,
            Some(88200),
            "{label} output window"
        );
        assert_eq!(v.delivery_profile, None, "{label} profile absent ⇒ None (κανόνας 5.2)");
    }

    assert_eq!(c.input_delivery_peak_db, Some(-3.25), "ancient input peak");
    assert_eq!(c.input_delivery_rms_db, Some(-20.75), "ancient input rms");
    assert_eq!(c.input_delivery_noise_floor_db, Some(-64.5), "ancient input floor");

    println!(
        "old ≡ new ≡ ancient:\n\
         \tinput  peak={:?} rms={:?} floor={:?} window={:?} compliant={:?}\n\
         \toutput peak={:?} rms={:?} floor={:?} window={:?}\n\
         \tdelivery_profile={:?}",
        a.input_delivery_peak_db,
        a.input_delivery_rms_db,
        a.input_delivery_noise_floor_db,
        a.input_delivery_quietest_window_start_frame,
        a.input_acx_compliant,
        a.output_delivery_peak_db,
        a.output_delivery_rms_db,
        a.output_delivery_noise_floor_db,
        a.output_delivery_quietest_window_start_frame,
        a.delivery_profile,
    );
}
