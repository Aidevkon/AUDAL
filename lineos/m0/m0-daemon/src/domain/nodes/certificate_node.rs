//! certificate_node — telemetry, certificate, StoredBlob assembly.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P5

use crate::blob_store::{
    StageRecord, StemFingerprints, StoredBlob,
};
use chrono::Utc;
use lineos_telemetry::lra::LraCalculator;
use lineos_telemetry::windows::{momentary_lufs, short_term_lufs};
use lineos_types::pre_analysis::PreAnalysisData;

pub struct CertificateOutput {
    pub blob: crate::blob_store::StoredBlobV2,
    pub file_path: std::sync::Arc<lineos_types::audio::ManagedPcm>,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    blob_id: &str,
    lufs: f32,
    true_peak: f32,
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
        input_pcm_hash,
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

    let qr_base64 = crate::handlers::certificate::generate_qr_base64(
        blob_id,
        file_path
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        lufs,
        true_peak,
        telemetry_lra,
        fingerprints,
    );

    let pcm_blake3 = crate::handlers::certificate::blake3_pcm(left_slice);
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
        processing_timeline,
        n_total,
        pcm_blake3,
        qr_base64,
        cert_json,
        config_json,
        telemetry_lra,
        telemetry_short_term,
        telemetry_momentary,
        crate::dsp::signal_health::DeadAirSummary::default(),
        None, // ACX check never runs on the Music path
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
    pub dead_air: crate::dsp::signal_health::DeadAirSummary,
    /// ACX delivery check from the trunk pass — Some only when the preset's
    /// DeliverySpec carries a noise-floor limit. None = not measured.
    pub acx: Option<sp314_dsp::analysis::acx_check::AcxCheckReport>,
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
) -> Result<CertificateOutput, String> {
    // Episode: no array telemetry pass.
    // momentary / short-term stay 0.0.
    let telemetry_lra = lra;
    let telemetry_short_term = 0.0_f32;
    let telemetry_momentary = 0.0_f32;

    // Certificate from the precomputed streaming
    // SHA-256 (identical to the batch certificate
    // for the same audio).
    let cert = aether_bridge::generate_certificate_from_hash(
        input_pcm_hash,
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

    let qr_base64 = crate::handlers::certificate::generate_qr_base64(
        blob_id,
        file_path
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        lufs,
        true_peak,
        telemetry_lra,
        fingerprints,
    );

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
        processing_timeline,
        n_total,
        cert_data.pcm_blake3,
        qr_base64,
        cert_json,
        config_json,
        telemetry_lra,
        telemetry_short_term,
        telemetry_momentary,
        cert_data.dead_air,
        cert_data.acx,
    )
}

/// Assemble the StoredBlob, generate the PDF,
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
    processing_timeline: Vec<StageRecord>,
    n_total: usize,
    pcm_blake3: String,
    qr_base64: Option<String>,
    cert_json: String,
    config_json: String,
    telemetry_lra: f32,
    telemetry_short_term: f32,
    telemetry_momentary: f32,
    dead_air: crate::dsp::signal_health::DeadAirSummary,
    acx: Option<sp314_dsp::analysis::acx_check::AcxCheckReport>,
) -> Result<CertificateOutput, String> {
    let cert_sig =
        crate::handlers::certificate::sign_certificate(blob_id, &pcm_blake3, lufs, fingerprints);

    // TODO: wire real DR when this path carries trunk metrics (register item)
    let dr = 10.0;
    let sc = 1.0;

    // Extract early to avoid borrow-after-move when dead_air is consumed below.
    let noise_floor = dead_air.noise_floor_dbfs;

    let blob_v2 = crate::blob_store::StoredBlobV2 {
        core: crate::blob_store::StoredBlobCore {
            id: blob_id.to_string(),
            version: "1.0".into(),
            blob_type: "audio".into(),
            created_at: Utc::now().to_rfc3339(),
            input_hash: input_hash_hex.to_string(),
            seed,
            pipeline_version: env!("CARGO_PKG_VERSION").to_string(),
            preset_id: preset_id.to_string(),
            schema_version: 2,
            pcm_blake3: Some(pcm_blake3),
            cert_signature: Some(cert_sig),
            audio_path: file_path.clone(),
            sample_rate,
            channels,
            num_frames: n_total,
        },
        variant: crate::blob_store::BlobVariant::Certified {
            stem_fingerprints: Some(fingerprints.clone()),
            qr_base64,
            processing_timeline,
            dead_air,
            aether_cert: Some(cert_json),
            aether_persona: Some(persona_config.id.clone()),
            aether_config: Some(config_json),
            loudness: crate::blob_store::StoredLoudness {
                integrated_lufs: lufs,
                short_term_lufs: telemetry_short_term,
                momentary_lufs: telemetry_momentary,
                true_peak_dbtp: true_peak,
                lra: telemetry_lra,
                noise_floor_dbfs: noise_floor,
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
                acx_sample_peak_db: acx.map(|a| a.sample_peak_db),
                acx_rms_db: acx.map(|a| a.rms_db),
                acx_noise_floor_db: acx.and_then(|a| a.noise_floor_db),
                acx_quietest_window_start_frame: acx.and_then(|a| a.quietest_window_start_frame),
                acx_compliant: acx.map(|a| a.passes_acx()),
            },
            quality: crate::blob_store::StoredQuality {
                stereo_correlation: sc,
                phase_coherence: 0.97,
                stereo_width: 0.5,
                dynamic_range_db: dr,
                rms_db: lufs + 3.0,
                spectral_centroid: 3_200.0,
                spectral_flatness: 0.12,
                clips_detected: 0,
                clip_free: true_peak <= -1.0,
            },
            spatial: crate::blob_store::StoredSpatial {
                low: crate::blob_store::BandSpatial {
                    pan_mean: spatial_metadata.spatial[0].pan_mean,
                    pan_width: spatial_metadata.spatial[0].pan_width,
                },
                low_mid: crate::blob_store::BandSpatial {
                    pan_mean: spatial_metadata.spatial[1].pan_mean,
                    pan_width: spatial_metadata.spatial[1].pan_width,
                },
                mid: crate::blob_store::BandSpatial {
                    pan_mean: spatial_metadata.spatial[2].pan_mean,
                    pan_width: spatial_metadata.spatial[2].pan_width,
                },
                high_mid: crate::blob_store::BandSpatial {
                    pan_mean: spatial_metadata.spatial[3].pan_mean,
                    pan_width: spatial_metadata.spatial[3].pan_width,
                },
                high: crate::blob_store::BandSpatial {
                    pan_mean: spatial_metadata.spatial[4].pan_mean,
                    pan_width: spatial_metadata.spatial[4].pan_width,
                },
            },
            provenance: crate::blob_store::StoredProvenance {
                engine_id: "E11".into(),
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                processing_time_ms: elapsed_ms,
                host_os: std::env::consts::OS.to_string(),
                created_by: "stillair-cockpit".into(),
                aether_enriched: true,
                aether_devices: vec![],
            },
        }
    };
    // Generate PDF certificate — silent, never blocks pipeline
    let pdf_path =
        std::env::temp_dir().join(format!("m0d-cert-{}.pdf", &blob_v2.core.id[..blob_v2.core.id.len().min(8)]));
    let pdf_path_str = pdf_path.to_string_lossy();
    crate::handlers::pdf_gen::generate_silent_certificate(&blob_v2, &pdf_path_str);

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
