//! dsp_node — pre-analysis, autotune, AetherBridge, Corpus, DspAdapter.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P4

use crate::dsp::DspAdapter;
use lineos_types::{LoudnessTarget, MasteringIntent, StemFeatures};
use sp314_dsp::analysis::PreAnalyzer;

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

#[allow(clippy::ptr_arg)]
pub fn run(
    chunk_left: &mut Vec<f32>,
    chunk_right: &mut Vec<f32>,
    left_slice: &mut [f32],
    right_slice: &mut [f32],
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
) -> Result<DspOutput, String> {
    // Pre-Analysis
    let mut pre_analysis = PreAnalyzer::run(chunk_left, chunk_right, sample_rate);

    // Rhythm Analysis
    let mono_samples: Vec<f32> = chunk_left
        .iter()
        .zip(chunk_right.iter())
        .map(|(l, r)| (*l + *r) * 0.5)
        .collect();
    let detector = crate::dsp::beat_detector::BeatDetector::new(sample_rate);
    let (bpm, beats_ms, downbeats_ms, transients_ms) = detector.analyze(&mono_samples);
    tracing::info!("Rhythm Analysis: BPM = {:.1}, {} transients, {} downbeats", bpm, transients_ms.len(), downbeats_ms.len());
    pre_analysis.bpm = bpm;
    pre_analysis.beats_ms = beats_ms;
    pre_analysis.downbeats_ms = downbeats_ms;
    pre_analysis.transients_ms = transients_ms;

    // Autotune
    let autotune_result = sp314_dsp::pipeline::autotune::autotune(
        pre_analysis.integrated_lufs,
        target_lufs.unwrap_or(-14.0),
    );
    let gain_linear = libm::powf(10.0_f32, autotune_result.pre_gain_db / 20.0_f32);
    for s in chunk_left.iter_mut() {
        *s *= gain_linear;
    }
    for s in chunk_right.iter_mut() {
        *s *= gain_linear;
    }

    // MasteringIntent
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
    };

    // AetherBridge
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
    };

    let (dsp_config, proof_log, persona_config) =
        aether_bridge::build_dsp_config(&aether_req, streaming_features, Some(&pre_analysis))
            .map_err(|e| format!("AetherBridge error: {}", e))?;

    // NODE 2: CORPUS (Must run BEFORE master mutates slices)
    // Load UserMarkovModel from state dir
    let proj_id = project_id.unwrap_or("default");
    let state_dir = format!(
        "{}/.creator_os/state",
        std::env::var("HOME").unwrap_or_else(|_| ".".to_string())
    );
    let model_path = format!("{}/user_model_{}.json", state_dir, proj_id);
    let user_model = std::fs::read_to_string(&model_path)
        .ok()
        .and_then(|json| lineos_corpus::store::UserMarkovModel::from_json(&json).ok())
        .unwrap_or_else(|| lineos_corpus::store::UserMarkovModel::new(proj_id));

    let corpus_out = crate::domain::nodes::corpus_node::run(
        streaming_features,
        left_slice,
        &pre_analysis,
        blob_id,
        sample_rate,
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
        Some(&dsp_config),
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
