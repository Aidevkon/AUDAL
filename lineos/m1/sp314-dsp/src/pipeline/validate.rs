//! Pipeline entry validation gates — sp314-dsp v2.9 §Preset Validation / §Input Validation Gate.
//! Authority: sp314-dsp-v2_9-spec.md v2.9

use crate::types::mastering_preset::MasteringPreset;

pub use crate::pipeline::stage_pool::MAX_POOL_FRAMES;

/// Validate preset configuration before any DSP stage runs.
pub fn validate_preset(preset: &MasteringPreset) -> Result<(), &'static str> {
    if preset.dither_bits != 32 && preset.dither_seed == 0 {
        return Err("dither_seed must be non-zero for sub-32-bit presets");
    }
    if preset.dither_bits != 16 && preset.dither_bits != 24 && preset.dither_bits != 32 {
        return Err("dither_bits must be 16, 24, or 32");
    }
    if preset.oversampling == 0 || (preset.oversampling & (preset.oversampling - 1)) != 0 {
        return Err("oversampling must be a power of 2 (1, 2, 4, 8)");
    }
    Ok(())
}

/// Validate interleaved PCM sample count before any DSP stage runs.
pub fn validate_input(pcm: &[f32]) -> Result<(), &'static str> {
    if pcm.len() > MAX_POOL_FRAMES * 2 {
        return Err("sp314-dsp: input exceeds maximum sample count");
    }
    Ok(())
}
