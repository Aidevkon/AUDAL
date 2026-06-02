use crate::pipeline::engine::{Sp314MasteringEngine, EngineConfig};
use crate::pipeline::telemetry::analyze_offline_pre_pass;
use crate::pipeline::presets::MasteringTarget;

pub const AUTOTUNE_MIN_GAIN_DB:    f32 = -12.0;
pub const AUTOTUNE_MAX_GAIN_DB:    f32 =  12.0;
pub const AUTOTUNE_MAX_ITERATIONS: usize = 10;
pub const AUTOTUNE_TOLERANCE_DB:   f32 =  0.1;
pub const AUTOTUNE_MAX_CLIP_RATIO: f32 =  0.01;  // 1% clip tolerance
pub const AUTOTUNE_CHUNK_SAMPLES:  usize = 96_000; // 2 seconds @ 48kHz

#[derive(Debug, Clone, Copy)]
pub struct AutotuneResult {
    pub pre_gain_db: f32,
    pub estimated_input_lufs: f32,
}

/// Scan the buffer with 1-second hop and return the index of the
/// highest-energy 2-second window.
/// Zero allocation — operates on slices.
pub fn find_highest_energy_chunk(left: &[f32], right: &[f32]) -> usize {
    let chunk = AUTOTUNE_CHUNK_SAMPLES.min(left.len());
    let hop   = 48_000_usize; // 1-second hop
    let mut best_start = 0usize;
    let mut best_energy = 0.0_f32;

    let mut start = 0;
    while start + chunk <= left.len() {
        let mut energy = 0.0_f32;
        for i in start..(start + chunk) {
            energy += left[i] * left[i] + right[i] * right[i];
        }
        if energy > best_energy {
            best_energy = energy;
            best_start  = start;
        }
        start += hop;
    }
    
    // If the loop didn't run (e.g. left.len() < chunk)
    // best_start remains 0, which is correct for short buffers.
    best_start
}

/// Count samples that hit the Engine's hard clipper (>= 0.9999) AFTER processing.
///
/// WHY post-process: measuring on raw signal × makeup_linear ignores
/// EQ boosts, compressor gain reduction, and parallel mix energy addition.
/// The engine hard-clips at ±1.0 — saturated samples land at exactly 1.0.
/// Threshold 0.9999 catches these without false positives on legitimate peaks.
pub fn measure_clipping_ratio_post_process(left: &[f32], right: &[f32]) -> f32 {
    let total = (left.len() + right.len()) as f32;
    if total == 0.0 {
        return 0.0;
    }
    let mut over = 0usize;
    for i in 0..left.len() {
        if libm::fabsf(left[i])  >= 0.9999_f32 { over += 1; }
        if libm::fabsf(right[i]) >= 0.9999_f32 { over += 1; }
    }
    over as f32 / total
}

/// Compute pre-gain to hit target LUFS.
/// Pure function — no IO, no file reading, no chunk estimation.
/// Uses PreAnalysis integrated_lufs (full track, EBU R128 gated).
/// Same input → same output always. INV-AB-1 preserved.
pub fn autotune(
    measured_lufs: f32,
    target_lufs:   f32,
) -> AutotuneResult {
    let pre_gain_db = if measured_lufs.is_finite() && target_lufs.is_finite() {
        (target_lufs - measured_lufs).clamp(-20.0, 20.0)
    } else {
        0.0  // safe fallback
    };
    
    AutotuneResult {
        pre_gain_db,
        estimated_input_lufs: measured_lufs,
    }
}
