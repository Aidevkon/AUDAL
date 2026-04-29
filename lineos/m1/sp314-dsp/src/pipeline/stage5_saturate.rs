//! Stage 5 - Saturation (Harmonic Excitement)
//! Ported from sm-core. No changes needed - already pure algorithmic, no std::f32.
//! fast_tanh: Pade approximation - deterministic, no external math library needed.
//! Authority: LineOS Constitution v2.0 §09.1

use crate::types::audio::AudioChunk;

pub struct Stage5Saturate {
    drive:     f32,
    _channels: u16,
}

impl Stage5Saturate {
    /// sat_drive_default from bmr-128.schema.json (1.3)
    pub fn new(drive: f32, channels: u16) -> Self {
        Self { drive, _channels: channels }
    }

    /// Fast Pade approximation of tanh(x) with hard clamp.
    /// Pade is accurate for |x| < ~2.0 (typical DSP signal range after compression).
    /// Values outside this range are clamped to [-1, 1] - saturation is the correct behavior.
    #[inline(always)]
    fn fast_tanh(x: f32) -> f32 {
        let x2 = x * x;
        let y  = x * (27.0 + x2) / (27.0 + 9.0 * x2);
        // Clamp to [-1, 1]: ensures bounded output regardless of input magnitude
        if y > 1.0 { 1.0 } else if y < -1.0 { -1.0 } else { y }
    }

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        if self.drive <= 1.0 {
            return;
        }

        let inverse_drive = 1.0 / Self::fast_tanh(self.drive);

        for sample in chunk.samples.iter_mut() {
            *sample = Self::fast_tanh(*sample * self.drive) * inverse_drive;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_saturate_silence() {
        let mut process = Stage5Saturate::new(1.3, 2);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        process.process_chunk(&mut chunk);
        for &s in &chunk.samples {
            assert!(s.abs() < 1e-6);
        }
    }

    #[test]
    fn test_fast_tanh_unity_at_zero() {
        assert_eq!(Stage5Saturate::fast_tanh(0.0), 0.0);
    }

    #[test]
    fn test_fast_tanh_bounded() {
        // fast_tanh must always return values in [-1, 1] (with clamp)
        for i in -1000..=1000 {
            let x = i as f32 * 0.1;
            let y = Stage5Saturate::fast_tanh(x);
            assert!(y.abs() <= 1.0 + 1e-5, "fast_tanh({x}) = {y} - unbounded!");
        }
    }
}