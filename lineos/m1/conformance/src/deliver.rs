//! run_deliver_core and its own value types — moved here 21/09
//! (F-137/PRD §6.1). `sanitize_title`/`validate_and_plan` stay in
//! m0-daemon's handlers/deliver.rs: they take `crate::db::schema::
//! Track`, a DB-adjacent type, and run_deliver_core itself never
//! calls either — it only consumes the already-built `DeliveryPlan`.
//! `apply_sidecar_updates`/`write_manifest_file`/`post_deliver_plan`/
//! `post_deliver` also stay (axum handlers, or need blob_store's
//! find_sidecar/write_sidecar, which stays per F-129).

use lineos_types::audio::ManagedPcm;
use lineos_types::certificate::{
    BlobVariant, StoredBlobCore, StoredBlobV2, UncertifiedReason,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct DeliverRequest {
    pub output_dir: Option<String>,
    pub book_title: String,
    pub entries: Vec<DeliverEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeliverEntry {
    pub track_id: String,
    pub index: u32,
    pub title: String,
    pub role: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PlanEntry {
    pub index: u32,
    pub title: String,
    pub role: String,
    pub filename: String,
    pub duration_ms: u64,
    pub exists: bool,
    #[serde(skip)]
    pub track_id: String,
    #[serde(skip)]
    pub audio_path: String,
    #[serde(skip)]
    pub resolved_blob: Option<StoredBlobV2>,
}

#[derive(Debug, Serialize)]
pub struct DeliveryPlan {
    pub book_title_sanitized: String,
    pub entries: Vec<PlanEntry>,
}

#[derive(Debug, Serialize)]
pub struct DeliverResponse {
    pub book_dir: Option<String>,
    pub files: Vec<String>,
    pub manifest_path: Option<String>,
    pub warnings: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Plan endpoint only: the per-entry preview (index, title, role,
    /// filename, duration, exists). None on actual delivery responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<PlanEntry>>,
    /// The declaration, as a value. `run_deliver_core` builds it but does not
    /// write it — the caller writes `manifest.json` from this, once it has
    /// merged in any sidecar warnings. Not wire format: never sent to HTTP
    /// clients.
    #[serde(skip)]
    pub manifest: Option<Manifest>,
    /// Per-entry certificate sidecar updates the core computed but did not
    /// write — the caller performs the `find_sidecar`/`write_sidecar` pair
    /// (storage-layer work, not the core's).
    #[serde(skip)]
    pub sidecar_updates: Vec<SidecarUpdate>,
}

/// A certified blob's loudness fields, updated with this delivery's
/// measurements, plus the id needed to look up (and rewrite) its sidecar.
/// The core computes this as a value; only the caller with masters_dir/
/// project_id in scope can turn it into a disk write.
#[derive(Debug)]
pub struct SidecarUpdate {
    pub track_id: String,
    pub blob_id: String,
    pub updated_blob: StoredBlobV2,
}

#[derive(Debug, Serialize)]
pub struct ManifestEntry {
    pub index: u32,
    pub title: String,
    pub role: String,
    pub filename: String,
    pub duration_ms: u64,
    pub sample_peak_db: f32,
    pub rms_db: f32,
    pub noise_floor_db: f32,
    /// Η ΣΥΝΟΛΙΚΗ ΕΤΥΜΗΓΟΡΙΑ ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ — fold πάνω σε ΟΛΕΣ τις
    /// γραμμές (επίπεδα · spacing · μορφή).
    ///
    /// ⚠ ΜΕΤΟΝΟΜΑΣΙΑ ΑΠΟ `passes_acx`, 2026-08-25 — ΟΧΙ κοσμητική.
    /// Το `passes_acx` σήμαινε **ΜΟΝΟ ΕΠΙΠΕΔΑ**: ήταν
    /// `AcxCheckReport::levels_within_limits_with_margin()`, που τρέχει πάνω σε
    /// rms/peak/noise_floor και **δεν γνωρίζει spacing ούτε μορφή**.
    /// Να του δώσουμε τη ΝΕΑ, πλήρη σημασία κρατώντας το όνομα θα
    /// σήμαινε ότι κάθε παλιό manifest λέει **άλλο πράγμα με το ίδιο
    /// κλειδί** — ακριβώς η παγίδα του F-074 (`input_hash`).
    ///
    /// ⚠ ΤΟ serde alias ΘΑ ΗΤΑΝ ΑΔΡΑΝΕΣ: το `ManifestEntry` είναι
    /// `Serialize` ΜΟΝΟ — κανείς δεν το αποσειριοποιεί σε αυτό το
    /// δέντρο. Η επιφάνεια συμβατότητας είναι **εξωτερικοί αναγνώστες
    /// του manifest.json**, και γι' αυτούς αλλάζει το σχήμα, όχι ένα
    /// alias. Καταγράφεται στο certificate-schema-v0.md.
    ///
    /// ΕΝΑ πεδίο, όχι δύο: μόλις υπάρχει η πλήρης ετυμηγορία, η μερική
    /// δεν έχει καταναλωτή.
    pub delivery_verdict: crate::blob_paths::DeliveryVerdict,
    pub head_quiet_secs: f32,
    pub tail_quiet_secs: f32,
    /// Η ΚΡΙΣΗ του spacing, §5.3 σχήμα — ΑΝΤΙΚΑΤΕΣΤΗΣΕ τα τέσσερα
    /// `warnings.push(format!(...))` της 25/08.
    ///
    /// ΓΙΑΤΙ ΕΔΩ ΚΑΙ ΟΧΙ ΜΟΝΟ ΣΤΟ SIDECAR: η επανεγγραφή του sidecar
    /// είναι ΥΠΟ ΟΡΟΥΣ (θέλει resolved_blob + masters_dir + project_id)·
    /// το manifest γράφεται ΠΑΝΤΑ. Χωρίς αυτό το πεδίο, η αφαίρεση των
    /// warnings θα άφηνε τη διαδρομή fallback ΧΩΡΙΣ καμία κρίση
    /// spacing — κενό που έπιασε το `test_deliver_e2e_small`.
    pub spacing_checks: Vec<lineos_types::certificate::DeliveryCheck>,
}

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub book_title: String,
    pub generated_at: String,
    pub entries: Vec<ManifestEntry>,
    pub rms_spread_db: f32,
    pub warnings: Vec<String>,
}

/// ΒΗΜΑ 5/7: αυτό το blob είναι ΜΕΤΑΦΟΡΕΑΣ, όχι source
/// of truth. Η export_mp3_acx διαβάζει ΜΟΝΟ audio_path
/// και channels· το manifest.json γεμίζει από
/// πραγματικές μετρήσεις (outcome.report).
fn build_minimal_blob(audio_path: &str) -> StoredBlobV2 {
    let blob_v2 = StoredBlobV2 {
        core: StoredBlobCore {
            // ΠΡΑΓΜΑΤΙΚΑ (διαβάζονται από export_mp3_acx):
            audio_path: Arc::new(ManagedPcm::new(std::path::PathBuf::from(audio_path))),
            channels: 2,
            // ευθυγράμμιση με βήματα 1-4. ΗΤΑΝ 1.
            // northstar: ΣΠΑΕΙ ΧΩΡΙΣ MIGRATION μέχρι το πρώτο public release.
            schema_version: 0,
            pcm_blake3: None,
            cert_signature: None,
            // ΨΕΥΔΗ, αλλά ΔΕΝ διαβάζονται από κανέναν σήμερα.
            // preset_id="acx" ό,τι κι αν ζήτησε ο χρήστης,
            // id="delivery" για κάθε track. Διορθώνονται στο
            // ΜΗΤΡΩΟ, όπου το preset_id σπάει σε
            // delivery_target / flavour / routing_mode ούτως ή
            // άλλως. northstar §3.
            id: "delivery".into(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: "".into(),
            input_path_hash: "".into(),
            input_pcm_sha256: None,
            seed: 1,
            pipeline_version: "".into(),
            preset_id: "acx".into(),
            sample_rate: 48000,
            num_frames: 0,
        },
        variant: BlobVariant::Uncertified {
            reason: UncertifiedReason::TransportOnlyNotASource,
        },
    };
    blob_v2
}

/// Ο πυρήνας γράφει ήχο, όχι χαρτί (DECISIONS.md §7, απόφαση 3): η
/// δηλωμένη κατάσταση — το `Manifest` και τα per-entry sidecar-update
/// δεδομένα — επιστρέφει ΩΣ ΤΙΜΗ μέσα στο `DeliverResponse`
/// (`manifest`, `sidecar_updates`). Ο καλών γράφει το `manifest.json`
/// (`write_manifest_file`) και ξαναγράφει το sidecar
/// (`apply_sidecar_updates`) — και οι δύο, storage-layer, όχι πυρήνας.
/// `book_dir`/τα mp3 (το ίδιο το παραδοτέο) γράφονται εδώ, αμετάβλητα.
pub fn run_deliver_core(
    req: &DeliverRequest,
    plan: DeliveryPlan,
) -> Result<DeliverResponse, Vec<String>> {
    let out_dir_str = match &req.output_dir {
        Some(d) => d,
        None => return Err(vec!["output_dir is required for deliver".into()]),
    };

    let book_dir = std::path::PathBuf::from(out_dir_str).join(&plan.book_title_sanitized);
    if let Err(e) = std::fs::create_dir_all(&book_dir) {
        return Err(vec![format!("failed to create book dir: {}", e)]);
    }

    let mut manifest_entries = Vec::new();
    let mut warnings = Vec::new();
    let mut files = Vec::new();
    let mut sidecar_updates = Vec::new();

    for plan_entry in &plan.entries {
        if !plan_entry.exists {
            return Err(vec![format!(
                "PCM file for track {} does not exist at {}",
                plan_entry.track_id, plan_entry.audio_path
            )]);
        }

        let blob = match &plan_entry.resolved_blob {
            Some(b) => b.clone(),
            None => build_minimal_blob(&plan_entry.audio_path),
        };
        let final_path = book_dir.join(&plan_entry.filename);

        let outcome = match crate::export::export_mp3_acx(&blob, &final_path) {
            Ok(o) => o,
            Err(e) => {
                return Err(vec![format!(
                    "export_mp3_acx failed for track {}: {}",
                    plan_entry.track_id, e
                )])
            }
        };

        // ΟΙ ΓΡΑΜΜΕΣ ΜΙΑ ΦΟΡΑ, ΑΝΕΥ ΟΡΩΝ — τις μοιράζονται το sidecar
        // (που γράφεται ΥΠΟ ΟΡΟΥΣ) και το manifest (που γράφεται ΠΑΝΤΑ).
        // Υπολογίζονταν μέσα στο υπό-όρους μπλοκ· έτσι η ετυμηγορία του
        // manifest θα ήταν χτισμένη σε ΑΛΛΟ σύνολο γραμμών από του
        // sidecar. Ένα σύνολο, δύο αναγνώστες.
        //
        // ΤΟ ΣΥΜΒΟΛΑΙΟ: `presets::ACX` ΡΗΤΑ — αυτή η διαδρομή καλεί
        // `export_mp3_acx` ΑΝΕΥ ΟΡΩΝ και το `build_minimal_blob` βάζει
        // `preset_id: "acx"` ό,τι κι αν ζήτησε ο χρήστης.
        let spec = &lineos_types::presets::ACX;
        let mut entry_checks =
            crate::declare::delivery_checks_from_margin_checks(&outcome.report);
        entry_checks.extend(lineos_types::certificate::DeliveryCheck::from_spacing(
            spec,
            outcome.head_quiet_secs,
            outcome.tail_quiet_secs,
        ));
        entry_checks.extend(lineos_types::certificate::DeliveryCheck::from_format(
            spec,
            outcome.delivered_sample_rate,
            outcome.delivered_channels,
            outcome.delivered_bitrate_kbps,
        ));

        // §5.6 Δ2, 2026-08-22: το `blob` πιο πάνω είναι ΚΛΩΝΟΣ του
        // resolved_blob (ΒΗΜΑ 0 recon) — γράφοντας output_acx_* πάνω
        // του δεν αγγίζει το sidecar στον δίσκο. Η εύρεση
        // (`find_sidecar`) και η επανεγγραφή (`write_sidecar`) είναι
        // ΚΑΙ ΟΙ ΔΥΟ storage-layer — μετακινήθηκαν στον καλούντα
        // (`apply_sidecar_updates`), που έχει masters_dir/project_id.
        // Ο πυρήνας εδώ φτιάχνει μόνο την ΤΙΜΗ: το ενημερωμένο blob,
        // ΜΟΝΟ όταν υπάρχει πραγματικό resolved_blob (άρα υπαρκτό
        // sidecar να ξαναγραφεί)· χωρίς αυτό (π.χ. τεστ με
        // build_minimal_blob fallback) δεν παράγεται SidecarUpdate —
        // ΔΕΝ υπάρχει τι να ξαναγραφεί.
        if plan_entry.resolved_blob.is_some() {
            let mut updated = blob.clone();
            if let BlobVariant::Certified { loudness, .. } = &mut updated.variant {
                loudness.output_delivery_peak_db = Some(outcome.report.sample_peak_db);
                loudness.output_delivery_rms_db = Some(outcome.report.rms_db);
                loudness.output_delivery_noise_floor_db = outcome.report.noise_floor_db;
                loudness.output_delivery_quietest_window_start_frame =
                    outcome.report.quietest_window_start_frame;
                // §5.3 — η κρίση μπαίνει στο certificate εδώ, μαζί με
                // τα κατώφλια που χρησιμοποίησε (ίδια λογική με
                // levels_within_limits_with_margin(), sp314_dsp::analysis::
                // acx_check::AcxCheckReport::margin_checks() — μία
                // υλοποίηση, δύο καλούντες). Απουσία μετρικής (π.χ.
                // noise_floor όταν το αρχείο < 1s) = καμία εγγραφή.
                // 2026-08-25: η αντιστοίχιση βγήκε στο
                // DeliveryCheck::from_margin_checks — δεύτερος
                // καλών (/export) τη χρειάζεται, και inline θα
                // σήμαινε δύο αντίγραφα. Ίδιες τιμές, ίδιος κανόνας.
                // 2026-08-25: ΚΑΙ το spacing, από τον ΙΔΙΟ
                // παραγωγό με το /export (from_spacing) — μία
                // υλοποίηση, δύο καλούντες. Χωριστά από τα
                // margin_checks ώστε το "advisory" να μη φτάνει
                // ποτέ στη συνολική κρίση.
                loudness.delivery_checks = Some(entry_checks.clone());
            }
            sidecar_updates.push(SidecarUpdate {
                track_id: plan_entry.track_id.clone(),
                blob_id: blob.core.id.clone(),
                updated_blob: updated,
            });
        }

        files.push(plan_entry.filename.clone());
        manifest_entries.push(ManifestEntry {
            index: plan_entry.index,
            title: plan_entry.title.clone(),
            role: plan_entry.role.clone(),
            filename: plan_entry.filename.clone(),
            duration_ms: plan_entry.duration_ms,
            sample_peak_db: outcome.report.sample_peak_db,
            rms_db: outcome.report.rms_db,
            noise_floor_db: outcome.report.noise_floor_db.unwrap_or_default(),
            // Margin-adjusted (F-077 encoder gap) — this report is the exact
            // pre-LAME buffer from export_mp3_acx, so the measured gap
            // applies here. NOT the same call as certificate_node.rs's
            // input_acx_compliant, which measures raw input pre-render/
            // pre-encode and must stay on the nominal levels_within_limits().
            // ΤΟ FOLD, όχι το μερικό. Το `levels_within_limits_with_margin()`
            // ΔΕΝ αγγίχτηκε — παραμένει σωστό για ό,τι καλύπτει και
            // τροφοδοτεί τις γραμμές `rms`/`peak`/`noise_floor` μέσω
            // του `from_margin_checks`. Απλώς έπαψε να είναι η ΤΕΛΙΚΗ
            // κρίση: είναι ΕΝΑΣ παραγωγός ανάμεσα σε τρεις.
            delivery_verdict: crate::blob_paths::DeliveryVerdict::compose(
                &lineos_types::presets::ACX,
                &entry_checks,
            ),
            head_quiet_secs: outcome.head_quiet_secs,
            tail_quiet_secs: outcome.tail_quiet_secs,
            // ΤΟ ΙΔΙΟ σύνολο γραμμών, φιλτραρισμένο — όχι δεύτερη κλήση
            // του παραγωγού. Δύο κλήσεις θα μπορούσαν να αποκλίνουν.
            spacing_checks: entry_checks
                .iter()
                .filter(|c| c.metric.ends_with("_spacing"))
                .cloned()
                .collect(),
        });
    }

    let mut min_rms = f32::MAX;
    let mut max_rms = f32::MIN;
    for e in &manifest_entries {
        if e.role == "chapter" {
            if e.rms_db < min_rms {
                min_rms = e.rms_db;
            }
            if e.rms_db > max_rms {
                max_rms = e.rms_db;
            }
        }
    }
    let rms_spread = if min_rms <= max_rms {
        max_rms - min_rms
    } else {
        0.0
    };
    if rms_spread > 4.0 {
        warnings.push(format!(
            "RMS spread across chapters is {:.1} dB (> 4.0 dB)",
            rms_spread
        ));
    }

    // ΤΟ SPACING ΕΦΥΓΕ ΑΠΟ ΤΑ warnings — 2026-08-25.
    //
    // ΗΤΑΝ: τέσσερα `warnings.push(format!(...))` με ελεύθερο κείμενο.
    // ΕΙΝΑΙ: τέσσερις εγγραφές `DeliveryCheck` (head/tail x
    // ΑΠΑΙΤΗΣΗ/ΣΥΣΤΑΣΗ) από τον `DeliveryCheck::from_spacing`, παραπάνω.
    //
    // ΓΙΑΤΙ ΕΦΥΓΑΝ ΑΝΤΙ ΝΑ ΜΕΙΝΟΥΝ ΔΙΠΛΑ: ελεύθερο κείμενο δεν
    // ταξιδεύει — δεν φτάνει στο /export, δεν μπαίνει στο sidecar, δεν
    // διαβάζεται από μηχανή. Κρατώντας ΚΑΙ τα δύο, το ίδιο γεγονός θα
    // λεγόταν σε δύο μορφές που μπορούν να αποκλίνουν: ακριβώς το
    // μοτίβο των «τριών αγκυρών» που έχει ήδη κοστίσει.
    //
    // Η ΜΕΤΡΗΣΗ δεν χάνεται σε καμία περίπτωση: το `ManifestEntry`
    // κουβαλάει `head_quiet_secs`/`tail_quiet_secs` αυτούσια, και όταν
    // η επανεγγραφή του sidecar παραλείπεται, ΑΥΤΟ ήδη προειδοποιείται
    // ρητά («no existing sidecar found ... certificate rewrite
    // skipped»). Ο μετρητής καταγράφει· ο ερμηνευτής μεταφράζει.

    let roles: std::collections::HashSet<&str> =
        manifest_entries.iter().map(|e| e.role.as_str()).collect();
    if !roles.contains("opening_credits") {
        warnings.push("missing opening_credits".into());
    }
    if !roles.contains("closing_credits") {
        warnings.push("missing closing_credits".into());
    }
    if !roles.contains("retail_sample") {
        warnings.push("missing retail_sample".into());
    }

    let manifest = Manifest {
        schema_version: 1,
        book_title: req.book_title.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        entries: manifest_entries,
        rms_spread_db: rms_spread,
        warnings: warnings.clone(),
    };

    // ΔΕΝ γράφεται εδώ πια — η δήλωση μένει τιμή. Ο καλών γράφει το
    // αρχείο μέσω `write_manifest_file`, αφού πρώτα εφαρμόσει
    // (ή όχι) τα `sidecar_updates` — τα warnings τους πρέπει να
    // προλάβουν να μπουν στο `manifest.warnings` πριν τη σειριοποίηση.
    let manifest_path = book_dir.join("manifest.json");

    Ok(DeliverResponse {
        book_dir: Some(book_dir.to_string_lossy().into_owned()),
        files,
        manifest_path: Some(manifest_path.to_string_lossy().into_owned()),
        warnings,
        errors: vec![],
        entries: None,
        manifest: Some(manifest),
        sidecar_updates,
    })
}
