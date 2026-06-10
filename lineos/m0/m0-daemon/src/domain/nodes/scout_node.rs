//! scout_node — TwoPassEngine scout + Maestro AutoTuning.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P3

use crate::dsp::maestro::{AutoTuningController, RenderParams};
use lineos_corpus::store::UserMarkovModel;
use sp314_dsp::stft::two_pass::{ScoutResult, TwoPassEngine};

pub struct ScoutOutput {
    pub engine: TwoPassEngine,
    pub scout: ScoutResult,
    pub render_params: RenderParams,
    pub mono: Vec<f32>,
}

pub fn run(
    left: &[f32],
    right: &[f32],
    sample_rate: u32,
    project_id: &str,
    flavour_id: &str,
) -> Result<ScoutOutput, String> {
    // Build mono mix
    let mono: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    // True Scout Window — seek 30% to chorus
    let scout_window_len = (2.0 * sample_rate as f32) as usize;
    let scout_start = (mono.len() as f32 * 0.30) as usize;
    let scout_end = (scout_start + scout_window_len).min(mono.len());
    let scout_slice = &mono[scout_start..scout_end];

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(scout_slice, sample_rate);

    // Maestro — load user model, compute adaptive ducking_gain
    let model_path = format!("user_model_{}.json", project_id);
    let user_model = std::fs::read_to_string(&model_path)
        .ok()
        .and_then(|json| UserMarkovModel::from_json(&json).ok());

    let render_params =
        AutoTuningController::compute_render_params(&scout, user_model.as_ref(), flavour_id);

    tracing::info!(
        event = "m0d.maestro_params",
        ducking_gain = render_params.ducking_gain,
        bass_drums_distance = scout.stem_mfccs.bass_drums_distance(),
        "Maestro: adaptive ducking_gain computed"
    );

    Ok(ScoutOutput {
        engine,
        scout,
        render_params,
        mono,
    })
}
