//! PCA-based Spatial Analysis
//! Authority: Orthogonal Transforms Spec v1.1 OT-P5
//! INV-OT-3: deterministic
//! INV-OT-4: does not modify input

/// Result of PCA on a stereo signal.
#[derive(Debug, Clone)]
pub struct PcaSpatialResult {
    pub pc1:            [f32; 2],  // dominant axis
    pub pc2:            [f32; 2],  // secondary axis
    pub pc1_ratio:      f32,       // [0.0, 1.0] energy in PC1
    pub correlation:    f32,       // [-1.0, 1.0] stereo correlation
    pub ms_angle_rad:   f32,       // rotation from fixed M/S
    pub cov_ll:         f32,
    pub cov_rr:         f32,
    pub cov_lr:         f32,
}

impl PcaSpatialResult {
    /// Is signal mono-like? (high correlation)
    pub fn is_correlated(&self) -> bool { self.correlation > 0.8 }
    /// Is signal wide? (low correlation)
    pub fn is_wide(&self) -> bool { self.correlation.abs() < 0.1 }
    /// Energy split: how much in dominant axis
    pub fn dominance(&self) -> f32 { self.pc1_ratio }
}

/// Compute PCA on stereo signal.
/// Returns adaptive principal components for spatial analysis.
/// INV-OT-3: deterministic — same signal → same result always.
pub fn pca_spatial(left: &[f32], right: &[f32]) -> PcaSpatialResult {
    assert_eq!(left.len(), right.len());
    let n = left.len() as f32;
    if n < 2.0 {
        return PcaSpatialResult {
            pc1: [1.0, 0.0], pc2: [0.0, 1.0],
            pc1_ratio: 0.5, correlation: 0.0,
            ms_angle_rad: 0.0,
            cov_ll: 0.0, cov_rr: 0.0, cov_lr: 0.0,
        };
    }

    // Covariance matrix (2×2)
    let mean_l: f32 = left.iter().sum::<f32>()  / n;
    let mean_r: f32 = right.iter().sum::<f32>() / n;

    let cov_ll: f32 = left.iter()
        .map(|&x| (x - mean_l).powi(2)).sum::<f32>() / (n - 1.0);
    let cov_rr: f32 = right.iter()
        .map(|&x| (x - mean_r).powi(2)).sum::<f32>() / (n - 1.0);
    let cov_lr: f32 = left.iter().zip(right.iter())
        .map(|(&l, &r)| (l - mean_l) * (r - mean_r))
        .sum::<f32>() / (n - 1.0);

    // Analytical 2×2 eigendecomposition (deterministic)
    // Eigenvalues of [[a, b], [b, c]]:
    // λ = (a+c)/2 ± sqrt(((a-c)/2)² + b²)
    let a = cov_ll;
    let b = cov_lr;
    let c = cov_rr;

    let trace  = a + c;
    let disc   = libm::sqrtf(((a - c) / 2.0).powi(2) + b * b);
    let lambda1 = trace / 2.0 + disc; // larger eigenvalue
    let lambda2 = trace / 2.0 - disc; // smaller eigenvalue

    // Eigenvectors
    let (pc1, pc2) = if disc > 1e-10 {
        let v1 = normalize([b, lambda1 - a]);
        let v2 = normalize([b, lambda2 - a]);
        (v1, v2)
    } else {
        ([1.0f32, 0.0], [0.0f32, 1.0]) // identity (already diagonal)
    };

    let total    = lambda1 + lambda2;
    let pc1_ratio = if total > 1e-10 { lambda1 / total } else { 0.5 };

    let denom     = libm::sqrtf(cov_ll * cov_rr);
    let correlation = if denom > 1e-10 { cov_lr / denom } else { 0.0 };
    let ms_angle_rad = libm::atan2f(pc1[1], pc1[0]);

    PcaSpatialResult {
        pc1, pc2, pc1_ratio,
        correlation: correlation.clamp(-1.0, 1.0),
        ms_angle_rad,
        cov_ll, cov_rr, cov_lr,
    }
}

fn normalize(v: [f32; 2]) -> [f32; 2] {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1]);
    if len > 1e-10 { [v[0] / len, v[1] / len] }
    else           { [1.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlated_stereo_has_high_correlation() {
        let n = 48000usize;
        let mono: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0
                      * i as f32 / 48000.0).sin())
            .collect();
        let left  = mono.clone();
        let right = mono.clone();
        let r = pca_spatial(&left, &right);
        assert!(r.correlation > 0.99,
            "Expected correlation > 0.99, got {}", r.correlation);
        assert!(r.pc1_ratio > 0.99,
            "Expected pc1_ratio > 0.99, got {}", r.pc1_ratio);
        assert!(r.is_correlated());
    }

    #[test]
    fn uncorrelated_stereo_has_low_pc1_ratio() {
        // Independent L and R → pc1_ratio ≈ 0.5
        let left:  Vec<f32> = (0..4096)
            .map(|i| libm::sinf(i as f32 * 0.1)).collect();
        let right: Vec<f32> = (0..4096)
            .map(|i| libm::cosf(i as f32 * 0.1)).collect();
        let r = pca_spatial(&left, &right);
        assert!(r.pc1_ratio < 0.7,
            "Expected pc1_ratio < 0.7 for orthogonal signals, got {}",
            r.pc1_ratio);
    }

    #[test]
    fn pca_deterministic() {
        // INV-OT-3: same input → same output
        let left:  Vec<f32> = (0..1024)
            .map(|i| libm::sinf(i as f32 * 0.05)).collect();
        let right: Vec<f32> = (0..1024)
            .map(|i| libm::cosf(i as f32 * 0.05)).collect();
        let r1 = pca_spatial(&left, &right);
        let r2 = pca_spatial(&left, &right);
        assert_eq!(r1.correlation,  r2.correlation);
        assert_eq!(r1.pc1_ratio,    r2.pc1_ratio);
        assert_eq!(r1.ms_angle_rad, r2.ms_angle_rad);
    }

    #[test]
    fn pca_does_not_modify_input() {
        // INV-OT-4
        let left:  Vec<f32> = vec![0.5, -0.3, 0.7, -0.1];
        let right: Vec<f32> = vec![0.4, -0.2, 0.6, -0.05];
        let left_orig  = left.clone();
        let right_orig = right.clone();
        let _ = pca_spatial(&left, &right);
        assert_eq!(left,  left_orig);
        assert_eq!(right, right_orig);
    }

    #[test]
    fn eigenvectors_are_orthogonal() {
        // PC1 · PC2 = 0
        let left:  Vec<f32> = (0..2048)
            .map(|i| libm::sinf(i as f32 * 0.1)).collect();
        let right: Vec<f32> = (0..2048)
            .map(|i| libm::sinf(i as f32 * 0.1 + 0.5)).collect();
        let r = pca_spatial(&left, &right);
        let dot = r.pc1[0] * r.pc2[0] + r.pc1[1] * r.pc2[1];
        assert!(dot.abs() < 1e-5,
            "PC1 and PC2 not orthogonal: dot={}", dot);
    }
}
