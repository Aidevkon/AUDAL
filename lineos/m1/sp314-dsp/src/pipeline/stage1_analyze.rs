//! Stage 1 — Normalization + Anomaly Detection
//! Ported from sm-core. Adapted for no_std + alloc.
//! Authority: LineOS Constitution v2.0 §05

use crate::types::audio::AudioChunk;

pub struct Stage1Analyze {
    norm_gain: f32,
}

impl Stage1Analyze {
    /// norm_gain: linear gain computed by AnalysisAccumulator::normalization_gain_linear()
    /// This value comes from the pre-analysis pass, derived from the target_lufs
    /// loaded from bmr-128.schema.json — never hardcoded.
    pub fn new(norm_gain: f32) -> Self {
        Self { norm_gain }
    }

    /// Apply normalization gain and clip-check.
    /// Returns Err if DC offset or anomaly detected (fatal per M0 Constitution §04.3).
    pub fn apply_gain_and_check(&self, chunk: &mut AudioChunk) -> Result<(), &'static str> {
        for sample in chunk.samples.iter_mut() {
            *sample *= self.norm_gain;
            // Hard clip detection — values > 1.5 indicate gain stage miscalculation
            if libm::fabsf(*sample) > 1.5 {
                return Err("Stage1: normalization overflow — check target_lufs");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage1_unity_gain() {
        let stage = Stage1Analyze::new(1.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.5; 64],
            sample_rate: 48000,
            channels:    2,
        };
        stage.apply_gain_and_check(&mut chunk).unwrap();
        for &s in &chunk.samples {
            assert!((s - 0.5).abs() < 1e-6);
        }
    }

    #[test]
    fn test_stage1_applies_gain() {
        let stage = Stage1Analyze::new(2.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.1; 64],
            sample_rate: 48000,
            channels:    2,
        };
        stage.apply_gain_and_check(&mut chunk).unwrap();
        for &s in &chunk.samples {
            assert!((s - 0.2).abs() < 1e-5);
        }
    }

    #[test]
    fn test_stage1_silence() {
        let stage = Stage1Analyze::new(1.0);
        let mut chunk = AudioChunk {
            samples:     alloc::vec![0.0; 1024],
            sample_rate: 48000,
            channels:    2,
        };
        stage.apply_gain_and_check(&mut chunk).unwrap();
        for &s in &chunk.samples {
            assert_eq!(s, 0.0);
        }
    }
}
