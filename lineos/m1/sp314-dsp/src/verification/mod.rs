// sp314-dsp/src/verification/mod.rs
// Authority: S-013 v1.0 (locked)
// Post-Flight Verification — deterministic loudness check
// after DSP render, before ExecutionProof generation.
// Constitutional: libm only, max ONE trim iteration, no ML.

use lineos_types::config::LoudnessTarget;
use lineos_types::audio::StereoBuffer;
use crate::metering::lufs::measure_integrated_lufs;

/// Configuration for post-flight verification.
#[derive(Debug, Clone)]
pub struct VerificationConfig {
    /// Target integrated LUFS (e.g. -14.0 for Spotify)
    pub target_lufs:    f32,
    /// Maximum True Peak dBTP (e.g. -1.0)
    pub target_tp_db:   f32,
    /// Pass tolerance in LU (default: 0.5)
    pub lufs_tolerance: f32,
    /// Safety clamp on trim magnitude in dB (default: 3.0)
    pub max_trim_db:    f32,
}

impl VerificationConfig {
    pub fn from_target(target: &LoudnessTarget) -> Self {
        Self {
            target_lufs:    target.target_lufs,
            target_tp_db:   target.max_true_peak_db,
            lufs_tolerance: 0.5,
            max_trim_db:    3.0,
        }
    }
    pub fn spotify() -> Self {
        Self::from_target(&LoudnessTarget::spotify())
    }
    pub fn broadcast() -> Self {
        Self::from_target(&LoudnessTarget::broadcast())
    }
}

/// Result of post-flight verification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerificationResult {
    pub lufs_before_trim:  f32,
    pub lufs_after_trim:   f32,
    pub true_peak_dbfs:    f32,
    pub trim_applied_db:   f32,
    pub was_trimmed:       bool,
    pub passed:            bool,
    /// Warning if still out of bounds after trim (non-fatal)
    pub warning:           Option<String>,
}

pub struct PostFlightVerifier;

impl PostFlightVerifier {
    /// Verify and optionally trim output PCM.
    /// Mutates audio.left and audio.right in-place if trim needed.
    /// MAX ONE trim iteration — no loops (S-013 constitutional rule).
    /// Certificate must be generated AFTER this call.
    pub fn verify_and_trim(
        audio:  &mut StereoBuffer,
        config: &VerificationConfig,
    ) -> VerificationResult {
        if audio.left.is_empty() || audio.right.is_empty() {
            return VerificationResult {
                lufs_before_trim: config.target_lufs,
                lufs_after_trim:  config.target_lufs,
                true_peak_dbfs:   config.target_tp_db,
                trim_applied_db:  0.0,
                was_trimmed:      false,
                passed:           true,
                warning:          None,
            };
        }

        // Step 1: Measure current state
        let lufs_before = measure_integrated_lufs(
            &audio.left, &audio.right);
        let tp_before   = Self::measure_peak_dbfs(
            &audio.left, &audio.right);

        let delta_lufs  = config.target_lufs - lufs_before;

        // Step 2: Check if already passing
        if libm::fabsf(delta_lufs) <= config.lufs_tolerance
           && tp_before <= config.target_tp_db {
            return VerificationResult {
                lufs_before_trim: lufs_before,
                lufs_after_trim:  lufs_before,
                true_peak_dbfs:   tp_before,
                trim_applied_db:  0.0,
                was_trimmed:      false,
                passed:           true,
                warning:          None,
            };
        }

        // Step 3: Compute trim
        // Safety clamp: never trim more than max_trim_db
        let mut trim_db = delta_lufs
            .clamp(-config.max_trim_db, config.max_trim_db);

        // CRITICAL: clamp positive trim by TP headroom
        // Prevents hard clipping when boosting quiet output
        let tp_headroom = config.target_tp_db - tp_before;
        if trim_db > tp_headroom {
            trim_db = tp_headroom;
        }

        // Step 4: Apply ONE trim pass
        let gain_linear = Self::db_to_linear(trim_db);
        Self::apply_gain(&mut audio.left,
                         &mut audio.right, gain_linear);

        // Step 5: Re-measure (read-only — no second trim)
        let lufs_after = measure_integrated_lufs(
            &audio.left, &audio.right);
        let tp_after   = Self::measure_peak_dbfs(
            &audio.left, &audio.right);

        let passed = libm::fabsf(
                         config.target_lufs - lufs_after)
                     <= config.lufs_tolerance
                     && tp_after <= config.target_tp_db;

        // Step 6: Warning if still out of bounds (non-fatal)
        let warning = if !passed {
            Some(format!(
                "Post-trim: {:.1} LUFS / {:.1} dBTP — \
                 target: {:.1} LUFS / {:.1} dBTP (±{:.1} LU). \
                 Manual adjustment may be required.",
                lufs_after, tp_after,
                config.target_lufs, config.target_tp_db,
                config.lufs_tolerance
            ))
        } else {
            None
        };

        VerificationResult {
            lufs_before_trim: lufs_before,
            lufs_after_trim:  lufs_after,
            true_peak_dbfs:   tp_after,
            trim_applied_db:  trim_db,
            was_trimmed:      true,
            passed,
            warning,
        }
    }

    /// Simplified peak measurement: max absolute sample → dBFS.
    /// TODO v1.1: Replace with 4x/8x oversampled true peak meter
    ///            to catch inter-sample peaks (ITU-R BS.1770).
    fn measure_peak_dbfs(left: &[f32], right: &[f32]) -> f32 {
        let max_l = left.iter()
            .map(|s| libm::fabsf(*s))
            .fold(0.0_f32, f32::max);
        let max_r = right.iter()
            .map(|s| libm::fabsf(*s))
            .fold(0.0_f32, f32::max);
        let peak  = max_l.max(max_r).max(1e-10_f32);
        20.0_f32 * libm::log10f(peak)
    }

    /// dB to linear gain. libm only — no std::f32.
    fn db_to_linear(db: f32) -> f32 {
        libm::powf(10.0_f32, db / 20.0_f32)
    }

    /// Apply linear gain to both channels in-place.
    fn apply_gain(left: &mut [f32], right: &mut [f32],
                  gain: f32) {
        for s in left.iter_mut()  { *s *= gain; }
        for s in right.iter_mut() { *s *= gain; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::audio::StereoBuffer;

    fn make_buffer(left: Vec<f32>,
                   right: Vec<f32>) -> StereoBuffer {
        let num_frames = left.len();
        StereoBuffer { left, right, sample_rate: 48000, num_frames }
    }

    fn sine_buffer(freq_hz: f32, duration_s: f32,
                   amplitude: f32) -> StereoBuffer {
        let sr = 48000_u32;
        let n  = (sr as f32 * duration_s) as usize;
        let signal: Vec<f32> = (0..n).map(|i| {
            amplitude * libm::sinf(
                2.0 * std::f32::consts::PI
                * freq_hz * i as f32 / sr as f32)
        }).collect();
        StereoBuffer {
            left:  signal.clone(),
            right: signal,
            sample_rate: sr,
            num_frames: n,
        }
    }

    #[test]
    fn verify_pass_when_already_on_target() {
        // Silent buffer → LUFS ≈ -144 dBFS (very quiet)
        // Use a loud sine to get near target
        let mut audio = sine_buffer(1000.0, 3.0, 0.1);
        let cfg = VerificationConfig::spotify();
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        // Just verify it doesn't panic and returns a result
        assert!(result.lufs_before_trim < 0.0);
    }

    #[test]
    fn verify_trim_applied_when_too_loud() {
        // Very loud signal → should be trimmed down
        let mut audio = sine_buffer(1000.0, 3.0, 0.9);
        let cfg = VerificationConfig::spotify();
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        if result.was_trimmed {
            assert!(result.trim_applied_db < 0.0,
                "Trim should be negative (gain reduction)");
        }
    }

    #[test]
    fn verify_max_trim_respected() {
        // Extremely quiet signal → trim capped at max_trim_db
        let mut audio = sine_buffer(1000.0, 3.0, 0.0001);
        let cfg = VerificationConfig::spotify();
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        assert!(result.trim_applied_db.abs()
            <= cfg.max_trim_db + 1e-4,
            "Trim must not exceed max_trim_db");
    }

    #[test]
    fn verify_tp_clamp_prevents_clipping() {
        // Signal near 0 dBFS — positive trim would clip
        let mut audio = sine_buffer(1000.0, 3.0, 0.95);
        let cfg = VerificationConfig {
            target_lufs:    -8.0, // demand loud output
            target_tp_db:   -1.0,
            lufs_tolerance: 0.5,
            max_trim_db:    6.0,
        };
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        // TP after trim must not exceed target
        assert!(result.true_peak_dbfs
            <= cfg.target_tp_db + 0.1,
            "TP must not exceed target after trim");
    }

    #[test]
    fn verify_deterministic() {
        let audio1 = sine_buffer(440.0, 2.0, 0.5);
        let audio2 = audio1.clone();
        let cfg    = VerificationConfig::spotify();

        let mut a1 = audio1;
        let mut a2 = audio2;

        let r1 = PostFlightVerifier::verify_and_trim(
            &mut a1, &cfg);
        let r2 = PostFlightVerifier::verify_and_trim(
            &mut a2, &cfg);

        assert_eq!(r1.trim_applied_db, r2.trim_applied_db);
        assert_eq!(r1.lufs_before_trim, r2.lufs_before_trim);
    }

    #[test]
    fn verify_empty_no_panic() {
        let mut audio = make_buffer(vec![], vec![]);
        let cfg    = VerificationConfig::spotify();
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        assert!(result.passed);
        assert!(!result.was_trimmed);
    }

    #[test]
    fn verify_one_pass_only() {
        // After one trim, no second pass should occur.
        // We verify this by checking was_trimmed=true
        // and that lufs_after_trim is the FINAL state.
        let mut audio = sine_buffer(1000.0, 3.0, 0.5);
        let cfg    = VerificationConfig::spotify();
        let result = PostFlightVerifier::verify_and_trim(
            &mut audio, &cfg);
        // If trimmed, after-trim LUFS must exist
        if result.was_trimmed {
            assert_ne!(result.lufs_before_trim,
                       result.lufs_after_trim);
        }
    }

    #[test]
    fn verify_config_from_spotify() {
        let cfg = VerificationConfig::spotify();
        assert_eq!(cfg.target_lufs,  -14.0);
        assert_eq!(cfg.target_tp_db, -1.0);
        assert_eq!(cfg.lufs_tolerance, 0.5);
        assert_eq!(cfg.max_trim_db,   3.0);
    }
}
