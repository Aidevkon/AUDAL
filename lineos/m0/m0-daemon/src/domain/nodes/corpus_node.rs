//! corpus_node — corpus generation + UserMarkovModel update.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P2
//! Extracted from dsp_pipeline.rs with zero behavior change.
//! INV-CB-1: never modifies DSP behavior — background learning only.

use lineos_corpus::contract::CorpusEnvelope;
use lineos_corpus::store::{aggregate_preset, UserMarkovModel};
use lineos_types::pre_analysis::PreAnalysisData;
use lineos_types::StemFeatures;

pub struct CorpusOutput {
    pub envelope: CorpusEnvelope,
    pub corpus_path: String,
}

pub fn run(
    streaming_features: &StemFeatures,
    left_slice: &[f32],
    pre_analysis: &PreAnalysisData,
    blob_id: &str,
    sample_rate: u32,
    flavour_id: &str,
    project_id: &str,
) -> CorpusOutput {
    use lineos_corpus::builder::build_timeline;

    let corpus_envelope = build_timeline(
        streaming_features,
        left_slice,
        left_slice,
        left_slice,
        left_slice,
        left_slice,
        pre_analysis,
        blob_id,
        sample_rate,
        flavour_id,
    );

    // Write corpus.json — silent failure
    let corpus_path = format!("session_{}.corpus.json", &blob_id[..blob_id.len().min(8)]);
    if let Ok(json) = serde_json::to_string_pretty(&corpus_envelope) {
        let _ = std::fs::write(&corpus_path, json);
    }

    // Update UserMarkovModel — incremental, silent
    let model_path = format!("user_model_{}.json", project_id);
    let mut user_model = std::fs::read_to_string(&model_path)
        .ok()
        .and_then(|json| UserMarkovModel::from_json(&json).ok())
        .unwrap_or_else(|| UserMarkovModel::new(project_id));

    user_model.update(flavour_id, &corpus_envelope);

    if let Ok(json) = user_model.to_json() {
        let _ = std::fs::write(&model_path, json);
    }

    // Every 10 sessions: recompute global preset snapshot
    if user_model.version % 10 == 0 {
        if let Some(global) = aggregate_preset(flavour_id, &[&user_model]) {
            let global_path = format!("global_{}_v{}.json", flavour_id, user_model.version / 10);
            if let Ok(json) = serde_json::to_string(&global) {
                let _ = std::fs::write(&global_path, json);
            }
        }
    }

    CorpusOutput {
        envelope: corpus_envelope,
        corpus_path,
    }
}
