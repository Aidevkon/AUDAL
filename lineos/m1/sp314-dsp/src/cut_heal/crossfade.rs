//! Crossfade primitive for stitching processed and passthrough audio.

use std::f32::consts::PI;

/// Default crossfade length in samples (256 samples = ~5.3ms at 48kHz).
/// This is a starting value; the A/B test may re-pin it.
pub const DEFAULT_XFADE: usize = 256;

/// Equal-power crossfade between two audio buffers.
///
/// Output length will be exactly `a.len()`. Asserts `a.len() == b.len()`.
/// The first `fade_len` samples blend `a -> b` using an equal-power law:
/// `g_a = cos(t * PI/2)` and `g_b = sin(t * PI/2)` for `t` in `[0, 1)`.
/// After `fade_len`, the output is pure `b`.
///
/// **Why equal-power over linear?**
/// Correlated signals sum linearly, but uncorrelated signals sum in power.
/// Our processed and passthrough signals are near-correlated in the null case,
/// but decorrelated where spectral masks act. Equal-power is the safer default
/// for maintaining constant perceived energy in uncorrelated sections. We measure
/// its cost on correlated inputs (the "bump") in the oracle test.
pub fn equal_power_crossfade(a: &[f32], b: &[f32], fade_len: usize) -> Vec<f32> {
    assert_eq!(
        a.len(),
        b.len(),
        "Crossfade inputs must have identical length"
    );
    assert!(
        fade_len <= a.len(),
        "Fade length cannot exceed input length"
    );

    let mut out = Vec::with_capacity(a.len());

    for i in 0..fade_len {
        let t = (i as f32) / (fade_len as f32);
        let angle = t * (PI / 2.0);
        let g_a = angle.cos();
        let g_b = angle.sin();
        out.push(a[i] * g_a + b[i] * g_b);
    }

    for i in fade_len..a.len() {
        out.push(b[i]);
    }

    out
}

/// Equal-gain crossfade between two audio buffers.
///
/// Output length will be exactly `a.len()`. Asserts `a.len() == b.len()`.
/// The first `fade_len` samples blend `a -> b` using a linear ramp (equal-gain):
/// `g_a = 1.0 - t` and `g_b = t` for `t` in `[0, 1)`.
/// After `fade_len`, the output is pure `b`.
///
/// **Division of labor (Chosen by the MEASURED +3.01dB null bump, 2026-08-02):**
/// * Equal-gain for correlated pairs (our processed/passthrough seams).
/// * Equal-power for uncorrelated material.
pub fn equal_gain_crossfade(a: &[f32], b: &[f32], fade_len: usize) -> Vec<f32> {
    assert_eq!(
        a.len(),
        b.len(),
        "Crossfade inputs must have identical length"
    );
    assert!(
        fade_len <= a.len(),
        "Fade length cannot exceed input length"
    );

    let mut out = Vec::with_capacity(a.len());

    for i in 0..fade_len {
        let t = (i as f32) / (fade_len as f32);
        let g_a = 1.0 - t;
        let g_b = t;
        out.push(a[i] * g_a + b[i] * g_b);
    }

    for i in fade_len..a.len() {
        out.push(b[i]);
    }

    out
}
