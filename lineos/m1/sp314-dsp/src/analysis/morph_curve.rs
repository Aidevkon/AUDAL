//! MorphCurve — smooth DspState interpolation between tracks.
//! Used for gapless album transitions via ArcSwap commits.
//! Authority: aether-black-spec-v1_0.md AB-P3
//! INV-AB-1: same t + same states → same result. Always.

use xaak::repo::DspState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurveType {
    Linear,
    EaseInOut,   // S-curve — most natural for ear
    Exponential, // fast start, slow end
}

pub struct MorphCurve {
    pub duration_ms: u32,
    pub curve: CurveType,
}

impl Default for MorphCurve {
    fn default() -> Self {
        Self {
            duration_ms: 3000, // 3 second transition
            curve: CurveType::EaseInOut,
        }
    }
}

impl MorphCurve {
    pub fn new(duration_ms: u32, curve: CurveType) -> Self {
        Self { duration_ms, curve }
    }

    /// Interpolate between two DspStates at position t ∈ [0.0, 1.0].
    /// t=0.0 → from state, t=1.0 → to state.
    /// INV-AB-1: deterministic — libm only, no randomness.
    pub fn interpolate(&self, from: &DspState, to: &DspState, t: f32) -> DspState {
        let t = t.clamp(0.0, 1.0);
        let shaped = self.apply_curve(t);

        DspState {
            ducking_depth: lerp(from.ducking_depth, to.ducking_depth, shaped),
            ms_width: lerp(from.ms_width, to.ms_width, shaped),
            lfe_gain: lerp(from.lfe_gain, to.lfe_gain, shaped),
            sidechain_hold: lerp_usize(from.sidechain_hold, to.sidechain_hold, shaped),
        }
    }

    /// Generate N evenly-spaced DspState snapshots for the transition.
    /// Used by AlbumConductor to pre-compute ArcSwap commit timeline.
    pub fn generate_frames(&self, from: &DspState, to: &DspState, n: usize) -> Vec<DspState> {
        if n == 0 {
            return vec![];
        }
        if n == 1 {
            return vec![*to];
        }
        (0..n)
            .map(|i| {
                let t = i as f32 / (n - 1) as f32;
                self.interpolate(from, to, t)
            })
            .collect()
    }

    fn apply_curve(&self, t: f32) -> f32 {
        match self.curve {
            CurveType::Linear => t,
            CurveType::EaseInOut => ease_in_out(t),
            CurveType::Exponential => exponential(t),
        }
    }
}

/// Linear interpolation for f32.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation for usize (rounded).
fn lerp_usize(a: usize, b: usize, t: f32) -> usize {
    let a = a as f32;
    let b = b as f32;
    (a + (b - a) * t).round() as usize
}

/// S-curve: smooth start and end — most natural for ear transitions.
/// f(t) = 3t² - 2t³
fn ease_in_out(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Exponential: fast start, slow end.
/// f(t) = 1 - e^(-5t) normalized to [0,1]
fn exponential(t: f32) -> f32 {
    (1.0 - libm::expf(-5.0 * t)) / (1.0 - libm::expf(-5.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> DspState {
        DspState {
            ducking_depth: 1.0,
            ms_width: 1.0,
            sidechain_hold: 3,
            lfe_gain: 0.0,
        }
    }

    fn club() -> DspState {
        DspState {
            ducking_depth: 1.5,
            ms_width: 1.0,
            sidechain_hold: 5,
            lfe_gain: 2.0,
        }
    }

    #[test]
    fn t0_returns_from_state() {
        let curve = MorphCurve::default();
        let result = curve.interpolate(&neutral(), &club(), 0.0);
        assert!((result.ducking_depth - 1.0).abs() < 0.001);
        assert!((result.lfe_gain - 0.0).abs() < 0.001);
    }

    #[test]
    fn t1_returns_to_state() {
        let curve = MorphCurve::default();
        let result = curve.interpolate(&neutral(), &club(), 1.0);
        assert!((result.ducking_depth - 1.5).abs() < 0.001);
        assert!((result.lfe_gain - 2.0).abs() < 0.001);
    }

    #[test]
    fn midpoint_is_between() {
        let curve = MorphCurve::new(3000, CurveType::Linear);
        let result = curve.interpolate(&neutral(), &club(), 0.5);
        assert!(result.ducking_depth > 1.0 && result.ducking_depth < 1.5);
        assert!(result.lfe_gain > 0.0 && result.lfe_gain < 2.0);
    }

    #[test]
    fn ease_in_out_is_symmetric() {
        let a = ease_in_out(0.25);
        let b = ease_in_out(0.75);
        assert!(
            (a - (1.0 - b)).abs() < 0.001,
            "EaseInOut should be symmetric: f(0.25)={:.3}, 1-f(0.75)={:.3}",
            a,
            1.0 - b
        );
    }

    #[test]
    fn generate_frames_count() {
        let curve = MorphCurve::default();
        let frames = curve.generate_frames(&neutral(), &club(), 10);
        assert_eq!(frames.len(), 10);
        assert!((frames[0].ducking_depth - 1.0).abs() < 0.001);
        assert!((frames[9].ducking_depth - 1.5).abs() < 0.001);
    }
}
