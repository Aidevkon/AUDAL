//! certificate_node — telemetry, certificate, StoredBlob assembly.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P5
//!
//! `run`/`run_streaming`/`assemble_blob` moved here 21/09 (§7 απόφαση
//! 3, F-137) — the unsigned declaration as a value. `sign_and_render`
//! (identity, sign_certificate, generate_qr_base64,
//! generate_silent_certificate — the three axum/key-adjacent files)
//! stays in m0-daemon's domain/nodes/certificate_node.rs, and is
//! called from here up: execute_streaming_plan (this crate) after
//! `run_streaming`, and run_dsp_internal (m0-daemon, staying) after
//! `run`/`run_streaming`, via full path.

use lineos_types::certificate::{StageRecord, StemFingerprints};
use chrono::Utc;
use lineos_telemetry::lra::LraCalculator;
use lineos_telemetry::windows::{momentary_lufs, short_term_lufs};
use lineos_types::pre_analysis::PreAnalysisData;

pub struct CertificateOutput {
    pub blob: lineos_types::certificate::StoredBlobV2,
    pub file_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    blob_id: &str,
    lufs: f32,
    true_peak: f32,
    // F-085: ΔΕΝ χρησιμοποιείται για το quality block — περιγράφει την
    // ΕΙΣΟΔΟ (χτίζεται από trunk_metrics ΠΡΙΝ τον render,
    // dsp_pipeline.rs:662/1036). Τα quality πεδία μετρώνται στα
    // post-master left_slice/right_slice παρακάτω.
    _pre_analysis: &PreAnalysisData,
    fingerprints: &StemFingerprints,
    spatial_metadata: &sp314_dsp::stft::two_pass::RenderMetadata,
    proof_log: &integration::proof_log::ProofLog,
    persona_config: &aether::personas::config::PersonaConfig,
    aether_req: &aether_bridge::AetherRequest,
    dsp_config: &integration::config::DspConfig,
    left_slice: &[f32],
    right_slice: &[f32],
    file_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
    input_hash_hex: &str,
    _original_sr: u32,
    _original_ch: u16,
    _duration_ms: u64,
    _target_lufs: Option<f32>,
    sample_rate: u32,
    elapsed_ms: u64,
    seed: u64,
    preset_id: &str,
    input_pcm_hash: String,
    n_total: usize,
    processing_timeline: Vec<StageRecord>,
    folddown_gain_db: Option<f32>,
    // F-070: ΜΕΤΡΗΜΕΝΟ stereo RMS (dB) από το render pass του Music.
    // None = δεν μετρήθηκε (μελλοντικοί callers) ⇒ fallback lufs+3.0.
    stereo_rms_measured: Option<f32>,
) -> Result<CertificateOutput, String> {
    // ── Phase 9: Telemetry pass — real LRA + windowed LUFS ───────────────────
    let mut post_master_samples = Vec::with_capacity(n_total * 2);
    for i in 0..n_total {
        post_master_samples.push(left_slice[i]);
        post_master_samples.push(right_slice[i]);
    }
    let post_master_sr = sample_rate;
    let post_master_channels = 2_u16;

    let telemetry_lra = {
        let mut calc = LraCalculator::new(post_master_sr);
        calc.feed_samples(&post_master_samples, post_master_channels);
        calc.compute()
    };

    let telemetry_momentary =
        momentary_lufs(&post_master_samples, post_master_sr, post_master_channels);
    let telemetry_short_term =
        short_term_lufs(&post_master_samples, post_master_sr, post_master_channels);

    tracing::info!(
        "Telemetry: lra={:.2} LU, momentary={:.2} LUFS, short_term={:.2} LUFS",
        telemetry_lra,
        telemetry_momentary,
        telemetry_short_term
    );

    // Certificate generation
    let cert = aether_bridge::generate_certificate(
        input_pcm_hash.clone(),
        &post_master_samples,
        aether_bridge::CertificateRequest {
            lra: telemetry_lra,
            persona: persona_config,
            dsp_config,
            proof_log,
            req: aether_req,
            system_version: env!("CARGO_PKG_VERSION"),
        },
    );
    let cert_json = serde_json::to_string(&cert).unwrap_or_default();
    let config_json = serde_json::to_string(&dsp_config).unwrap_or_default();

    // QR rendering moved to sign_and_render (§7 απόφαση 3, F-137) —
    // the core no longer produces it, so assemble_blob gets None here.
    let pcm_blake3 = crate::dsp_pipeline_helpers::blake3_pcm(left_slice);

    // F-085 wiring (offline): οι μετρήσεις γίνονται ΕΔΩ, στα
    // post-master slices που η συνάρτηση ΗΔΗ κρατάει — όχι στο
    // pre_analysis, που περιγράφει την ΕΙΣΟΔΟ (dsp_pipeline.rs:662/1036
    // το χτίζουν από trunk_metrics ΠΡΙΝ τον render).
    // Υπάρχουσες συναρτήσεις, σωστό σήμα.
    let stereo_correlation_measured =
        sp314_dsp::analysis::stereo::stereo_correlation(left_slice, right_slice);
    let dynamic_range_measured = {
        let n = left_slice.len().min(right_slice.len());
        let mono: Vec<f32> = (0..n)
            .map(|i| (left_slice[i] + right_slice[i]) * 0.5)
            .collect();
        let mut d = sp314_dsp::analysis::dynamics::StreamingDynamicsAnalyzer::new(sample_rate);
        d.feed_chunk(&mono);
        d.finish().dyn_range_db
    };
    // F-085 wiring (φασματικά + width), ίδιο σήμα, ίδιο σημείο.
    // ΠΡΟΣΟΧΗ: οι batch spectral_* θέλουν ΟΛΟΚΛΗΡΟ το σήμα στη μνήμη —
    // εδώ επιτρέπεται, ο offline caller ΗΔΗ κρατάει τα slices. Στο
    // streaming μονοπάτι χρησιμοποιείται ο StreamingSpectralAnalyzer,
    // που είναι bit-exact ισοδύναμος (assert_eq! στα streaming_tests).
    let (spectral_centroid_measured, spectral_flatness_measured) = {
        let n = left_slice.len().min(right_slice.len());
        let mono: Vec<f32> = (0..n)
            .map(|i| (left_slice[i] + right_slice[i]) * 0.5)
            .collect();
        (
            sp314_dsp::analysis::spectral::spectral_centroid_hz(&mono, sample_rate),
            sp314_dsp::analysis::spectral::spectral_flatness(&mono),
        )
    };
    // ΠΑΡΑΓΩΓΟ του correlation (stereo.rs:32), όχι νέα ανάγνωση.
    let stereo_width_measured = 1.0 - stereo_correlation_measured.abs();
    // F-085: γεγονότα clipping στα ΙΔΙΑ post-master slices.
    let clips_measured =
        sp314_dsp::analysis::clipping::count_clip_events(left_slice, right_slice);

    assemble_blob(
        blob_id,
        lufs,
        true_peak,
        fingerprints,
        spatial_metadata,
        persona_config,
        file_path,
        input_hash_hex,
        sample_rate,
        2,
        elapsed_ms,
        seed,
        preset_id,
        input_pcm_hash,
        processing_timeline,
        n_total,
        pcm_blake3,
        None,
        cert_json,
        config_json,
        telemetry_lra,
        Some(telemetry_short_term),
        Some(telemetry_momentary),
        crate::signal_health::DeadAirSummary::default(),
        None, // ACX check never runs on the Music path
        Vec::new(), // ούτε ανάλυση πατώματος — κενό, όχι absent
        folddown_gain_db,
        stereo_rms_measured,
        stereo_correlation_measured,
        dynamic_range_measured,
        stereo_width_measured,
        spectral_centroid_measured,
        spectral_flatness_measured,
        clips_measured,
    )
}

/// Precomputed certificate hashes from the
/// Episode streaming render. Both are produced
/// incrementally during episode_render (never
/// holding the full buffer in RAM):
///   - pcm_blake3:    left channel, f32 LE
///   - output_sha256: interleaved L+R, f32 BE
pub struct StreamingCertData {
    pub pcm_blake3: String,
    pub output_sha256: String,
    pub dead_air: crate::signal_health::DeadAirSummary,
    /// ACX delivery check from the trunk pass — Some only when the preset's
    /// DeliverySpec carries a noise-floor limit. None = not measured.
    pub acx: Option<sp314_dsp::analysis::acx_check::AcxCheckReport>,
    /// Τι αποφάσισε ή τι μέτρησε η μηχανή. Χτίζεται στον ΚΑΛΟΥΝΤΑ, γιατί εκεί
    /// ζει η γνώση του προορισμού (όριο, edge) — ο κόμβος πιστοποίησης
    /// συναρμολογεί, δεν αποφασίζει. Κενό όταν δεν υπήρξε ανάλυση.
    pub corrections: Vec<lineos_types::certificate::CorrectionRecord>,
}

/// Certificate node for the Episode streaming
/// path. Identical output to run() for the same
/// audio, but takes precomputed hashes instead of
/// left/right slices — the podcast pipeline never
/// holds the full buffer in RAM.
///
/// Differs from run() only in the top half (hash
/// source + telemetry); the StoredBlob assembly is
/// the shared assemble_blob() helper.
///
/// TODO(wave-2): short_term / momentary stay 0.0
///   for wave 2; noise_floor_db + apple_podcasts
///   pending.
#[allow(clippy::too_many_arguments)]
pub fn run_streaming(
    blob_id: &str,
    lufs: f32,
    lra: f32,
    true_peak: f32,
    fingerprints: &StemFingerprints,
    spatial_metadata: &sp314_dsp::stft::two_pass::RenderMetadata,
    proof_log: &integration::proof_log::ProofLog,
    persona_config: &aether::personas::config::PersonaConfig,
    aether_req: &aether_bridge::AetherRequest,
    dsp_config: &integration::config::DspConfig,
    file_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
    input_hash_hex: &str,
    sample_rate: u32,
    elapsed_ms: u64,
    seed: u64,
    preset_id: &str,
    input_pcm_hash: String,
    n_total: usize,
    processing_timeline: Vec<StageRecord>,
    cert_data: StreamingCertData,
    folddown_gain_db: Option<f32>,
    // F-085: μετρήσεις ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ από το πέρασμα που διαβάζει το
    // μασταραρισμένο σήμα — το ίδιο που δίνει output_lufs/output_lra/
    // true_peak_dbtp. ΟΧΙ από το trunk pass: εκείνο μετράει το raw_tap
    // ΠΡΙΝ τον render και περιγράφει την ΕΙΣΟΔΟ (executor.rs:282 vs :355).
    output_stereo_correlation: f32,
    output_dynamic_range_db: f32,
    output_rms_db: f32,
    output_stereo_width: f32,
    output_spectral_centroid: f32,
    output_spectral_flatness: f32,
    output_clips_detected: u32,
) -> Result<CertificateOutput, String> {
    // Episode: no array telemetry pass.
    // momentary / short-term stay None.
    let telemetry_lra = lra;

    // Certificate from the precomputed streaming
    // SHA-256 (identical to the batch certificate
    // for the same audio).
    let cert = aether_bridge::generate_certificate_from_hash(
        input_pcm_hash.clone(),
        cert_data.output_sha256,
        aether_bridge::CertificateRequest {
            lra: telemetry_lra,
            persona: persona_config,
            dsp_config,
            proof_log,
            req: aether_req,
            system_version: env!("CARGO_PKG_VERSION"),
        },
    );
    let cert_json = serde_json::to_string(&cert).unwrap_or_default();
    let config_json = serde_json::to_string(&dsp_config).unwrap_or_default();

    // QR rendering moved to sign_and_render (§7 απόφαση 3, F-137) —
    // the core no longer produces it, so assemble_blob gets None here.

    assemble_blob(
        blob_id,
        lufs,
        true_peak,
        fingerprints,
        spatial_metadata,
        persona_config,
        file_path,
        input_hash_hex,
        sample_rate,
        2,
        elapsed_ms,
        seed,
        preset_id,
        input_pcm_hash,
        processing_timeline,
        n_total,
        cert_data.pcm_blake3,
        None,
        cert_json,
        config_json,
        telemetry_lra,
        None,
        None,
        cert_data.dead_air,
        cert_data.acx,
        cert_data.corrections,
        folddown_gain_db,
        // F-070 ΚΛΕΙΝΕΙ ΚΑΙ ΓΙΑ ΤΟ STREAMING (F-085 wiring, 25/08):
        // το RMS του παραδοτέου μετριέται πλέον στο ίδιο πέρασμα με
        // LUFS/LRA/true-peak. Η lufs+3.0 προσέγγιση δεν χρησιμοποιείται
        // πια εδώ.
        Some(output_rms_db),
        output_stereo_correlation,
        output_dynamic_range_db,
        output_stereo_width,
        output_spectral_centroid,
        output_spectral_flatness,
        output_clips_detected,
    )
}

/// Assemble the  generate the PDF,
/// and return the CertificateOutput. Shared by
/// run() (Music, hashes computed from arrays)
/// and run_streaming() (Episode, hashes computed
/// incrementally during render). One source of
/// truth for the forensic record structure — the
/// two paths differ only in HOW they obtain the
/// hashes and telemetry, never in the blob shape.
#[allow(clippy::too_many_arguments)]
fn assemble_blob(
    blob_id: &str,
    lufs: f32,
    true_peak: f32,
    fingerprints: &StemFingerprints,
    spatial_metadata: &sp314_dsp::stft::two_pass::RenderMetadata,
    persona_config: &aether::personas::config::PersonaConfig,
    file_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
    input_hash_hex: &str,
    sample_rate: u32,
    channels: u16,
    elapsed_ms: u64,
    seed: u64,
    preset_id: &str,
    input_pcm_hash: String,
    processing_timeline: Vec<StageRecord>,
    n_total: usize,
    pcm_blake3: String,
    qr_base64: Option<String>,
    cert_json: String,
    config_json: String,
    telemetry_lra: f32,
    telemetry_short_term: Option<f32>,
    telemetry_momentary: Option<f32>,
    dead_air: crate::signal_health::DeadAirSummary,
    acx: Option<sp314_dsp::analysis::acx_check::AcxCheckReport>,
    corrections: Vec<lineos_types::certificate::CorrectionRecord>,
    folddown_gain_db: Option<f32>,
    stereo_rms_measured: Option<f32>,
    // F-085: μετρήσεις ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ (post-master), όχι της εισόδου.
    stereo_correlation_measured: f32,
    dynamic_range_measured: f32,
    stereo_width_measured: f32,
    spectral_centroid_measured: f32,
    spectral_flatness_measured: f32,
    clips_measured: u32,
) -> Result<CertificateOutput, String> {
    // Signing moved to sign_and_render (§7 απόφαση 3, F-137) — the
    // core assembles the unsigned declaration as a value, the caller
    // signs it. Extract early to avoid borrow-after-move when
    // dead_air is consumed below.
    // ΟΡΙΟ ΜΕΤΟΝΟΜΑΣΙΑΣ [F-097]: το πεδίο πηγή μετονομάστηκε, το πεδίο
    // προορισμού (StoredLoudness) ΟΧΙ — είναι σχήμα υπογεγραμμένου cert.
    let quietest_active = dead_air.quietest_active_window_dbfs;

    let blob_v2 = lineos_types::certificate::StoredBlobV2 {
        core: lineos_types::certificate::StoredBlobCore {
            id: blob_id.to_string(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: Utc::now().to_rfc3339(),
            input_path_hash: input_hash_hex.to_string(),
            input_pcm_sha256: Some(input_pcm_hash),
            seed,
            pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
            preset_id: preset_id.to_string(),
            schema_version: 2,
            pcm_blake3: Some(pcm_blake3),
            cert_signature: None, // sign_and_render fills this in
            audio_path: file_path.clone(),
            sample_rate,
            channels,
            num_frames: n_total,
        },
        variant: lineos_types::certificate::BlobVariant::Certified {
            stem_fingerprints: Some(fingerprints.clone()),
            qr_base64,
            processing_timeline,
            dead_air,
            aether_cert: Some(cert_json),
            aether_persona: Some(persona_config.id.clone()),
            aether_config: Some(config_json),
            corrections,
            loudness: lineos_types::certificate::StoredLoudness {
                integrated_lufs: lufs,
                short_term_lufs: telemetry_short_term,
                momentary_lufs: telemetry_momentary,
                true_peak_dbtp: true_peak,
                lra: telemetry_lra,
                noise_floor_dbfs: quietest_active,
                k_weighted: true,
                ebu_r128_target_lufs: -23.0,
                ebu_r128_compliant: lufs <= -23.0 && true_peak <= -1.0,
                spotify_compliant: platform_ok(lufs, -14.0, true_peak),
                youtube_compliant: platform_ok(lufs, -14.0, true_peak),
                apple_music_compliant: platform_ok(lufs, -16.0, true_peak),
                apple_podcasts_compliant: platform_ok(lufs, -16.0, true_peak),
                broadcast_compliant: platform_ok(lufs, -23.0, true_peak),
                tidal_compliant: platform_ok(lufs, -14.0, true_peak),
                too_quiet_for_mobile: lufs < MIN_MOBILE_PLAYBACK_LUFS,
                input_delivery_peak_db: acx.map(|a| a.sample_peak_db),
                input_delivery_rms_db: acx.map(|a| a.rms_db),
                input_delivery_noise_floor_db: acx.and_then(|a| a.noise_floor_db),
                input_delivery_quietest_window_start_frame: acx
                    .and_then(|a| a.quietest_window_start_frame),
                // ⚠ ΤΙ ΑΚΡΙΒΩΣ ΚΑΛΥΠΤΕΙ: ΤΡΙΑ από τα οκτώ κριτήρια του
                // οίκου — rms · peak · noise floor. ΟΧΙ spacing, ΟΧΙ
                // μορφή (sample rate/κανάλια/bitrate).
                // ⚠ ΚΑΙ ΤΙ ΚΡΙΝΕΙ: ΤΗΝ ΕΙΣΟΔΟ, πριν από κάθε render και
                // encode — ΟΧΙ το παραδοτέο. Άλλο ερώτημα: «μπορεί αυτή
                // η ηχογράφηση να γίνει ACX;», όχι «είναι το αρχείο που
                // παραδίδω συμμορφούμενο;». Η δεύτερη απάντηση είναι το
                // DeliveryVerdict::compose (conformance/blob_paths.rs).
                // ΤΟ ΟΝΟΜΑ ΤΟΥ ΠΕΔΙΟΥ ΔΕΝ ΑΛΛΑΖΕΙ ΕΔΩ: ταξιδεύει στο
                // ΥΠΟΓΕΓΡΑΜΜΕΝΟ cert — αλλαγή ονόματος = αλλαγή σχήματος,
                // ξεχωριστή απόφαση.
                input_acx_compliant: acx.map(|a| a.levels_within_limits()),
                // output_delivery_* δεν μετριέται εδώ — μόνο στο export path
                // (run_deliver_core), μετά το certificate. §5.6 Δ2.
                output_delivery_peak_db: None,
                output_delivery_rms_db: None,
                output_delivery_noise_floor_db: None,
                output_delivery_quietest_window_start_frame: None,
                delivery_checks: None,
                delivery_profile: None,
            },
            quality: lineos_types::certificate::StoredQuality {
                // ── ΜΕΤΡΗΜΕΝΑ ΣΤΟ ΠΑΡΑΔΟΤΕΟ (F-085 wiring, 2026-08-25) ──
                // Και τα τρία προέρχονται από το ΜΑΣΤΕΡΑΡΙΣΜΕΝΟ σήμα:
                // offline από τα left_slice/right_slice, streaming από
                // το wav_to_raw_measured πέρασμα. ΟΧΙ από το trunk pass —
                // εκείνο μετράει το raw_tap ΠΡΙΝ τον render
                // (executor.rs:282 vs :355) και περιγράφει την ΕΙΣΟΔΟ.
                stereo_correlation: stereo_correlation_measured,
                dynamic_range_db: dynamic_range_measured,
                // ΜΕΤΡΗΜΕΝΟ RMS του παραδοτέου· fallback lufs+3.0 ΜΟΝΟ
                // όπου δεν μετρήθηκε (F-070).
                rms_db: stereo_rms_measured.unwrap_or(lufs + 3.0),

                // ΜΕΤΡΗΜΕΝΑ στο master buffer (F-085 wiring, 25/08).
                // stereo_width = 1 − |stereo_correlation|. ΠΑΡΑΓΩΓΟ,
                // όχι ανεξάρτητη μέτρηση. Κατά §7.5 (παράγωγο δίπλα
                // στην πηγή του), υποψήφιο για αφαίρεση σε επόμενη
                // έκδοση σχήματος — σήμερα γίνεται τίμιο, δεν
                // αφαιρείται (παγωμένο σχήμα + 5 προβολές UI).
                stereo_width: stereo_width_measured,
                spectral_centroid: spectral_centroid_measured,
                spectral_flatness: spectral_flatness_measured,

                // ΜΕΤΡΗΜΕΝΟ: γεγονότα clipping του ΠΑΡΑΔΟΤΕΟΥ.
                // ΟΡΙΣΜΟΣ (sp314_dsp::analysis::clipping): ≥3 ΔΙΑΔΟΧΙΚΑ
                // δείγματα |x|≥0.999 στο ΙΔΙΟ κανάλι = ΕΝΑ γεγονός·
                // ριπή 847 δειγμάτων μετράει ΜΙΑ φορά· μεμονωμένο
                // δείγμα στο 1.0 ΔΕΝ είναι clip (κορυφή που ακουμπάει
                // το ταβάνι)· τα δύο κανάλια ΑΘΡΟΙΖΟΝΤΑΙ.
                // ⚠ ΜΕΤΡΑΕΙ ΤΗΝ ΕΞΟΔΟ — δικό μας clipping.
                // ΔΕΝ ΑΝΙΧΝΕΥΕΙ clipping ΤΗΣ ΕΙΣΟΔΟΥ: μετά από
                // gain/EQ/LTASS οι επίπεδες κορυφές μετακινούνται και
                // δεν κάθονται πια στο ταβάνι. Η ζημιά μένει, ο
                // ανιχνευτής δεν τη βλέπει. 0 εδώ ΔΕΝ σημαίνει «η
                // ηχογράφηση είναι καθαρή».
                // Input clipping = ΝΕΟ πεδίο στο input_delivery_*
                // μπλοκ, δικό του βήμα (πύλη εισόδου).
                clips_detected: clips_measured,

                // clip_free: ΔΕΝ είναι σταθερά — παράγωγο του
                // πραγματικού true_peak.
                clip_free: true_peak <= -1.0,
            },
            spatial: lineos_types::certificate::StoredSpatial {
                low: lineos_types::certificate::BandSpatial {
                    pan_mean: spatial_metadata.spatial[0].pan_mean,
                    pan_width: spatial_metadata.spatial[0].pan_width,
                },
                low_mid: lineos_types::certificate::BandSpatial {
                    pan_mean: spatial_metadata.spatial[1].pan_mean,
                    pan_width: spatial_metadata.spatial[1].pan_width,
                },
                mid: lineos_types::certificate::BandSpatial {
                    pan_mean: spatial_metadata.spatial[2].pan_mean,
                    pan_width: spatial_metadata.spatial[2].pan_width,
                },
                high_mid: lineos_types::certificate::BandSpatial {
                    pan_mean: spatial_metadata.spatial[3].pan_mean,
                    pan_width: spatial_metadata.spatial[3].pan_width,
                },
                high: lineos_types::certificate::BandSpatial {
                    pan_mean: spatial_metadata.spatial[4].pan_mean,
                    pan_width: spatial_metadata.spatial[4].pan_width,
                },
                folddown_gain_db,
            },
            provenance: lineos_types::certificate::StoredProvenance {
                engine_id: "E11".into(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                processing_time_ms: elapsed_ms,
                host_os: std::env::consts::OS.to_string(),
                created_by: "stillair-cockpit".into(),
                aether_enriched: true,
                aether_devices: vec![],
                target_triple: env!("TARGET_TRIPLE").to_string(),
                target_arch: env!("TARGET_ARCH").to_string(),
                target_os: env!("TARGET_OS").to_string(),
                target_env: env!("TARGET_ENV").to_string(),
                target_cpu: env!("TARGET_CPU").to_string(),
                rustc_version: env!("RUSTC_VERSION").to_string(),
                opt_level: env!("PROFILE_OPT_LEVEL").to_string(),
                codegen_units: env!("PROFILE_CODEGEN_UNITS").to_string(),
                // Η ίδια μηχανή παρήγαγε ΚΑΙ μετράει. Δηλώνεται, δεν κρύβεται.
                audio_origin: lineos_types::certificate::AudioOrigin::SelfProduced,
            },
        }
    };
    // PDF generation moved to sign_and_render (§7 απόφαση 3, F-137) —
    // the core returns the unsigned, unrendered declaration as a value.

    Ok(CertificateOutput { blob: blob_v2, file_path })
}

/// AES TD1008 §5: "it is recommended to keep the
/// Integrated Loudness of content above -20 LUFS" —
/// κάτω από αυτό, players με ανεπαρκές gain δεν το
/// παίζουν αρκετά δυνατά, ιδίως με περιβαλλοντικό
/// θόρυβο. Ιδιότητα των ΣΥΣΚΕΥΩΝ, όχι των πλατφορμών:
/// broadcast (-23) και Netflix (-27) είναι νόμιμα
/// κάτω από αυτό ΚΑΙ όντως πολύ ήσυχα για κινητό.
/// Επισήμανση, ΟΧΙ σφάλμα — wide dynamic range content
/// είναι σκόπιμα χαμηλό.
pub const MIN_MOBILE_PLAYBACK_LUFS: f32 = -20.0;

/// LUFS within 1 LU of target AND TP ≤ ceiling → platform compliant.
fn platform_ok(lufs: f32, target: f32, tp: f32) -> bool {
    lufs <= target + 1.0 && tp <= -1.0
}
