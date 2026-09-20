//! Maestro Auto-Tuning Controller
//! Authority: lineos/docs/maestro-controller-spec-v1_0.md
//!
//! Bridges AI Brain (PreAnalysisData + BPM) → DSP Muscle (ducking_gain).
//! Pure function — no side effects, no I/O.
//! INV-AB-1: same inputs → same output.
//! INV-MAESTRO-1: ducking_gain clamped to [0.3, 1.0]
//! INV-MAESTRO-2: pure function

use lineos_types::pre_analysis::PreAnalysisData;

/// Parameters computed by Maestro for Pass 2.
/// Replaces hardcoded COLLISION_DUCKING_GAIN.
#[derive(Debug, Clone, Copy)]
pub struct RenderParams {
    /// Adaptive ducking gain for bass when collision detected.
    /// Range: [0.3, 1.0]. Default: 0.707 (-3dB).
    // ducking_gain: volume target during sidechain (0.0=silent, 1.0=no duck)
    pub ducking_gain: f32,
    /// Release time for ducking envelope
    pub release_ms: f32,
}

impl Default for RenderParams {
    fn default() -> Self {
        Self {
            ducking_gain: 0.707,
            release_ms: 120.0,
        }
    }
}

pub struct AutoTuningController;

impl AutoTuningController {
    /// Compute render params from BPM and Transient Density.
    pub fn compute_render_params(pre_analysis: &PreAnalysisData) -> RenderParams {
        let bpm = pre_analysis.bpm;

        // ducking_gain semantics:
        //   0.3 = DEEP ducking (volume drops to 30% during sidechain) → pumping effect
        //   0.8 = SHALLOW ducking (volume stays at 80%) → transparent compression
        // Relationship is INVERSE to BPM:
        //   Low BPM  → deep ducking  (wide gaps → room for pump)
        //   High BPM → shallow ducking (dense beats → no time for release)

        let (ducking_gain, release_ms): (f32, f32) = if bpm == 0.0 {
            // Ambient / spoken word / no rhythm detected
            (0.8, 200.0)
        } else if bpm < 100.0 {
            // Trap / Dubstep / Slow Hip-Hop — deep pump
            let density = pre_analysis.transient_density;
            let gain = (0.3 + density * 0.15).clamp(0.3, 0.5);
            (gain, 250.0)
        } else if bpm <= 130.0 {
            // Standard Pop / Mid-tempo
            (0.55, 120.0)
        } else {
            // DnB / Techno / Fast EDM — shallow, fast recovery
            (0.75, 50.0)
        };

        // DSP safety clamps
        let ducking_gain = ducking_gain.clamp(0.0, 1.0);
        let release_ms = release_ms.clamp(10.0, 500.0);

        RenderParams {
            ducking_gain,
            release_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpm_0_gives_subtle_ducking() {
        let mut pre = PreAnalysisData::silent();
        pre.bpm = 0.0;
        let params = AutoTuningController::compute_render_params(&pre);
        assert!((params.ducking_gain - 0.8).abs() < 0.001);
        assert!((params.release_ms - 200.0).abs() < 0.001);
    }

    #[test]
    fn bpm_75_gives_deep_ducking() {
        let mut pre = PreAnalysisData::silent();
        pre.bpm = 75.0;
        pre.transient_density = 0.5; // => gain = 0.3 + 0.5*0.15 = 0.375
        let params = AutoTuningController::compute_render_params(&pre);
        assert!(params.ducking_gain >= 0.3 && params.ducking_gain <= 0.5);
        assert!((params.release_ms - 250.0).abs() < 0.001);
    }

    #[test]
    fn bpm_120_gives_standard_ducking() {
        let mut pre = PreAnalysisData::silent();
        pre.bpm = 120.0;
        let params = AutoTuningController::compute_render_params(&pre);
        assert!((params.ducking_gain - 0.55).abs() < 0.001);
        assert!((params.release_ms - 120.0).abs() < 0.001);
    }

    #[test]
    fn bpm_160_gives_shallow_ducking() {
        let mut pre = PreAnalysisData::silent();
        pre.bpm = 160.0;
        let params = AutoTuningController::compute_render_params(&pre);
        assert!((params.ducking_gain - 0.75).abs() < 0.001);
        assert!((params.release_ms - 50.0).abs() < 0.001);
    }

    #[test]
    fn render_params_clamped() {
        // We can't directly inject bad ducking_gain through compute_render_params easily,
        // but we can ensure compute_render_params outputs values within clamps.
        let pre = PreAnalysisData::silent();
        let params = AutoTuningController::compute_render_params(&pre);
        assert!(params.ducking_gain >= 0.0 && params.ducking_gain <= 1.0);
        assert!(params.release_ms >= 10.0 && params.release_ms <= 500.0);
    }
}
