//! scout_node — TwoPassEngine scout + Maestro AutoTuning.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P3

use crate::dsp::maestro::{AutoTuningController, RenderParams};
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
    _project_id: &str,
    _flavour_id: &str,
    pre_analysis: &lineos_types::pre_analysis::PreAnalysisData,
    use_nmfd: bool,
) -> Result<ScoutOutput, String> {
    // Build mono mix
    let mono: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(left, right, sample_rate, None, None, use_nmfd);

    // Maestro — compute adaptive ducking_gain based on rhythm
    let render_params = AutoTuningController::compute_render_params(pre_analysis);

    tracing::info!(
        event = "m0d.maestro_params",
        ducking_gain = render_params.ducking_gain,
        release_ms = render_params.release_ms,
        bpm = pre_analysis.bpm,
        "Maestro: adaptive ducking_gain computed"
    );

    Ok(ScoutOutput {
        engine,
        scout,
        render_params,
        mono,
    })
}
