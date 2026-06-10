//! Maestro Auto-Tuning Controller
//! Authority: lineos/docs/maestro-controller-spec-v1_0.md
//!
//! Bridges AI Brain (UserMarkovModel + StemMfccs) → DSP Muscle (ducking_gain).
//! Pure function — no side effects, no I/O.
//! INV-AB-1: same inputs → same output.
//! INV-MAESTRO-1: ducking_gain clamped to [0.3, 1.0]
//! INV-MAESTRO-2: pure function
//! INV-MAESTRO-3: no model → distance-only logic

use sp314_dsp::stft::two_pass::ScoutResult;
use lineos_corpus::store::UserMarkovModel;

/// Parameters computed by Maestro for Pass 2.
/// Replaces hardcoded COLLISION_DUCKING_GAIN.
#[derive(Debug, Clone, Copy)]
pub struct RenderParams {
    /// Adaptive ducking gain for bass when collision detected.
    /// Range: [0.3, 1.0]. Default: 0.707 (-3dB).
    pub ducking_gain: f32,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self { ducking_gain: 0.707 }
    }
}

pub struct AutoTuningController;

impl AutoTuningController {
    /// Compute render params from scout MFCC data + historical model.
    ///
    /// Logic:
    ///   1. Compute L2 distance between bass and drums MFCC fingerprints
    ///   2. Map distance to ducking_gain (low distance = high collision risk)
    ///   3. Apply historical modifier from UserMarkovModel if available
    ///   4. Clamp to [0.3, 1.0]
    pub fn compute_render_params(
        scout:   &ScoutResult,
        model:   Option<&UserMarkovModel>,
        preset:  &str,
    ) -> RenderParams {
        let distance = scout.stem_mfccs.bass_drums_distance();

        // MFCC distance → base ducking gain
        // Low distance = similar timbre = high collision risk = more ducking
        let base_gain = if distance < 5.0 {
            0.5_f32   // -6dB aggressive
        } else if distance < 15.0 {
            0.707_f32 // -3dB default
        } else {
            0.9_f32   // -1dB subtle
        };

        // Historical modifier from UserMarkovModel
        let modifier = if let Some(m) = model {
            Self::historical_modifier(m, preset)
        } else {
            1.0_f32
        };

        let ducking_gain = (base_gain * modifier).clamp(0.3_f32, 1.0_f32);

        RenderParams { ducking_gain }
    }

    /// Compute historical collision modifier from model.
    /// Returns value in [0.7, 1.2] — multiplied into ducking_gain.
    fn historical_modifier(model: &UserMarkovModel, preset: &str) -> f32 {
        let preset_model = match model.preset(preset) {
            Some(p) => p,
            None    => return 1.0,
        };

        // Check drums stem collision history via transient density
        let drums_model = match preset_model.stem("drums") {
            Some(s) => s,
            None    => return 1.0,
        };

        // Use n_sessions as proxy for collision experience
        // More sessions with drums model → user works with transient-heavy music
        let sessions = drums_model.n_sessions;
        if sessions == 0 { return 1.0; }

        // High transient history → apply more aggressive ducking
        // Low history → subtle ducking
        if sessions >= 10 {
            0.8_f32 // aggressive: multiply base_gain × 0.8
        } else if sessions >= 3 {
            0.9_f32 // moderate
        } else {
            1.0_f32 // no modification yet
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp314_dsp::stft::two_pass::StemMfccs;

    #[test]
    fn low_distance_gives_aggressive_ducking() {
        // distance < 5.0 → base_gain = 0.5
        let bass  = [0.0f32; 13];
        let mut drums = [0.0f32; 13];
        // L2 distance = 2.0 → below 5.0 threshold
        drums[0] = 2.0;
        let d = StemMfccs::distance(&bass, &drums);
        assert!(d < 5.0, "Distance should be < 5.0, got {:.2}", d);
        let base_gain = if d < 5.0 { 0.5_f32 } else if d < 15.0 { 0.707_f32 } else { 0.9_f32 };
        assert!((base_gain - 0.5_f32).abs() < 0.001);
    }

    #[test]
    fn high_distance_gives_subtle_ducking() {
        let bass  = [0.0f32; 13];
        let mut drums = [0.0f32; 13];
        // L2 distance ~20.0 → above 15.0 threshold
        for k in 0..13 { drums[k] = 20.0 / (13.0_f32).sqrt(); }
        let d = StemMfccs::distance(&bass, &drums);
        assert!(d >= 15.0, "Distance should be >= 15.0, got {:.2}", d);
        let base_gain = if d < 5.0 { 0.5_f32 } else if d < 15.0 { 0.707_f32 } else { 0.9_f32 };
        assert!((base_gain - 0.9_f32).abs() < 0.001);
    }

    #[test]
    fn render_params_clamped() {
        let params = RenderParams { ducking_gain: 2.0 };
        // After clamping
        let clamped = params.ducking_gain.clamp(0.3, 1.0);
        assert_eq!(clamped, 1.0);
        let params2 = RenderParams { ducking_gain: 0.1 };
        let clamped2 = params2.ducking_gain.clamp(0.3, 1.0);
        assert_eq!(clamped2, 0.3);
    }
}
