//! dsp_node — pre-analysis, autotune, AetherBridge, Corpus, DspAdapter.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P4
//!
//! `build_intent_and_config`/`IntentConfig` moved to conformance 21/09
//! (F-137) — the engine's own need. `run()` here (DspAdapter-dependent,
//! called only by run_dsp_internal) calls it via full path.

use crate::dsp::DspAdapter;
use lineos_types::StemFeatures;

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
    // ΙΔΙΟ fallback με το intent (γρ.52): το autotune
    // υπολογίζει pre-gain ΠΡΟΣ τον στόχο. Αν στοχεύει
    // αλλού από το intent, το gain staging ανεβάζει και
    // η LUFS correction κατεβάζει — δύο στάδια που
    // διαφωνούν μέσα στο ίδιο render, με το υλικό να
    // περνάει από saturation και limiter σε λάθος στάθμη.
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        target_lufs.unwrap_or(
            lineos_types::config::LoudnessTarget::from_preset(preset_id)
                .target_lufs,
        ),
    );
    let gain_linear = libm::powf(10.0_f32, autotune_result.pre_gain_db / 20.0_f32);
    // F-042 fixed: applied as drive staging inside
    // DspAdapter::master (see mod.rs).

    // Build intent + aether config via the
    // shared helper (also used by the Episode
    // streaming path). One source of truth.
    let cfg = conformance::dsp_node::build_intent_and_config(
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
    let user_model = match std::fs::read_to_string(&model_path)
        .ok()
        .and_then(|json| lineos_corpus::store::UserMarkovModel::from_json(&json).ok())
    {
        Some(m) if m.is_stale() => {
            eprintln!(
                "[corpus] model reset: schema {} < {} (F-044 stem-separation fix)",
                m.schema,
                lineos_corpus::store::CORPUS_SCHEMA
            );
            lineos_corpus::store::UserMarkovModel::new(proj_id)
        }
        Some(m) => m,
        None => lineos_corpus::store::UserMarkovModel::new(proj_id),
    };

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
