//! Stage 5 - Saturation (Harmonic Excitement)
//! Ported from sm-core. libm::tanhf only — no std::f32 transcendental methods.
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

    pub fn process_chunk(&mut self, chunk: &mut AudioChunk) {
        if self.drive <= 1.0 {
            return;
        }

        let inverse_drive = 1.0 / libm::tanhf(self.drive);

        for sample in chunk.samples.iter_mut() {
            *sample = libm::tanhf(*sample * self.drive) * inverse_drive;
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
    fn test_tanhf_unity_at_zero() {
        assert_eq!(libm::tanhf(0.0), 0.0);
    }

    #[test]
    fn test_tanhf_bounded() {
        for i in -1000..=1000 {
            let x = i as f32 * 0.1;
            let y = libm::tanhf(x);
            assert!(libm::fabsf(y) <= 1.0 + 1e-5, "tanhf({x}) = {y} - unbounded!");
        }
    }
}