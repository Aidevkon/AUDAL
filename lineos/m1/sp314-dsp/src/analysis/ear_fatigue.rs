//! EarFatigueModel — Temporary Threshold Shift simulation.
//! Authority: aether-black-spec-v1_0.md AB-P2
//! INV-AB-1: same input → same DspState delta. Always.

use lineos_types::pre_analysis::PreAnalysisData;
use xaak::repo::DspState;

/// Threshold above which ear fatigue is triggered.
/// Tracks louder than this require recovery compensation.
pub const FATIGUE_LUFS_THRESHOLD: f32 = -9.0;

/// Recovery window — first N ms of next track get soft treatment.
pub const RECOVERY_MS: u32 = 15_000;

/// Adjustment applied to next track when fatigue is detected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EarFatigueDelta {
    /// Multiply next track's ducking_depth by this factor
    pub ducking_multiplier: f32,
    /// Multiply next track's ms_width by this factor  
    pub width_multiplier:   f32,
    /// Recovery duration in ms
    pub recovery_ms:        u32,
    /// Was fatigue detected?
    pub fatigue_detected:   bool,
}

impl Default for EarFatigueDelta {
    fn default() -> Self {
        Self {
            ducking_multiplier: 1.0,
            width_multiplier:   1.0,
            recovery_ms:        0,
            fatigue_detected:   false,
        }
    }
}

pub struct EarFatigueModel {
    pub threshold_lufs: f32,
    pub recovery_ms:    u32,
}

impl Default for EarFatigueModel {
    fn default() -> Self {
        Self {
            threshold_lufs: FATIGUE_LUFS_THRESHOLD,
            recovery_ms:    RECOVERY_MS,
        }
    }
}

impl EarFatigueModel {
    pub fn new(threshold_lufs: f32, recovery_ms: u32) -> Self {
        Self { threshold_lufs, recovery_ms }
    }

    /// Compute fatigue delta from previous track analysis.
    /// Returns adjustment to apply to NEXT track's opening DspState.
    pub fn compute_delta(&self, prev_track: &PreAnalysisData) -> EarFatigueDelta {
        let is_loud = prev_track.integrated_lufs > self.threshold_lufs;
        let is_aggressive = prev_track.transient_density > 3.0;

        if !is_loud && !is_aggressive {
            return EarFatigueDelta::default();
        }

        // How far above threshold?
        let loudness_excess = if is_loud {
            (prev_track.integrated_lufs - self.threshold_lufs).max(0.0)
        } else { 0.0 };

        // Scale adjustments based on severity
        // Max excess ~9 LU (threshold -9, max around 0 LUFS)
        let severity = (loudness_excess / 9.0).clamp(0.0, 1.0);

        // More severe → softer treatment for next track
        // ducking_multiplier: 1.0 (no fatigue) → 0.5 (max fatigue)
        let ducking_multiplier = 1.0 - (severity * 0.5);

        // width_multiplier: 1.0 (no fatigue) → 0.7 (max fatigue)
        let width_multiplier = 1.0 - (severity * 0.3);

        EarFatigueDelta {
            ducking_multiplier,
            width_multiplier,
            recovery_ms:      self.recovery_ms,
            fatigue_detected: true,
        }
    }

    /// Apply fatigue delta to a base DspState.
    pub fn apply_delta(&self, base: &DspState, delta: &EarFatigueDelta) -> DspState {
        DspState {
            ducking_depth:  (base.ducking_depth * delta.ducking_multiplier)
                                .clamp(0.3, 1.0),
            ms_width:       (base.ms_width * delta.width_multiplier)
                                .clamp(0.5, 2.0),
            sidechain_hold: base.sidechain_hold,
            lfe_gain:       base.lfe_gain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_analysis(lufs: f32, td: f32) -> PreAnalysisData {
        use lineos_types::pre_analysis::PreAnalysisData;
        PreAnalysisData {
            integrated_lufs:   lufs,
            transient_density: td,
            ..PreAnalysisData::silent()
        }
    }

    #[test]
    fn no_fatigue_below_threshold() {
        let model  = EarFatigueModel::default();
        let quiet  = make_analysis(-14.0, 1.0);
        let delta  = model.compute_delta(&quiet);
        assert!(!delta.fatigue_detected);
        assert!((delta.ducking_multiplier - 1.0).abs() < 0.001);
    }

    #[test]
    fn fatigue_detected_above_threshold() {
        let model      = EarFatigueModel::default();
        let aggressive = make_analysis(-7.0, 4.0);
        let delta      = model.compute_delta(&aggressive);
        assert!(delta.fatigue_detected);
        assert!(delta.ducking_multiplier < 1.0);
        assert!(delta.width_multiplier < 1.0);
    }

    #[test]
    fn apply_delta_clamps_to_valid_range() {
        let model = EarFatigueModel::default();
        let base  = DspState { ducking_depth: 0.4, ms_width: 0.6,
                               sidechain_hold: 3, lfe_gain: 0.0 };
        let delta = EarFatigueDelta {
            ducking_multiplier: 0.5,
            width_multiplier:   0.5,
            recovery_ms:        15000,
            fatigue_detected:   true,
        };
        let adjusted = model.apply_delta(&base, &delta);
        assert!(adjusted.ducking_depth >= 0.3);
        assert!(adjusted.ms_width >= 0.5);
    }

    #[test]
    fn severity_scales_proportionally() {
        let model     = EarFatigueModel::default();
        let moderate  = make_analysis(-6.0, 2.0);
        let extreme   = make_analysis(-1.0, 5.0);
        let d_mod     = model.compute_delta(&moderate);
        let d_ext     = model.compute_delta(&extreme);
        assert!(d_ext.ducking_multiplier < d_mod.ducking_multiplier,
            "Extreme fatigue should produce more reduction");
    }
}
