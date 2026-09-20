//! execute_streaming_plan — moved here 21/09 (F-137/PRD §6.1): the
//! engine's own function, formerly agents/executor.rs in m0-daemon.
//! Returns the UNSIGNED certificate as a value (§7 απόφαση 3) —
//! callers (agents/executor.rs's `run()`, agents/batch.rs, both
//! staying in m0-daemon) build a `CertificateOutput` from the
//! returned blob and call `certificate_node::sign_and_render` on it,
//! since identity/signing/QR/PDF stay with the server.

use lineos_types::streaming::{ExecutorError, StreamingOutput, StreamingPlan};

/// Synchronous streaming DSP execution for a StreamingPlan.
/// pub for run_batch (§Β)
pub fn execute_streaming_plan(
    plan: &StreamingPlan,
    progress_tx: Option<tokio::sync::broadcast::Sender<crate::mastering_progress::MasteringProgress>>,
) -> Result<(StreamingOutput, lineos_types::certificate::StoredBlobV2), ExecutorError> {
    let audio_path = plan.audio_path.clone();
    let job_id = plan.session_id.clone();
    let blob_id = uuid::Uuid::new_v4().to_string();
    let output_path = crate::spool::spool_dir()
        .join(format!("m0d-v3-streaming-{}.wav", blob_id))
        .to_string_lossy()
        .into_owned();
    let raw_tap_path = crate::blob_paths::raw_dump_path(&blob_id)
        .to_string_lossy()
        .into_owned();
    let raw_guard = std::sync::Arc::new(lineos_types::audio::ManagedPcm::new(
        std::path::PathBuf::from(&raw_tap_path),
    ));

    let path_hash = crate::dsp_pipeline_helpers::compute_sha256_bytes(audio_path.as_bytes());
    let input_hash_hex_path = hex::encode(path_hash);
    let seed = crate::dsp_pipeline_helpers::derive_seed(&path_hash);
    let cert_start = std::time::Instant::now();

    let mut profiler = crate::timeline::TimelineProfiler::new();
    let path = std::path::Path::new(&audio_path);

    // ΕΝΑ object, δύο πεδία: το target_lufs δέχεται override από το plan,
    // το ταβάνι ΟΧΙ — έρχεται πάντα από το spec του preset.
    let delivery_target = lineos_types::config::LoudnessTarget::from_preset(&plan.preset_id);
    let target_lufs = plan
        .target_lufs_override
        .unwrap_or(delivery_target.target_lufs);
    // Το όριο πατώματος ΔΕΝ ζει στο LoudnessTarget (config.rs:9-14 — τέσσερα
    // πεδία, κανένα δικό του). Ζει στο DeliverySpec, και η μόνη διαδρομή προς
    // τα εκεί είναι το μητρώο. Ίδιο ιδίωμα με το `wants_acx`
    // (dsp_pipeline.rs:624). None και για άγνωστο preset και για preset χωρίς
    // απαίτηση — και οι δύο σημαίνουν «κανένα όριο εδώ».
    let delivery_max_noise_floor_db = lineos_types::presets::lookup(&plan.preset_id)
        .and_then(|e| e.delivery.max_noise_floor_db);
    let (metrics, p0_decoder) = crate::input_lufs::pass0_decode_to_dump(path, &raw_tap_path)
        .map_err(ExecutorError::DspFailed)?;
    let input_lufs = metrics.integrated_lufs;

    if let Some(ref tx) = progress_tx {
        let _ = tx.send(crate::mastering_progress::MasteringProgress {
            job_id: job_id.clone(),
            stage: "ANALYZING".to_string(),
            elapsed_ms: 0,
            blob_id: None,
            error: None,
            bpm: Some(metrics.bpm),
        });
    }

    let pre_gain_linear = match input_lufs {
        Some(measured) => {
            let result = sp314_dsp::pipeline::autotune::autotune(measured, target_lufs);
            libm::powf(10.0, result.pre_gain_db / 20.0)
        }
        None => 1.0,
    };

    // Ο analyzer τρέχει ΜΟΝΟ όταν ο προορισμός δηλώνει όριο πατώματος. Κόστος
    // για όποιον δεν το δηλώνει: μηδέν — ίδιο δόγμα με το `wants_acx` του
    // episode μονοπατιού (dsp_pipeline.rs:624).
    let delivery_edge_sec = lineos_types::presets::lookup(&plan.preset_id)
        .and_then(|e| e.delivery.room_tone_max_s);

    let trunk_report = if delivery_max_noise_floor_db.is_some() {
        // ΚΑΝΕΝΑ unwrap_or: αν ο προορισμός ζητά πάτωμα αλλά δεν λέει πόσο
        // room tone επιτρέπει, το DeliverySpec είναι ασυνεπές και το λέμε.
        // Δεν συμβαίνει σήμερα (μόνο το acx δηλώνει όριο, και δηλώνει και τα
        // δύο) — ο κλάδος υπάρχει για να μη γεννηθεί σιωπηλά αύριο.
        let edge_sec = delivery_edge_sec.ok_or_else(|| {
            ExecutorError::DspFailed(format!(
                "preset {} δηλώνει max_noise_floor_db αλλά όχι \
                 room_tone_max_s — ασυνεπές DeliverySpec",
                plan.preset_id
            ))
        })?;
        sp314_orchestrator::trunk_pass::run_trunk_pass_with_acx(
            std::path::Path::new(&raw_tap_path),
            false,
            edge_sec,
        )
    } else {
        sp314_orchestrator::trunk_pass::run_trunk_pass(
            std::path::Path::new(&raw_tap_path),
            false,
        )
    }
    .map_err(|e| ExecutorError::DspFailed(format!("trunk pass failed: {e}")))?;
    let boundaries = trunk_report.boundaries.clone();
    profiler.mark_stage_with_hash("Trunk Pass", String::new());

    let pre_analysis = trunk_report.to_pre_analysis(
        metrics.true_peak_dbtp,
        metrics.bpm,
        metrics.beats_ms.clone(),
        metrics.downbeats_ms.clone(),
        metrics.transients_ms.clone(),
    );

    let (left, right, sample_rate) =
        crate::lazy_reader::read_scout_sample(path, 30.0).ok_or_else(|| {
            ExecutorError::DspFailed("Failed to read 30s scout sample".into())
        })?;

    let scout_out = crate::scout_node::run(
        &left,
        &right,
        sample_rate,
        "default",
        plan.flavour_id.as_deref().unwrap_or("default"),
        &pre_analysis,
        false,
    )
    .map_err(|e| ExecutorError::DspFailed(format!("scout failed: {e}")))?;
    let streaming_features = scout_out.scout.features.clone();
    profiler.mark_stage_with_hash("Scout/PreAnalysis", String::new());

    let mut db =
        sp314_nodes::topology::DspTopologyBuilder::new("ducking_fallback_topology");
    let d_in = db.add_node("in", "Input", serde_json::json!({}));
    let d_gain = db.add_node(
        "duck_gain",
        "Gain",
        serde_json::json!({ "gain": 1.0, "glide_ms": 300.0 }),
    );
    let d_out = db.add_node("out", "Output", serde_json::json!({}));

    db.connect(&d_in, &d_gain);
    db.connect(&d_gain, &d_out);
    let ducking_topology = db.build();

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    // F-102: the shadow reader used to open the ORIGINAL file at its
    // native sample rate (`sample_rate` above is read_scout_sample's —
    // correct for the Scout, wrong here) while the streaming loop
    // indexed the resulting stems at the render's 48kHz. Reading the
    // SAME raw_tap_path dump the render itself reads (already
    // resampled by pass0_decode_to_dump) and passing that ONE rate to
    // both consumers keeps stems and the streaming index on one clock.
    let shadow_reader = sp314_orchestrator::decode_provider::DumpSeekProvider::new(
        &raw_tap_path,
        crate::stream_core::TARGET_SR,
    )
    .map_err(|e| ExecutorError::DspFailed(format!("Failed to open shadow reader: {}", e)))?;

    let _worker_handle = crate::nmf_worker::spawn(
        shadow_reader,
        crate::stream_core::TARGET_SR,
        rx_job,
        tx_res,
    );

    let (_, flagged_indices) = crate::nmf_worker::dispatch_all_jobs(
        &boundaries,
        &tx_job,
    );

    let render_decoder = sp314_orchestrator::decode_provider::DumpDecodeProvider::new(
        raw_tap_path.clone(),
    );
    profiler.mark_stage_with_hash("Decode Setup", String::new());

    let frames_written =
        sp314_orchestrator::streaming_pipeline::run_streaming_pipeline_with_timeline(
            &render_decoder,
            &output_path,
            &sp314_orchestrator::streaming_pipeline::StreamingConfig {
                topology: &ducking_topology,
                block_size: 1024,
                // F-102: same constant the shadow reader/nmf_worker got
                // above — one source for the rate stems are built AND
                // indexed at, not two numbers that happened to agree.
                sample_rate: crate::stream_core::TARGET_SR,
                ducking_node_id: "duck_gain",
                speech_gain: 1.0,
                music_gain: 0.501,
                pre_gain_linear,
                expected_output_frames: p0_decoder.expected_output_frames(),
                quietest_active_window_dbfs: trunk_report.quietest_active_window_dbfs,
                max_true_peak_db: delivery_target.max_true_peak_db,
                max_noise_floor_db: delivery_max_noise_floor_db,
                // Μόνο η τιμή· το frame δεν χρειάζεται στη διαδρομή του κόμβου.
                input_interior_floor_db: trunk_report
                    .acx_interior_noise_floor
                    .map(|(db, _frame)| db),
                // Το κατώφλι του expander: η τομή της κατανομής του ίδιου του
                // αρχείου, από το ΙΔΙΟ trunk pass. None = μη διμερής.
                quiet_window_split_dbfs: trunk_report.quiet_window_split_dbfs,
                // Η θεμελιώδης ομιλίας, ΙΔΙΟ trunk pass, αντίστροφο
                // κριτήριο παραθύρου από την παύση. Αποφασίζει τη γωνία
                // του low-cut στη λωρίδα hybrid.
                input_fundamental: trunk_report.input_fundamental,
                intent_dynamics: plan.intent_dynamics,
                restoration_enabled: true,
            },
            sp314_orchestrator::streaming_pipeline::TimelinePlan {
                boundaries,
                flagged_indices,
                pre_analysis: Some(&pre_analysis),
            },
            rx_res,
        )
        .map_err(|e| ExecutorError::DspFailed(format!("Streaming pipeline failed: {}", e)))?;
    profiler.mark_stage_with_hash("Streaming Render", String::new());

    let mastered_raw_path = crate::blob_paths::mastered_path(&blob_id);
    let mastered_guard = std::sync::Arc::new(lineos_types::audio::ManagedPcm::new(
        mastered_raw_path.clone(),
    ));
    let measured =
        crate::wav_to_raw::wav_to_raw_measured(&output_path, &mastered_raw_path)
            .map_err(|e| ExecutorError::DspFailed(format!("wav→raw post-pass failed: {e}")))?;
    profiler.mark_stage_with_hash("Verification Pass", measured.pcm_blake3.clone());

    if frames_written != measured.frames_written {
        return Err(ExecutorError::DspFailed(format!(
            "frame count mismatch: pipeline wrote {} but measured pass read {}",
            frames_written, measured.frames_written
        )));
    }

    let (_input_blake3, input_sha256) = p0_decoder.input_hashes();
    let dead_air = p0_decoder.into_dead_air();

    let icfg = crate::dsp_node::build_intent_and_config(
        &plan.preset_id,
        plan.flavour_id.as_deref().unwrap_or("default"),
        plan.intent_tone,
        plan.intent_dynamics,
        None,
        None,
        None,
        None,
        &streaming_features,
        &pre_analysis,
    )
    .map_err(ExecutorError::DspFailed)?;
    let (fingerprints, spatial_metadata) = {
        use crate::content_type::ContentTypeExt;
        crate::content_type::ContentType::bypassed_render()
    };
    profiler.mark_stage_with_hash("Certificate Assembly", String::new());
    // ── ΤΟ ΜΠΛΟΚ corrections ────────────────────────────────────────────
    // Χτίζεται ΕΔΩ γιατί εδώ ζει η γνώση του προορισμού. Ο κόμβος
    // πιστοποίησης συναρμολογεί, δεν αποφασίζει.
    //
    // ΤΟ ΣΤΑΔΙΟ ΠΟΥ ΓΡΑΦΕΤΑΙ ΕΙΝΑΙ ΑΥΤΟ ΠΟΥ ΤΡΕΧΕΙ: η ανάλυση. Ο κόμβος
    // διόρθωσης είναι γραμμένος αλλά ασύνδετος — ένα «applied» σε
    // υπογεγραμμένο έγγραφο θα ισχυριζόταν ενέργεια που δεν έγινε.
    //
    // ΚΕΝΟ όταν ο προορισμός δεν δηλώνει όριο: ο analyzer δεν έτρεξε
    // καθόλου, άρα δεν υπάρχει ανάλυση. Κενό ΔΕΝ είναι `absent`.
    let mut corrections: Vec<lineos_types::certificate::CorrectionRecord> =
        match (delivery_max_noise_floor_db, delivery_edge_sec) {
            (Some(limit_db), Some(edge_sec)) => {
                let mut measurements = Vec::new();
                let (state, reason) = match trunk_report.acx_interior_noise_floor {
                    Some((floor_db, _start_frame)) => {
                        measurements.push(lineos_types::certificate::NamedValue {
                            name: "input_interior_floor_db".into(),
                            value: floor_db,
                            unit: "dBFS".into(),
                        });
                        measurements.push(lineos_types::certificate::NamedValue {
                            name: "limit_db".into(),
                            value: limit_db,
                            unit: "dBFS".into(),
                        });
                        // ΣΥΜΒΑΣΗ ΠΡΟΣΗΜΟΥ: θετικό = το πάτωμα είναι ΚΑΤΩ από
                        // το όριο, καθαρό. Γραμμένη στο certificate-schema-v0.
                        measurements.push(lineos_types::certificate::NamedValue {
                            name: "margin_db".into(),
                            value: limit_db - floor_db,
                            unit: "dB".into(),
                        });
                        ("measured", String::new())
                    }
                    None => (
                        "absent",
                        "insufficient interior windows after edge exclusion".to_string(),
                    ),
                };
                measurements.push(lineos_types::certificate::NamedValue {
                    name: "edge_sec".into(),
                    value: edge_sec,
                    unit: "s".into(),
                });
                let mut records = vec![lineos_types::certificate::CorrectionRecord {
                    stage: "interior_noise_analysis".into(),
                    state: state.into(),
                    reason,
                    measurements,
                }];

                // ΔΕΥΤΕΡΗ ΕΓΓΡΑΦΗ: τι ΕΚΑΝΕ ο κόμβος, ξεχωριστά από τι
                // μετρήθηκε. Η συνθήκη έρχεται από ΤΗ ΜΙΑ πηγή
                // (streaming_pipeline::expander_threshold_db) — όχι αντίγραφο.
                // Το `skipped` είναι ΑΛΗΘΕΣ τώρα: ο κόμβος υπάρχει στην
                // αλυσίδα και η συνθήκη τον αφήνει ανενεργό. Κατάσταση, όχι
                // εκτίμηση.
                let interior_db = trunk_report.acx_interior_noise_floor.map(|(db, _)| db);
                let engaged = sp314_orchestrator::streaming_pipeline::expander_threshold_db(
                    delivery_max_noise_floor_db,
                    interior_db,
                    trunk_report.quiet_window_split_dbfs,
                );
                let (ex_state, ex_reason) = match engaged {
                    Some(_) => (
                        "applied",
                        "interior floor exceeds the destination limit",
                    ),
                    None if trunk_report.quiet_window_split_dbfs.is_none() => (
                        "skipped",
                        "the level distribution shows no room separate from the voice",
                    ),
                    None => (
                        "skipped",
                        "interior floor clears the destination limit",
                    ),
                };
                records.push(lineos_types::certificate::CorrectionRecord {
                    stage: "noise_expander".into(),
                    state: ex_state.into(),
                    reason: ex_reason.into(),
                    // ⚠ ΜΟΝΟ ΤΟ threshold_db. Τα `floor_db` (−12) και `ratio`
                    // (1.5) του expander είναι ΙΔΙΩΤΙΚΕΣ σταθερές του gate.rs
                    // χωρίς accessor. Γραμμένα εδώ ως literals θα ήταν ΔΕΥΤΕΡΟ
                    // αντίγραφο κατωφλιού μέσα σε ΥΠΟΓΕΓΡΑΜΜΕΝΟ έγγραφο — το
                    // ακριβές σχήμα που το ίδιο το schema entry απαγορεύει.
                    // Μπαίνουν όταν ο κόμβος τα εκθέσει· δηλωμένο κενό.
                    //
                    // ΤΟ `threshold_db` ΓΡΑΦΕΤΑΙ ΜΟΝΟ ΟΤΑΝ ΥΠΑΡΧΕΙ. Όταν ο
                    // κόμβος δεν τρέχει δεν υπάρχει κατώφλι να αναφερθεί, και
                    // ένα νούμερο-συμπλήρωμα σε υπογεγραμμένο έγγραφο θα ήταν
                    // ψέμα. «Κενό ≠ απόν» — το schema entry το ορίζει.
                    measurements: {
                        let mut m = Vec::new();
                        if let Some(t) = engaged {
                            m.push(lineos_types::certificate::NamedValue {
                                name: "threshold_db".into(),
                                value: t,
                                unit: "dBFS".into(),
                            });
                        }
                        if let Some(f) = interior_db {
                            m.push(lineos_types::certificate::NamedValue {
                                name: "interior_floor_db".into(),
                                value: f,
                                unit: "dBFS".into(),
                            });
                        }
                        m
                    },
                });
                records
            }
            _ => Vec::new(),
        };

    // ΤΡΙΤΗ ΕΓΓΡΑΦΗ, ΕΞΩ ΑΠΟ ΤΟ MATCH: ο ανιχνευτής βόμβου τρέχει σε ΚΑΘΕ
    // render (trunk_pass.rs, mains_line είναι πεδίο του TrunkReport,
    // υπολογίζεται ΑΝΕΞΑΡΤΗΤΑ από `with_acx`/`delivery_max_noise_floor_db`)
    // — άρα δεν έχει νόημα μέσα στο `(Some, Some) =>` παραπάνω. ΜΕΤΡΗΣΗ,
    // όχι κρίση: μηδέν κατώφλι, μηδέν «υπάρχει βόμβος».
    // ⚠ ΑΛΛΑΓΗ ΣΗΜΑΣΙΑΣ ΤΟΥ ΚΕΝΟΥ: πριν από αυτή την εγγραφή, άδειο
    // corrections σήμαινε «ο προορισμός δεν δηλώνει όριο, τίποτα δεν
    // έτρεξε». Από εδώ και πέρα το corrections ΔΕΝ είναι ΠΟΤΕ κενό.
    // certificate-schema-v0.md χρειάζεται γραμμή γι' αυτό — δεν γράφτηκε
    // εδώ, αναφέρεται ξεχωριστά.
    let (mh_state, mh_reason) = match (
        &trunk_report.mains_line,
        trunk_report.quiet_window_split_dbfs,
    ) {
        (Some(_), _) => ("measured", String::new()),
        // ΕΠΑΛΗΘΕΥΜΕΝΟ 2026-09-15: ο ίδιος ο ανιχνευτής (mains_hum.rs)
        // αρνείται ΜΟΝΟ κάτω από 48 δείγματα· η παύση που φτάνει εδώ
        // είναι πάντα ≥ QW_WINDOW (100ms @48kHz = 4800 δείγματα), άρα
        // αυτό το None ΔΕΝ έρχεται ποτέ από τον ανιχνευτή — έρχεται από
        // την ορχήστρα, και οι δύο περιπτώσεις παρακάτω είναι εξαντλητικές.
        (None, None) => (
            "absent",
            "the level distribution shows no room separate from the voice".to_string(),
        ),
        (None, Some(_)) => ("absent", "no pause long enough to measure".to_string()),
    };
    let mut mh_measurements = Vec::new();
    if let Some(line) = &trunk_report.mains_line {
        mh_measurements.push(lineos_types::certificate::NamedValue {
            name: "mains_frequency_hz".into(),
            value: line.hz,
            unit: "Hz".into(),
        });
        // Η προεξοχή ΕΞΑΡΤΑΤΑΙ ΑΠΟ ΤΟ ΟΡΓΑΝΟ.
        // ΜΕΤΡΗΘΗΚΕ 2026-09-14: το ίδιο δοκίμιο διαβάζεται 5.12 dB με
        // Welch N=16384 στα 48 kHz και 16.08 dB με αποδεκατισμό ×48 και
        // ένα FFT. Αυτό εδώ είναι το δεύτερο. Κάθε σύγκριση με νούμερο
        // άλλου οργάνου είναι άκυρη.
        // ΚΑΙ: πραγματικός φορέας χωρίς αντιληπτό βόμβο έδωσε 9.09 dB σε
        // αυτό το όργανο.
        mh_measurements.push(lineos_types::certificate::NamedValue {
            name: "prominence_db".into(),
            value: line.prominence_db,
            unit: "dB".into(),
        });
    }
    corrections.push(lineos_types::certificate::CorrectionRecord {
        stage: "mains_hum_analysis".into(),
        state: mh_state.into(),
        reason: mh_reason,
        measurements: mh_measurements,
    });

    // ΤΕΤΑΡΤΗ ΕΓΓΡΑΦΗ: το low-cut. state="applied" — ΟΧΙ "measured": ο
    // κόμβος ΕΝΕΡΓΕΙ (γωνία = θεμελιώδης/2, cleaner.rs's set_lowcut_corner),
    // πρώτη φορά που το λεξιλόγιο του σχήματος (0e91323: "applied — ο
    // κόμβος εκτελέστηκε και ενήργησε") το χρειάζεται αληθινά.
    // ΜΗΔΕΝ κατώφλι εδώ: το ρεκόρ λέει τι μετρήθηκε και τι έκανε ο
    // κόμβος, όχι κρίση ποιότητας.
    let (lc_state, lc_reason) = match &trunk_report.input_fundamental {
        Some(_) => ("applied", String::new()),
        None => ("absent", "the fundamental was not measured".to_string()),
    };
    let mut lc_measurements = Vec::new();
    if let Some(fund) = &trunk_report.input_fundamental {
        lc_measurements.push(lineos_types::certificate::NamedValue {
            name: "fundamental_hz".into(),
            value: fund.hz,
            unit: "Hz".into(),
        });
        lc_measurements.push(lineos_types::certificate::NamedValue {
            name: "corner_hz".into(),
            value: fund.hz / 2.0,
            unit: "Hz".into(),
        });
    }
    corrections.push(lineos_types::certificate::CorrectionRecord {
        stage: "low_cut".into(),
        state: lc_state.into(),
        reason: lc_reason,
        measurements: lc_measurements,
    });

    let cert_data = crate::certificate_node::StreamingCertData {
        pcm_blake3: measured.pcm_blake3.clone(),
        output_sha256: measured.output_sha256.clone(),
        dead_air,
        acx: None,
        corrections,
    };
    let cert_out = crate::certificate_node::run_streaming(
        &blob_id,
        measured.output_lufs,
        measured.output_lra,
        measured.true_peak_dbtp,
        &fingerprints,
        &spatial_metadata,
        &icfg.proof_log,
        &icfg.persona_config,
        &icfg.aether_req,
        &icfg.dsp_config,
        std::sync::Arc::new(lineos_types::audio::ManagedPcm::new(
            std::path::PathBuf::from(&output_path),
        )),
        &input_hash_hex_path,
        48_000,
        cert_start.elapsed().as_millis() as u64,
        seed,
        &plan.preset_id,
        input_sha256,
        measured.frames_written,
        profiler.finalize(),
        cert_data,
        None,
        // F-085: μετρήσεις ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ, από το ίδιο πέρασμα που
        // έδωσε output_lufs/true_peak. ΟΧΙ trunk_report — εκείνο
        // μέτρησε το raw_tap πριν τον render (:282 vs :355).
        measured.output_stereo_correlation,
        measured.output_dynamic_range_db,
        measured.output_rms_db,
        measured.output_stereo_width,
        measured.output_spectral_centroid,
        measured.output_spectral_flatness,
        measured.output_clips_detected,
    )
    .map_err(ExecutorError::DspFailed)?;
    // Signing/QR/PDF moved to sign_and_render (§7 απόφαση 3, F-137) —
    // identity.rs/handlers/certificate.rs/handlers/pdf_gen.rs stay
    // with the server, so the engine cannot call it. The caller
    // (agents/executor.rs's `run()`, agents/batch.rs — both staying)
    // builds a CertificateOutput from the returned blob and calls it.

    Ok((
        StreamingOutput {
            job_id,
            blob_id,
            status: "certified",
            pcm_data: Some(mastered_guard),
            num_frames: measured.frames_written,
            sample_rate: 48_000,
            pcm_blake3: measured.pcm_blake3,
            output_lufs: measured.output_lufs,
            raw_pcm_data: Some(raw_guard),
        },
        cert_out.blob,
    ))
}
