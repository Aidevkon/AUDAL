//! ΣΥΜΒΑΤΟΤΗΤΑ ΤΟΥ ΠΕΜΠΤΟΥ ΜΠΛΟΚ (`corrections`), 2026-09-13.
//!
//! Το σχήμα v0 πάγωσε 2026-08-23. Η προσθήκη είναι ΠΡΟΣΘΕΤΙΚΗ, και αυτό
//! αποδεικνύεται με ΤΡΕΞΙΜΟ, όχι με ισχυρισμό — ίδιο πρότυπο με το
//! delivery_field_alias_roundtrip.

use m0d::blob_store::{
    BlobVariant, CorrectionRecord, NamedValue, StoredBlobCore, StoredBlobV2, StoredLoudness,
    StoredProvenance, StoredQuality, StoredSpatial,
};

fn blob(corrections: Vec<CorrectionRecord>) -> StoredBlobV2 {
    StoredBlobV2 {
        core: StoredBlobCore {
            id: "corr-001".into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "2026-09-13T00:00:00Z".into(),
            input_path_hash: "aabb11223344".into(),
            input_pcm_sha256: Some("pcm-aabb11223344".into()),
            seed: 42,
            pipeline_version: "0.4.0".into(),
            schema_version: 1,
            preset_id: "acx".into(),
            pcm_blake3: Some("1234567890abcdef".into()),
            cert_signature: Some("sig_mock_12345".into()),
            audio_path: std::sync::Arc::new(lineos_types::audio::ManagedPcm::default()),
            sample_rate: 48000,
            channels: 2,
            num_frames: 96000,
        },
        variant: BlobVariant::Certified {
            loudness: StoredLoudness {
                integrated_lufs: -20.5,
                ..Default::default()
            },
            quality: StoredQuality::default(),
            provenance: StoredProvenance::default(),
            spatial: StoredSpatial::default(),
            stem_fingerprints: None,
            processing_timeline: vec![],
            dead_air: Default::default(),
            aether_cert: None,
            aether_persona: None,
            aether_config: None,
            qr_base64: None,
            corrections,
        },
    }
}

#[test]
fn old_sidecar_without_corrections_still_reads() {
    // ΤΟ «ΠΑΛΙΟ» JSON ΔΕΝ ΓΡΑΦΕΤΑΙ ΜΕ ΤΟ ΧΕΡΙ — ΠΑΡΑΓΕΤΑΙ.
    // Με κενό Vec το `skip_serializing_if` αφήνει το πεδίο ΕΞΩ, άρα το
    // αποτέλεσμα είναι ΑΚΡΙΒΩΣ το σχήμα που έγραφε ο κώδικας πριν από
    // σήμερα. Χειρόγραφο literal θα απεδείκνυε ό,τι υπέθεσε ο συγγραφέας.
    let json = serde_json::to_string_pretty(&blob(Vec::new())).expect("serialize");
    assert!(
        !json.contains("corrections"),
        "κενό corrections ΔΕΝ πρέπει να μπαίνει στο JSON — skip_serializing_if"
    );

    // Και διαβάζεται αμετάβλητο.
    let back: StoredBlobV2 = serde_json::from_str(&json).expect("παλιό sidecar πρέπει να διαβαστεί");
    match back.variant {
        BlobVariant::Certified { corrections, .. } => {
            assert!(corrections.is_empty(), "απόν πεδίο ⇒ κενό, μέσω serde(default)");
        }
        BlobVariant::Uncertified { .. } => panic!("περίμενα Certified"),
    }
}

#[test]
fn corrections_roundtrip_preserves_every_field() {
    // Ένα record με τα τέσσερα measurements του σταδίου που τρέχει σήμερα.
    // Οι τιμές είναι ΤΟΥ ΕΓΓΡΑΦΟΥ: το όριο και το edge από το preset, το
    // πάτωμα και το περιθώριο από τη μέτρηση των 20 αρχείων (13/09).
    let limit_db = lineos_types::presets::ACX
        .max_noise_floor_db
        .expect("ACX ορίζει max_noise_floor_db");
    let edge_sec = lineos_types::presets::ACX
        .room_tone_max_s
        .expect("ACX ορίζει room_tone_max_s");
    let floor_db = -68.549_18_f32; // swiss_family_robinson, μετρημένο 13/09

    let rec = CorrectionRecord {
        stage: "interior_noise_analysis".into(),
        state: "measured".into(),
        reason: String::new(),
        measurements: vec![
            NamedValue {
                name: "input_interior_floor_db".into(),
                value: floor_db,
                unit: "dBFS".into(),
            },
            NamedValue {
                name: "limit_db".into(),
                value: limit_db,
                unit: "dBFS".into(),
            },
            NamedValue {
                name: "margin_db".into(),
                value: limit_db - floor_db,
                unit: "dB".into(),
            },
            NamedValue {
                name: "edge_sec".into(),
                value: edge_sec,
                unit: "s".into(),
            },
        ],
    };

    let json = serde_json::to_string(&blob(vec![rec.clone()])).expect("serialize");
    assert!(json.contains("interior_noise_analysis"), "το στάδιο πρέπει να γράφεται");

    let back: StoredBlobV2 = serde_json::from_str(&json).expect("deserialize");
    match back.variant {
        BlobVariant::Certified { corrections, .. } => {
            assert_eq!(corrections.len(), 1, "μία εγγραφή");
            assert_eq!(corrections[0], rec, "κάθε πεδίο ταυτόσημο μετά το roundtrip");
            // Η ΣΥΜΒΑΣΗ ΤΟΥ ΠΡΟΣΗΜΟΥ: margin = limit − floor. Θετικό = καθαρό.
            let margin = corrections[0]
                .measurements
                .iter()
                .find(|m| m.name == "margin_db")
                .expect("margin_db");
            assert!(
                margin.value > 0.0,
                "πάτωμα −68.5 κάτω από όριο −60 ⇒ θετικό περιθώριο· μετρήθηκε {}",
                margin.value
            );
        }
        BlobVariant::Uncertified { .. } => panic!("περίμενα Certified"),
    }
}
