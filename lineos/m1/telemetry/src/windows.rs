//! Momentary + Short-term loudness windows (EBU R128 §2.2, §2.3)
//! These are point measurements from Golden Blob PCM output.
//! No DSP — pure window computation from already-processed samples.
//! All float math uses libm — no std::f32 methods.

use crate::lra::ms_to_lufs;

/// Momentary loudness: last 400ms window (EBU R128 §2.2).
/// Returns NEG_INFINITY if fewer samples than window size.
pub fn momentary_lufs(samples: &[f32], sample_rate: u32, channels: u16) -> f32 {
    // 400ms = sample_rate * 0.4 * channels samples
    let window = ((sample_rate as f32 * 0.4) as usize) * channels as usize;
    if samples.is_empty() || window == 0 || samples.len() < window {
        return f32::NEG_INFINITY;
    }
    let last = &samples[samples.len() - window..];
    ms_to_lufs(mean_square(last))
}

/// Short-term loudness: last 3s window (EBU R128 §2.3).
/// Returns NEG_INFINITY if fewer samples than window size.
pub fn short_term_lufs(samples: &[f32], sample_rate: u32, channels: u16) -> f32 {
    let window = sample_rate as usize * 3 * channels as usize;
    if samples.is_empty() || window == 0 || samples.len() < window {
        return f32::NEG_INFINITY;
    }
    let last = &samples[samples.len() - window..];
    ms_to_lufs(mean_square(last))
}

#[inline]
fn mean_square(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    sum / samples.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_momentary_too_short() {
        let samples = alloc::vec![0.5f32; 100]; // way too short
        let result = momentary_lufs(&samples, 48000, 2);
        assert!(result == f32::NEG_INFINITY);
    }

    #[test]
    fn test_short_term_too_short() {
        let samples = alloc::vec![0.5f32; 100];
        let result = short_term_lufs(&samples, 48000, 2);
        assert!(result == f32::NEG_INFINITY);
    }

    #[test]
    fn test_momentary_silence() {
        // Silence → ms = 0 → clamped to 1e-10 → very negative LUFS
        let samples = alloc::vec![0.0f32; 48000 * 2]; // 0.5s stereo
        let result = momentary_lufs(&samples, 48000, 2);
        assert!(result < -50.0);
    }

    #[test]
    fn test_short_term_loud_signal() {
        // 0dBFS sine → ms ≈ 0.5 → LUFS ≈ -3.7
        let sr = 48000u32;
        let ch = 2u16;
        let samples: Vec<f32> = (0..(sr * 4 * ch as u32) as usize)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / sr as f32))
            .collect();
        let result = short_term_lufs(&samples, sr, ch);
        // Should be meaningful LUFS value — not infinity, not silence
        assert!(
            result > -50.0 && result < 0.0,
            "ST LUFS out of range: {result}"
        );
    }
}
