pub mod all_pass;
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

/// StreamingSpatialAnalyzer — streaming counterpart to SpatialPreAnalysis.
///
/// ADAPTIVE MODE (total_frames = None) is deterministic but NOT bit-identical
/// to offline sample_for_pca. It uses a bounded in-place decimation strategy
/// that dynamically doubles the stride to keep the PCA sample buffer <= PCA_MAX_SAMPLES.
/// The final PCA buffer will contain between 512 and 1024 samples.
pub struct StreamingSpatialAnalyzer {
    mid_sq: f32,
    side_sq: f32,
    total_samples: usize,

    lp_80_l: f32,
    lp_80_r: f32,
    sub_sq: f32,
    total_sq: f32,
    alpha_80: f32,

    left_pca: Vec<f32>,
    right_pca: Vec<f32>,

    exact_total_frames: Option<usize>,
    stride: usize,
}

impl StreamingSpatialAnalyzer {
    /// total_frames: exact frame count from AudioSource::total_frames_hint().
    /// Some(n)  -> EXACT mode: bit-identical to offline analyze().
    /// None     -> ADAPTIVE mode: bounded in-place decimation fallback,
    ///             deterministic but NOT bit-identical to offline.
    pub fn new(sample_rate: u32, total_frames: Option<usize>) -> Self {
        let alpha_80 = libm::expf(-2.0_f32 * core::f32::consts::PI * 80.0_f32 / sample_rate as f32);

        let stride = if let Some(n) = total_frames {
            if n <= PCA_MAX_SAMPLES { 1 } else { n / PCA_MAX_SAMPLES }
        } else {
            1
        };

        Self {
            mid_sq: 0.0,
            side_sq: 0.0,
            total_samples: 0,
            lp_80_l: 0.0,
            lp_80_r: 0.0,
            sub_sq: 0.0,
            total_sq: 0.0,
            alpha_80,
            left_pca: Vec::with_capacity(PCA_MAX_SAMPLES),
            right_pca: Vec::with_capacity(PCA_MAX_SAMPLES),
            exact_total_frames: total_frames,
            stride,
        }
    }

    pub fn feed_chunk(&mut self, left: &[f32], right: &[f32]) {
        let n = left.len().min(right.len());

        for i in 0..n {
            let l = left[i];
            let r = right[i];

            // 1. M/S RMS Accumulation
            let m = (l + r) * 0.5_f32;
            let s = (l - r) * 0.5_f32;
            self.mid_sq += m * m;
            self.side_sq += s * s;

            // 2. Sub energy (IIR)
            self.total_sq += l * l + r * r;
            self.lp_80_l = l * (1.0_f32 - self.alpha_80) + self.lp_80_l * self.alpha_80;
            self.lp_80_r = r * (1.0_f32 - self.alpha_80) + self.lp_80_r * self.alpha_80;
            self.sub_sq += self.lp_80_l * self.lp_80_l + self.lp_80_r * self.lp_80_r;

            // 3. PCA Sample Collection
            let abs_idx = self.total_samples + i;

            if self.exact_total_frames.is_some() {
                if abs_idx % self.stride == 0 && self.left_pca.len() < PCA_MAX_SAMPLES {
                    self.left_pca.push(l);
                    self.right_pca.push(r);
                }
            } else {
                if abs_idx % self.stride == 0 {
                    if self.left_pca.len() == PCA_MAX_SAMPLES {
                        // In-place decimation: keep even indices, drop odd indices
                        let mut keep_idx = 0;
                        for j in (0..PCA_MAX_SAMPLES).step_by(2) {
                            self.left_pca[keep_idx] = self.left_pca[j];
                            self.right_pca[keep_idx] = self.right_pca[j];
                            keep_idx += 1;
                        }
                        self.left_pca.truncate(keep_idx);
                        self.right_pca.truncate(keep_idx);
                        self.stride *= 2;
                    }
                    self.left_pca.push(l);
                    self.right_pca.push(r);
                }
            }
        }
        self.total_samples += n;
    }

    pub fn finish(self) -> SpatialPreAnalysis {
        let n = self.total_samples;
        if n == 0 {
            return SpatialPreAnalysis {
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

        let mid_rms = libm::sqrtf(self.mid_sq / n as f32);
        let side_rms = libm::sqrtf(self.side_sq / n as f32);
        let total = (mid_rms + side_rms).max(1e-10_f32);

        let ms_ratio = (side_rms / total).clamp(0.0_f32, 1.0_f32);

        let pca = pca_spatial(&self.left_pca, &self.right_pca);
        let depth_score = (1.0_f32 - pca.pc1_ratio).clamp(0.0_f32, 1.0_f32);
        let transient_direction = libm::sinf(pca.ms_angle_rad).clamp(-1.0_f32, 1.0_f32);

        let total_energy = (self.total_sq / n as f32).max(1e-12_f32);
        let sub_energy = (self.sub_sq / n as f32) / total_energy;

        SpatialPreAnalysis {
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

    #[test]
    fn streaming_spatial_exact_matches_offline() {
        let sr = 48000u32;
        let n = 20000;
        let mut left = vec![0.0f32; n];
        let mut right = vec![0.0f32; n];
        for i in 0..n {
            let sub = (2.0 * std::f32::consts::PI * 40.0 * i as f32 / sr as f32).sin();
            let wide = (i as f32 * 0.01).sin();
            left[i] = sub + wide;
            right[i] = sub - wide;
        }

        let offline = SpatialPreAnalysis::analyze(&left, &right, sr);

        let mut stream_4096 = StreamingSpatialAnalyzer::new(sr, Some(n));
        for (lc, rc) in left.chunks(4096).zip(right.chunks(4096)) {
            stream_4096.feed_chunk(lc, rc);
        }
        let online_4096 = stream_4096.finish();

        let mut stream_1023 = StreamingSpatialAnalyzer::new(sr, Some(n));
        for (lc, rc) in left.chunks(1023).zip(right.chunks(1023)) {
            stream_1023.feed_chunk(lc, rc);
        }
        let online_1023 = stream_1023.finish();

        let mut stream_mixed = StreamingSpatialAnalyzer::new(sr, Some(n));
        let mut offset = 0;
        let chunk_sizes = [1, 1024, 5, 4096, 17];
        let mut idx = 0;
        while offset < n {
            let chunk_len = chunk_sizes[idx % chunk_sizes.len()].min(n - offset);
            stream_mixed.feed_chunk(&left[offset..offset + chunk_len], &right[offset..offset + chunk_len]);
            offset += chunk_len;
            idx += 1;
        }
        let online_mixed = stream_mixed.finish();

        for online in &[online_4096, online_1023, online_mixed] {
            assert_eq!(offline.mid_energy, online.mid_energy);
            assert_eq!(offline.side_energy, online.side_energy);
            assert_eq!(offline.ms_ratio, online.ms_ratio);
            assert_eq!(offline.transient_direction, online.transient_direction);
            assert_eq!(offline.depth_score, online.depth_score);
            assert_eq!(offline.sub_energy, online.sub_energy);
            assert_eq!(offline.presence_energy, online.presence_energy);
            assert_eq!(offline.air_energy, online.air_energy);
        }
    }

    #[test]
    fn streaming_spatial_exact_tiny_signal() {
        let sr = 48000;
        let n = 500;
        let left: Vec<f32> = (0..n).map(|i| (i as f32 * 0.1).sin()).collect();
        let right: Vec<f32> = (0..n).map(|i| (i as f32 * 0.1).cos()).collect();

        let offline = SpatialPreAnalysis::analyze(&left, &right, sr);
        let mut streaming = StreamingSpatialAnalyzer::new(sr, Some(n));
        streaming.feed_chunk(&left, &right);
        let online = streaming.finish();

        assert_eq!(offline.depth_score, online.depth_score);
        assert_eq!(offline.sub_energy, online.sub_energy);
    }

    #[test]
    fn streaming_spatial_adaptive_is_deterministic() {
        let sr = 48000;
        let n = 50000;
        let mut left = vec![0.0f32; n];
        let mut right = vec![0.0f32; n];
        for i in 0..n {
            let sub = (2.0 * std::f32::consts::PI * 40.0 * i as f32 / sr as f32).sin();
            let wide = (i as f32 * 0.01).sin();
            left[i] = sub + wide;
            right[i] = sub - wide;
        }

        let mut stream_1 = StreamingSpatialAnalyzer::new(sr, None);
        for (lc, rc) in left.chunks(2048).zip(right.chunks(2048)) {
            stream_1.feed_chunk(lc, rc);
        }
        let online_1 = stream_1.finish();

        let mut stream_2 = StreamingSpatialAnalyzer::new(sr, None);
        for (lc, rc) in left.chunks(333).zip(right.chunks(333)) {
            stream_2.feed_chunk(lc, rc);
        }
        let online_2 = stream_2.finish();

        assert_eq!(online_1.depth_score, online_2.depth_score);
        assert_eq!(online_1.sub_energy, online_2.sub_energy);

        let offline = SpatialPreAnalysis::analyze(&left, &right, sr);
        assert!((offline.depth_score - online_1.depth_score).abs() < 0.05, 
            "Adaptive depth_score deviated: offline={} online={}", offline.depth_score, online_1.depth_score);
    }

    #[test]
    fn streaming_spatial_adaptive_bounded() {
        let sr = 48000;
        let n = 20000;
        let left: Vec<f32> = (0..n).map(|i| (i as f32 * 0.1).sin()).collect();
        let right: Vec<f32> = (0..n).map(|i| (i as f32 * 0.1).cos()).collect();

        let mut streaming = StreamingSpatialAnalyzer::new(sr, None);
        let initial_capacity = streaming.left_pca.capacity();
        assert!(initial_capacity >= PCA_MAX_SAMPLES);

        for (lc, rc) in left.chunks(100).zip(right.chunks(100)) {
            streaming.feed_chunk(lc, rc);
            assert!(streaming.left_pca.len() <= PCA_MAX_SAMPLES);
            assert_eq!(streaming.left_pca.capacity(), initial_capacity, "Capacity grew!");
        }

        let final_len = streaming.left_pca.len();
        assert!(final_len >= 512 && final_len <= PCA_MAX_SAMPLES, "Final PCA buffer len {} out of bounds", final_len);
        let _online = streaming.finish();
    }
}
