use crate::mfcc::N_MFCC;
extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, PartialEq, Eq)]
pub enum CentroidError {
    EmptyInput,
}

pub fn compute_global_stats(
    all_frames: &[[f32; N_MFCC]],
) -> Result<([f32; N_MFCC], [f32; N_MFCC]), CentroidError> {
    let n = all_frames.len() as f32;
    if n == 0.0 {
        return Err(CentroidError::EmptyInput);
    }

    let mut mean = [0.0; N_MFCC];
    for frame in all_frames {
        for i in 0..N_MFCC {
            mean[i] += frame[i];
        }
    }
    for i in 0..N_MFCC {
        mean[i] /= n;
    }

    let mut var = [0.0; N_MFCC];
    for frame in all_frames {
        for i in 0..N_MFCC {
            let diff = frame[i] - mean[i];
            var[i] += diff * diff;
        }
    }

    let mut std = [0.0; N_MFCC];
    for i in 0..N_MFCC {
        // IEEE 754 requires correctly-rounded sqrt, so libm::sqrtf is bit-identical to std
        std[i] = libm::sqrtf(var[i] / n);
    }

    Ok((mean, std))
}

pub fn compute_bucket_centroid(
    bucket_frames: &[[f32; N_MFCC]],
    global_mean: &[f32; N_MFCC],
    global_std: &[f32; N_MFCC],
) -> Result<([f32; N_MFCC], [f32; N_MFCC]), CentroidError> {
    let n = bucket_frames.len() as f32;
    if n == 0.0 {
        return Err(CentroidError::EmptyInput);
    }

    let mut z_frames = Vec::with_capacity(bucket_frames.len());
    for frame in bucket_frames {
        let mut z = [0.0; N_MFCC];
        for i in 0..N_MFCC {
            // Apply z = (x - global_mean) / (global_std + 1e-8) // Αποφυγή division by zero!
            z[i] = (frame[i] - global_mean[i]) / (global_std[i] + 1e-8);
        }
        z_frames.push(z);
    }

    let mut mean = [0.0; N_MFCC];
    for z in &z_frames {
        for i in 0..N_MFCC {
            mean[i] += z[i];
        }
    }
    for i in 0..N_MFCC {
        mean[i] /= n;
    }

    let mut var = [0.0; N_MFCC];
    for z in &z_frames {
        for i in 0..N_MFCC {
            let diff = z[i] - mean[i];
            var[i] += diff * diff;
        }
    }
    for i in 0..N_MFCC {
        var[i] /= n;
    }

    Ok((mean, var))
}

pub struct TrackGateInputs {
    pub lufs: f32,
    pub crest_db: f32,
    pub slope: f32,
}

pub struct GateCriteria {
    pub lufs_min: f32,
    pub lufs_max: f32,
    pub crest_min_db: f32,
    pub slope_min: f32,
    pub slope_max: f32,
}

#[derive(Debug, PartialEq)]
pub enum RejectReason {
    LufsTooLow(f32, f32),
    LufsTooHigh(f32, f32),
    CrestTooLow(f32, f32),
    SlopeTooLow(f32, f32),
    SlopeTooHigh(f32, f32),
}

/// ALL rules evaluated in the fixed documented order LufsTooLow → LufsTooHigh →
/// CrestTooLow → SlopeTooLow → SlopeTooHigh; bounds INCLUSIVE (equal to bound = pass).
pub fn gate(track: &TrackGateInputs, c: &GateCriteria) -> Result<(), Vec<RejectReason>> {
    let mut rejects = Vec::new();

    if track.lufs < c.lufs_min {
        rejects.push(RejectReason::LufsTooLow(track.lufs, c.lufs_min));
    }
    if track.lufs > c.lufs_max {
        rejects.push(RejectReason::LufsTooHigh(track.lufs, c.lufs_max));
    }
    if track.crest_db < c.crest_min_db {
        rejects.push(RejectReason::CrestTooLow(track.crest_db, c.crest_min_db));
    }
    if track.slope < c.slope_min {
        rejects.push(RejectReason::SlopeTooLow(track.slope, c.slope_min));
    }
    if track.slope > c.slope_max {
        rejects.push(RejectReason::SlopeTooHigh(track.slope, c.slope_max));
    }

    if rejects.is_empty() {
        Ok(())
    } else {
        Err(rejects)
    }
}

/// gate-v1: deliberately loose — tightened only after measured corpus distributions (S-0XX §5)
pub const GATE_V1_IDM: GateCriteria = GateCriteria {
    // Pro-release ranges for IDM/Electronic
    lufs_min: -14.0,
    lufs_max: -6.0,
    // Anti-loudness-war crest floor
    crest_min_db: 4.0,
    // Pestana Table 2 slopes ±0.3 (Electronic anchor: -0.75)
    slope_min: -1.05,
    slope_max: -0.45,
};

/// gate-v1: identical to GATE_V1_IDM. Copied deliberately because the underlying
/// criteria (loudness, crest, slope) are sound for electronic/club music generally.
/// The IDM retirement was due to lossy MP3 source files, not bad thresholds.
pub const GATE_V1_TECHNO: GateCriteria = GateCriteria {
    lufs_min: -14.0,
    lufs_max: -6.0,
    crest_min_db: 4.0,
    slope_min: -1.05,
    slope_max: -0.45,
};

/// gate-v1.1: deliberately loose — tightened only after measured corpus distributions (S-0XX §5)
///
/// Retuned to gate-v1.1 (2026-07-09) from the first real-corpus measurement
/// run (24 curated acoustic tracks): the original anchors were
/// theoretically sound but excluded well-regarded 2010s+
/// acoustic-pop/indie-folk masters (First Aid Kit, Fleet Foxes,
/// Iron & Wine, Norah Jones) that the target bedroom-musician
/// audience recognizes as reference-quality. slope_max loosened
/// 0.15 (still excludes clearly bright pop mixes, e.g. measured
/// Sara Bareilles -0.61, Marc Broussard -0.67); lufs_max loosened
/// 1.5dB to the empirically-observed 2010s+ acoustic sweet spot
/// (still excludes Bareilles at -6.86). GATE_V1_IDM unchanged —
/// no real-corpus data yet to retune against.
pub const GATE_V1_ACOUSTIC: GateCriteria = GateCriteria {
    // Pro-release ranges for Acoustic
    lufs_min: -24.0,
    lufs_max: -8.5,
    // Anti-loudness-war crest floor
    crest_min_db: 8.0,
    // Pestana Table 2 slopes ±0.3 (Jazz anchor: -1.29, Folk anchor: -1.18)
    slope_min: -1.60,
    slope_max: -0.70,
};

/// Format as the shortest round-trip decimal.
/// Exists because generated constants once tripped clippy::excessive-precision (commit 956bbeb).
pub fn format_f32_const(v: f32) -> String {
    use alloc::format;
    let s = format!("{}", v);
    if !s.contains('.') && !s.contains('e') {
        format!("{}.0", s)
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mfcc::N_MFCC;

    #[test]
    fn test_compute_global_stats() {
        let frames = [[1.0; N_MFCC], [3.0; N_MFCC]];
        let (mean, std) = compute_global_stats(&frames).unwrap();

        for i in 0..N_MFCC {
            assert_eq!(mean[i], 2.0);
            // variance = ((1-2)^2 + (3-2)^2) / 2 = (1 + 1) / 2 = 1.0. std = 1.0
            assert_eq!(std[i], 1.0);
        }
    }

    #[test]
    fn test_compute_global_stats_empty() {
        let frames: [[f32; N_MFCC]; 0] = [];
        let err = compute_global_stats(&frames).unwrap_err();
        assert_eq!(err, CentroidError::EmptyInput);
    }

    #[test]
    fn test_compute_bucket_centroid() {
        let bucket = [[1.0; N_MFCC], [3.0; N_MFCC]];
        let global_mean = [2.0; N_MFCC];
        let global_std = [2.0; N_MFCC]; // z = (x - 2.0) / (2.0 + 1e-8)

        let (z_mean, z_var) = compute_bucket_centroid(&bucket, &global_mean, &global_std).unwrap();

        // z values: (1-2)/2 = -0.5, (3-2)/2 = 0.5
        // z_mean: (-0.5 + 0.5) / 2 = 0.0
        // z_var: ((-0.5 - 0)^2 + (0.5 - 0)^2) / 2 = (0.25 + 0.25) / 2 = 0.25

        for i in 0..N_MFCC {
            assert!((z_mean[i] - 0.0).abs() < 1e-6);
            assert!((z_var[i] - 0.25).abs() < 1e-6);
        }
    }

    #[test]
    fn test_compute_bucket_centroid_empty() {
        let bucket: [[f32; N_MFCC]; 0] = [];
        let global_mean = [2.0; N_MFCC];
        let global_std = [2.0; N_MFCC];
        let err = compute_bucket_centroid(&bucket, &global_mean, &global_std).unwrap_err();
        assert_eq!(err, CentroidError::EmptyInput);
    }

    #[test]
    fn test_sqrt_bit_identity() {
        // IEEE 754 requires correctly-rounded sqrt, so libm::sqrtf is bit-identical to std::f32::sqrt
        let values: [f32; 8] = [0.0, 0.25, 1.0, 2.0, 3.15, 100.0, 1e-8, 12_345.679];
        for &x in &values {
            let std_sqrt = x.sqrt();
            let libm_sqrt = libm::sqrtf(x);
            assert_eq!(
                std_sqrt.to_bits(),
                libm_sqrt.to_bits(),
                "sqrt mismatch for {}: std={} libm={}",
                x,
                std_sqrt,
                libm_sqrt
            );
        }
    }

    #[test]
    fn test_gate_criteria() {
        let criteria = GateCriteria {
            lufs_min: -14.0,
            lufs_max: -6.0,
            crest_min_db: 4.0,
            slope_min: -1.05,
            slope_max: -0.45,
        };

        // Pass
        let pass = TrackGateInputs {
            lufs: -10.0,
            crest_db: 6.0,
            slope: -0.75,
        };
        assert_eq!(gate(&pass, &criteria), Ok(()));

        // Fail LufsTooLow
        let fail_lufs_low = TrackGateInputs {
            lufs: -15.0,
            crest_db: 6.0,
            slope: -0.75,
        };
        assert_eq!(
            gate(&fail_lufs_low, &criteria),
            Err(vec![RejectReason::LufsTooLow(-15.0, -14.0)])
        );

        // Fail LufsTooHigh
        let fail_lufs_high = TrackGateInputs {
            lufs: -5.0,
            crest_db: 6.0,
            slope: -0.75,
        };
        assert_eq!(
            gate(&fail_lufs_high, &criteria),
            Err(vec![RejectReason::LufsTooHigh(-5.0, -6.0)])
        );

        // Fail CrestTooLow
        let fail_crest = TrackGateInputs {
            lufs: -10.0,
            crest_db: 3.0,
            slope: -0.75,
        };
        assert_eq!(
            gate(&fail_crest, &criteria),
            Err(vec![RejectReason::CrestTooLow(3.0, 4.0)])
        );

        // Fail SlopeTooLow
        let fail_slope_low = TrackGateInputs {
            lufs: -10.0,
            crest_db: 6.0,
            slope: -1.1,
        };
        assert_eq!(
            gate(&fail_slope_low, &criteria),
            Err(vec![RejectReason::SlopeTooLow(-1.1, -1.05)])
        );

        // Fail SlopeTooHigh
        let fail_slope_high = TrackGateInputs {
            lufs: -10.0,
            crest_db: 6.0,
            slope: -0.4,
        };
        assert_eq!(
            gate(&fail_slope_high, &criteria),
            Err(vec![RejectReason::SlopeTooHigh(-0.4, -0.45)])
        );

        // Multiple failures
        let fail_multi = TrackGateInputs {
            lufs: -15.0,
            crest_db: 3.0,
            slope: -0.4,
        };
        assert_eq!(
            gate(&fail_multi, &criteria),
            Err(vec![
                RejectReason::LufsTooLow(-15.0, -14.0),
                RejectReason::CrestTooLow(3.0, 4.0),
                RejectReason::SlopeTooHigh(-0.4, -0.45),
            ])
        );
    }

    #[test]
    fn test_gate_boundary_inclusive() {
        let criteria = GateCriteria {
            lufs_min: -14.0,
            lufs_max: -6.0,
            crest_min_db: 4.0,
            slope_min: -1.05,
            slope_max: -0.45,
        };

        // lufs exactly at lufs_min passes
        let pass = TrackGateInputs {
            lufs: -14.0,
            crest_db: 6.0,
            slope: -0.75,
        };
        assert_eq!(gate(&pass, &criteria), Ok(()));
    }

    #[test]
    fn test_gate_v1_1_acoustic_boundary() {
        // First Aid Kit (measured values: lufs -8.87649, slope -0.7214222)
        // Must PASS gate-v1.1 acoustic
        let fak = TrackGateInputs {
            lufs: -8.87649,
            crest_db: 10.0, // representative fixture value > crest_min_db
            slope: -0.7214222,
        };
        assert_eq!(
            gate(&fak, &GATE_V1_ACOUSTIC),
            Ok(()),
            "First Aid Kit must pass gate-v1.1"
        );

        // Sara Bareilles (measured values: lufs -6.863688, slope -0.6124779)
        // Must FAIL gate-v1.1 acoustic
        let bareilles = TrackGateInputs {
            lufs: -6.863688,
            crest_db: 10.0,
            slope: -0.6124779,
        };
        let rej = gate(&bareilles, &GATE_V1_ACOUSTIC).unwrap_err();
        assert!(rej.contains(&RejectReason::LufsTooHigh(-6.863688, -8.5)));
        assert!(rej.contains(&RejectReason::SlopeTooHigh(-0.6124779, -0.70)));
    }

    #[test]
    fn test_format_f32_const() {
        // Values that previously tripped clippy::excessive-precision
        let cases = [1.630_64, 0.1234567, -0.0001, 2.0, std::f32::consts::PI];

        for &val in &cases {
            let formatted = format_f32_const(val);
            let parsed: f32 = formatted.parse().unwrap();

            // Round-trip must be bit-identical
            assert_eq!(
                val.to_bits(),
                parsed.to_bits(),
                "Round-trip failed for {}: formatted as {}, parsed as {}",
                val,
                formatted,
                parsed
            );

            // Must contain a decimal point to be a valid Rust float literal
            assert!(
                formatted.contains('.'),
                "Formatted value {} lacks a decimal point",
                formatted
            );
        }
    }
}
