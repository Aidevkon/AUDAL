//! corpus_node — pure corpus computation. Zero file I/O.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P2
//! INV-CB-1: never modifies DSP behavior — background learning only.
//! PURE FUNCTIONAL CORE: No file I/O. State is injected and returned.

use lineos_corpus::contract::CorpusEnvelope;
use lineos_corpus::store::UserMarkovModel;
use lineos_types::pre_analysis::PreAnalysisData;
use lineos_types::StemFeatures;

pub struct CorpusOutput {
    pub envelope: CorpusEnvelope,
    pub user_model: UserMarkovModel,
}

pub fn run(
    streaming_features: &StemFeatures,
    left_slice: &[f32],
    pre_analysis: &PreAnalysisData,
    blob_id: &str,
    sample_rate: u32,
    flavour_id: &str,
    mut user_model: UserMarkovModel,
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

    // Pure in-memory mutation — zero I/O
    user_model.update(flavour_id, &corpus_envelope);

    CorpusOutput {
        envelope: corpus_envelope,
        user_model,
    }
}
