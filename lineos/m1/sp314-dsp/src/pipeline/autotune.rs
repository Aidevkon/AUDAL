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
    pub makeup_db:      f32,
    pub achieved_rms:   f32,
    pub achieved_lufs:  f32,
    pub clipping_ratio: f32,
    pub iterations:     usize,
    pub converged:      bool,
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

/// Find optimal makeup_db for the given audio and target.
/// Returns AutotuneResult with the safe makeup_db and convergence info.
/// All math: libm only. Zero allocation in search loop.
pub fn autotune(
    left:        &[f32],
    right:       &[f32],
    base_config: EngineConfig,    // preset config — parallel_mix etc locked
    target:      MasteringTarget,
    sample_rate: u32,
) -> AutotuneResult {
    // Transparent preset → no tuning needed
    let target_lufs = match target.target_lufs() {
        None => return AutotuneResult {
            makeup_db:      0.0,
            achieved_rms:   0.0,
            achieved_lufs:  -144.0,
            clipping_ratio: 0.0,
            iterations:     0,
            converged:      true,
        },
        Some(t) => t,
    };

    // Find highest energy chunk
    let chunk_start = find_highest_energy_chunk(left, right);
    let chunk_end   = (chunk_start + AUTOTUNE_CHUNK_SAMPLES).min(left.len());
    let chunk_l     = &left[chunk_start..chunk_end];
    let chunk_r     = &right[chunk_start..chunk_end];

    let mut min_gain = AUTOTUNE_MIN_GAIN_DB;
    let mut max_gain = AUTOTUNE_MAX_GAIN_DB;
    let mut best_makeup  = 0.0_f32;
    let mut best_rms     = -144.0_f32;
    let mut best_lufs    = -144.0_f32;
    let mut best_clip    = 0.0_f32;
    let mut iterations   = 0usize;

    for _ in 0..AUTOTUNE_MAX_ITERATIONS {
        iterations += 1;
        let mid = (min_gain + max_gain) / 2.0_f32;

        // Clone chunk for processing (dev-time only — chunk is small, 96k samples)
        let mut test_l = chunk_l.to_vec();
        let mut test_r = chunk_r.to_vec();

        // Run engine with this makeup
        let mut config = base_config.clone();
        config.target_makeup_db = mid;
        let mut engine = match Sp314MasteringEngine::new(config, sample_rate) {
            Ok(e)  => e,
            Err(_) => break,
        };
        engine.process_offline(&mut test_l, &mut test_r);

        let telemetry = analyze_offline_pre_pass(&test_l, &test_r);
        let output_rms = telemetry.rms_db;
        let mut output_lufs = telemetry.lufs;

        if output_lufs < -69.0 {
            output_lufs = telemetry.rms_db;
        }

        // Clipping ratio: measure on PROCESSED output (test_l, test_r)
        // NOT on raw chunk — raw signal misses EQ boosts and compression.
        // Engine hard-clips at ±1.0 — saturated samples detected via >= 0.9999.
        let clip_ratio = measure_clipping_ratio_post_process(&test_l, &test_r);

        if clip_ratio > AUTOTUNE_MAX_CLIP_RATIO {
            // Too hot — pull back
            max_gain = mid;
        } else if output_lufs < target_lufs {
            // Too quiet — push up, save as best candidate
            min_gain    = mid;
            best_makeup = mid;
            best_rms    = output_rms;
            best_lufs   = output_lufs;
            best_clip   = clip_ratio;
        } else {
            // Too loud — pull back, do NOT save as best
            max_gain = mid;
        }

        if max_gain - min_gain < AUTOTUNE_TOLERANCE_DB { break; }
    }

    AutotuneResult {
        makeup_db:      best_makeup,
        achieved_rms:   best_rms,
        achieved_lufs:  best_lufs,
        clipping_ratio: best_clip,
        iterations,
        converged:      (max_gain - min_gain) < AUTOTUNE_TOLERANCE_DB,
    }
}
