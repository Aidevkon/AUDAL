//! dsp_node — pre-analysis, autotune, AetherBridge, Corpus, DspAdapter.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P4

use crate::dsp::DspAdapter;
use lineos_types::{LoudnessTarget, MasteringIntent, StemFeatures};

pub struct DspOutput {
    pub lufs: f32,
    pub true_peak: f32,
    pub pre_analysis: lineos_types::pre_analysis::PreAnalysisData,
    pub dsp_config: integration::config::DspConfig,
    pub proof_log: integration::proof_log::ProofLog,
    pub persona_config: aether::personas::config::PersonaConfig,
    pub aether_req: aether_bridge::AetherRequest,
    pub user_model: Option<lineos_corpus::store::UserMarkovModel>,
}

/// Configuration built from a master
/// request — intent, aether config, and
/// the certificate provenance objects.
/// Shared between the standard dsp_node
/// path and the Episode streaming path.
pub struct IntentConfig {
    pub intent: MasteringIntent,
    pub aether_req: aether_bridge::AetherRequest,
    pub dsp_config: integration::config::DspConfig,
    pub proof_log: integration::proof_log::ProofLog,
    pub persona_config: aether::personas::config::PersonaConfig,
}

/// Build the MasteringIntent and Aether DSP
/// config for a master request. Does NOT run
/// corpus or the DSP graph — only constructs
/// the configuration objects that both the
/// standard and Episode paths need before
/// rendering.
#[allow(clippy::too_many_arguments)]
pub fn build_intent_and_config(
    preset_id: &str,
    flavour_id: &str,
    req_tone: Option<f32>,
    req_dynamics: Option<f32>,
    chaos_seed: Option<u64>,
    project_id: Option<&str>,
    track_id: Option<&str>,
    target_lufs: Option<f32>,
    streaming_features: &StemFeatures,
    pre_analysis: &lineos_types::pre_analysis::PreAnalysisData,
) -> Result<IntentConfig, String> {
    let intent = MasteringIntent {
        target: LoudnessTarget {
            target_lufs: target_lufs.unwrap_or(-14.0),
            max_true_peak_db: -1.0,
            max_lra_lu: None,
            platform: "default".into(),
        },
        preset_name: preset_id.to_string(),
        stem_mode: false,
        target_makeup_db: 0.0,
        limiter_blend_release_ms: {
            // Control Plane: lerp here,
            // NOT in DSP engine.
            // 0.0 (Smooth) -> 200ms
            // 0.5 (default) -> 95ms
            // 1.0 (Punchy) -> 10ms
            let d = req_dynamics.unwrap_or(0.5).clamp(0.0, 1.0);
            200.0 - (d * 190.0)
        },
        max_limiter_gr_db: 6.0,
    };

    use crate::domain::content_type::{ContentType, ContentTypeExt};
    use crate::domain::dsp_pipeline::map_flavour_to_persona;
    let mapped_persona = map_flavour_to_persona(flavour_id);
    let aether_req = aether_bridge::AetherRequest {
        persona_id: Some(mapped_persona.to_string()),
        tone: req_tone,
        dynamics: req_dynamics,
        ambience: None,
        chaos_seed,
        project_id: project_id.map(|s| s.to_string()),
        track_id: track_id.map(|s| s.to_string()),
        preset_name: Some(preset_id.to_string()),
        content_type: ContentType::from_preset(preset_id),
    };
    let (dsp_config, proof_log, persona_config) =
        aether_bridge::build_dsp_config(&aether_req, streaming_features, Some(pre_analysis))
            .map_err(|e| format!("AetherBridge error: {}", e))?;

    Ok(IntentConfig {
        intent,
        aether_req,
        dsp_config,
        proof_log,
        persona_config,
    })
}

// F-044 tourniquet: proxy stems replace scout_left x5 on the
// Music batch path. Temporary bridge — this data still comes
// from the 30s scout proxy, not the full file. Retired when
// Cycle 5's Y-trunk measures stems on the complete raw signal
// (same lifecycle as F-042's pre_gain trade-off).
#[allow(clippy::ptr_arg)]
#[allow(clippy::too_many_arguments)]
pub fn run(
    left_slice: &mut [f32],
    right_slice: &mut [f32],
    proxy_voice: &[f32],
    proxy_drums: &[f32],
    proxy_bass: &[f32],
    proxy_harmonics: &[f32],
    proxy_ambience: &[f32],
    sample_rate: u32,
    preset_id: &str,
    target_lufs: Option<f32>,
    flavour_id: &str,
    streaming_features: &StemFeatures,
    req_tone: Option<f32>,
    req_dynamics: Option<f32>,
    chaos_seed: Option<u64>,
    project_id: Option<&str>,
    track_id: Option<&str>,
    blob_id: &str,
    pre_analysis: lineos_types::pre_analysis::PreAnalysisData,
    state_dir: &str,
) -> Result<DspOutput, String> {
    // Autotune
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        target_lufs.unwrap_or(-14.0),
    );
    let gain_linear = libm::powf(10.0_f32, autotune_result.pre_gain_db / 20.0_f32);
    // F-042 fixed: applied as drive staging inside
    // DspAdapter::master (see mod.rs).

    // Build intent + aether config via the
    // shared helper (also used by the Episode
    // streaming path). One source of truth.
    let cfg = build_intent_and_config(
        preset_id,
        flavour_id,
        req_tone,
        req_dynamics,
        chaos_seed,
        project_id,
        track_id,
        target_lufs,
        streaming_features,
        &pre_analysis,
    )?;
    let intent = cfg.intent;
    let aether_req = cfg.aether_req;
    let dsp_config = cfg.dsp_config;
    let proof_log = cfg.proof_log;
    let persona_config = cfg.persona_config;

    // NODE 2: CORPUS (Must run BEFORE master mutates slices)
    // Load UserMarkovModel from state dir
    let proj_id = project_id.unwrap_or("default");
    let model_path = format!("{}/user_model_{}.json", state_dir, proj_id);
    let user_model = std::fs::read_to_string(&model_path)
        .ok()
        .and_then(|json| lineos_corpus::store::UserMarkovModel::from_json(&json).ok())
        .unwrap_or_else(|| lineos_corpus::store::UserMarkovModel::new(proj_id));

    let corpus_out = crate::domain::nodes::corpus_node::run(
        streaming_features,
        proxy_voice,
        proxy_drums,
        proxy_bass,
        proxy_harmonics,
        proxy_ambience,
        &pre_analysis,
        blob_id,
        sample_rate / sp314_dsp::stft::two_pass::SCOUT_DOWNSAMPLE as u32,
        flavour_id,
        user_model,
    );

    // DspAdapter::master (Mutates left_slice / right_slice)
    let dsp_start = std::time::Instant::now();
    let result = DspAdapter::master(
        &intent,
        left_slice,
        right_slice,
        sample_rate,
        &pre_analysis,
        &streaming_features.mix.stem_energy_ratios,
        Some(&dsp_config),
        gain_linear,
    )
    .map_err(|e| format!("DSP pipeline error: {:?}", e))?;
    if dsp_start.elapsed().as_secs() > 300 {
        tracing::warn!("DSP took more than 300 seconds");
    }

    let lufs = result.lufs.integrated_lufs;
    let true_peak = result.lufs.true_peak_dbfs;

    Ok(DspOutput {
        lufs,
        true_peak,
        pre_analysis,
        dsp_config,
        proof_log,
        persona_config,
        aether_req,
        user_model: Some(corpus_out.user_model),
    })
}
