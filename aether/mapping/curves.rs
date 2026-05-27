// aether/mapping/curves.rs — Curve shape application
// libm only — no std::f32 methods.
// Authority: spec/locked/S-005_macro_micro_mapping.md v1.0

use crate::personas::config::CurveShape;

/// Apply curve shape to a normalised input x ∈ [0.0, 1.0].
/// Output ∈ [0.0, 1.0].
/// Linear: identity
/// Log:    emphasises low end (sqrt-like)
/// Exp:    emphasises high end (square-like)
pub fn apply_curve(x: f32, curve: &CurveShape) -> f32 {
    let x = x.clamp(0.0_f32, 1.0_f32);
    match curve {
        CurveShape::Linear => x,
        CurveShape::Log    => libm::sqrtf(x),
        CurveShape::Exp    => x * x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curve_linear_identity() {
        assert!((apply_curve(0.5, &CurveShape::Linear) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn curve_log_greater_than_linear() {
        // sqrt(0.25) = 0.5 > 0.25
        let log = apply_curve(0.25, &CurveShape::Log);
        assert!(log > 0.25);
    }

    #[test]
    fn curve_exp_less_than_linear() {
        // 0.5^2 = 0.25 < 0.5
        let exp = apply_curve(0.5, &CurveShape::Exp);
        assert!(exp < 0.5);
    }

    #[test]
    fn curve_boundaries() {
        for curve in [CurveShape::Linear, CurveShape::Log, CurveShape::Exp] {
            assert!((apply_curve(0.0, &curve)).abs() < 1e-6);
            assert!((apply_curve(1.0, &curve) - 1.0).abs() < 1e-6);
        }
    }
}
