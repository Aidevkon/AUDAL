pub mod channel_assign;
pub mod five_dot_one;
pub mod mid_side;
pub mod renderer;
pub mod user_profile;

#[derive(Debug, Clone)]
pub struct SpatialPreAnalysis {
    pub mid_energy: f32,          // RMS of M channel
    pub side_energy: f32,         // RMS of S channel
    pub ms_ratio: f32,            // side/(mid+side) [0.0, 1.0]
    pub transient_direction: f32, // [-1.0=left, 0.0=center, 1.0=right]
    pub depth_score: f32,         // [0.0=front, 1.0=deep]
    pub sub_energy: f32,          // energy ratio below 80Hz
    pub presence_energy: f32,     // energy ratio 1-4kHz
    pub air_energy: f32,          // energy ratio above 8kHz
}

use crate::transforms::pca::pca_spatial;

/// Sample signal for PCA — every STRIDE samples.
/// Preserves statistical properties while reducing computation.
/// 1024 samples sufficient for covariance estimation.
const PCA_MAX_SAMPLES: usize = 1024;

fn sample_for_pca<'a>(signal: &'a [f32]) -> std::borrow::Cow<'a, [f32]> {
    if signal.len() <= PCA_MAX_SAMPLES {
        return std::borrow::Cow::Borrowed(signal);
    }
    let stride = signal.len() / PCA_MAX_SAMPLES;
    let sampled: Vec<f32> = signal
        .iter()
        .step_by(stride)
        .take(PCA_MAX_SAMPLES)
        .copied()
        .collect();
    std::borrow::Cow::Owned(sampled)
}

impl SpatialPreAnalysis {
    pub fn analyze(left: &[f32], right: &[f32], sample_rate: u32) -> Self {
        let n = left.len().min(right.len());
        if n == 0 {
            return Self {
                mid_energy: 0.0,
                side_energy: 0.0,
                ms_ratio: 0.0,
                transient_direction: 0.0,
                depth_score: 0.5,
                sub_energy: 0.0,
                presence_energy: 0.0,
                air_energy: 0.0,
            };
        }

        // ── Direct M/S energy measurement ────────────────────────────────────
        // mid  = (L+R)/2 — correlated content (voice, bass, center)
        // side = (L-R)/2 — uncorrelated content (width, ambience, stereo field)
        // This is mathematically exact — no eigendecomposition needed for RMS.
        let mut mid_sq = 0.0_f32;
        let mut side_sq = 0.0_f32;

        for i in 0..n {
            let m = (left[i] + right[i]) * 0.5_f32;
            let s = (left[i] - right[i]) * 0.5_f32;
            mid_sq += m * m;
            side_sq += s * s;
        }

        let mid_rms = libm::sqrtf(mid_sq / n as f32);
        let side_rms = libm::sqrtf(side_sq / n as f32);
        let total = (mid_rms + side_rms).max(1e-10_f32);

        // ms_ratio: 0.0 = fully mono, 1.0 = fully wide
        let ms_ratio = (side_rms / total).clamp(0.0_f32, 1.0_f32);

        // ── PCA for depth_score and transient_direction ───────────────────────
        // PCA eigendecomposition gives us:
        //   depth_score: how "deep" the stereo field is (correlated = front)
        //   transient_direction: L/R asymmetry of transients
        // These require the covariance matrix — PCA is the right tool here.
        let left_s = sample_for_pca(left);
        let right_s = sample_for_pca(right);
        let pca = pca_spatial(&left_s, &right_s);

        let depth_score = (1.0_f32 - pca.pc1_ratio).clamp(0.0_f32, 1.0_f32);
        let transient_direction = libm::sinf(pca.ms_angle_rad).clamp(-1.0_f32, 1.0_f32);

        // ── Sub energy (below 80 Hz) ──────────────────────────────────────────
        // IIR low-pass at 80 Hz — same as existing implementation.
        // alpha = exp(-2π·fc/fs) per LineOS Constitution §09.1 (libm only)
        let alpha_80 = libm::expf(-2.0_f32 * core::f32::consts::PI * 80.0_f32 / sample_rate as f32);

        let mut lp_80_l = 0.0_f32;
        let mut lp_80_r = 0.0_f32;
        let mut sub_sq = 0.0_f32;
        let mut total_sq = 0.0_f32;

        for i in 0..n {
            let l = left[i];
            let r = right[i];
            total_sq += l * l + r * r;
            lp_80_l = l * (1.0_f32 - alpha_80) + lp_80_l * alpha_80;
            lp_80_r = r * (1.0_f32 - alpha_80) + lp_80_r * alpha_80;
            sub_sq += lp_80_l * lp_80_l + lp_80_r * lp_80_r;
        }

        let total_energy = (total_sq / n as f32).max(1e-12_f32);
        let sub_energy = (sub_sq / n as f32) / total_energy;

        Self {
            mid_energy: mid_rms,
            side_energy: side_rms,
            ms_ratio,
            transient_direction,
            depth_score,
            sub_energy: sub_energy.clamp(0.0_f32, 1.0_f32),
            presence_energy: 0.0_f32, // future scope
            air_energy: 0.0_f32,      // future scope
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_signal_has_zero_side_energy() {
        // Mono: L == R → side = 0
        let signal = vec![0.5f32; 1024];
        let analysis = SpatialPreAnalysis::analyze(&signal, &signal, 48000);
        assert!(analysis.side_energy < 0.001);
        assert!(analysis.ms_ratio < 0.001);
    }

    #[test]
    fn wide_signal_has_high_ms_ratio() {
        // Wide: L = -R → pure side content
        let l: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.01).sin()).collect();
        let r: Vec<f32> = l.iter().map(|x| -x).collect();
        let analysis = SpatialPreAnalysis::analyze(&l, &r, 48000);
        assert!(
            analysis.ms_ratio > 0.8,
            "Expected high ms_ratio, got {}",
            analysis.ms_ratio
        );
    }

    #[test]
    fn sub_energy_detected_below_80hz() {
        // Pure 40Hz tone → high sub_energy
        let sr = 48000u32;
        let signal: Vec<f32> = (0..sr as usize)
            .map(|i| (2.0 * std::f32::consts::PI * 40.0 * i as f32 / sr as f32).sin())
            .collect();
        let analysis = SpatialPreAnalysis::analyze(&signal, &signal, sr);
        assert!(
            analysis.sub_energy > 0.5,
            "Expected high sub_energy, got {}",
            analysis.sub_energy
        );
    }

    #[test]
    fn analysis_deterministic() {
        let l: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).sin()).collect();
        let r: Vec<f32> = (0..512).map(|i| (i as f32 * 0.1).cos()).collect();
        let a1 = SpatialPreAnalysis::analyze(&l, &r, 48000);
        let a2 = SpatialPreAnalysis::analyze(&l, &r, 48000);
        assert_eq!(a1.ms_ratio, a2.ms_ratio);
        assert_eq!(a1.sub_energy, a2.sub_energy);
    }
}
