// src/metering/gating.rs
// ITU-R BS.1770-4 block gating algorithm.
// Integrated LUFS = gated mean square → LUFS conversion.

pub const BLOCK_SAMPLES:    usize = 19_200; // 400ms @ 48kHz
pub const HOP_SAMPLES:      usize =  4_800; // 100ms hop (75% overlap)
pub const ABSOLUTE_GATE_DB: f32   =   -70.0_f32; // LUFS
pub const RELATIVE_GATE_LU: f32   =   -10.0_f32; // LU below ungated mean

/// Convert mean square value to LUFS.
/// Formula: LUFS = -0.691 + 10 * log10(mean_square)
/// The -0.691 offset is the ITU-R BS.1770-4 standard calibration constant.
#[inline]
pub fn mean_square_to_lufs(mean_square: f32) -> f32 {
    if mean_square < 1e-15_f32 {
        return -144.0_f32; // silence floor
    }
    -0.691_f32 + 10.0_f32 * libm::log10f(mean_square)
}

/// Convert LUFS to mean square (inverse of above).
#[inline]
pub fn lufs_to_mean_square(lufs: f32) -> f32 {
    libm::powf(10.0_f32, (lufs + 0.691_f32) / 10.0_f32)
}

/// Compute Integrated LUFS from a Vec of per-block mean squares.
/// Applies absolute gate (-70 LUFS) then relative gate (-10 LU).
/// Returns Integrated LUFS or -144.0 if no blocks survive gating.
pub fn integrated_lufs(block_mean_squares: &[f32]) -> f32 {
    if block_mean_squares.is_empty() {
        return -144.0_f32;
    }

    // Step 1: Absolute gate — discard blocks below -70 LUFS
    let absolute_threshold = lufs_to_mean_square(ABSOLUTE_GATE_DB);
    let above_absolute: Vec<f32> = block_mean_squares
        .iter()
        .filter(|&&ms| ms >= absolute_threshold)
        .cloned()
        .collect();

    if above_absolute.is_empty() {
        return -144.0_f32;
    }

    // Step 2: Ungated mean (from absolute-gated blocks only)
    let ungated_mean = above_absolute.iter().sum::<f32>() / above_absolute.len() as f32;
    let ungated_lufs = mean_square_to_lufs(ungated_mean);

    // Step 3: Relative gate — discard blocks 10 LU below ungated mean
    let relative_threshold = lufs_to_mean_square(ungated_lufs + RELATIVE_GATE_LU);
    let above_relative: Vec<f32> = above_absolute
        .iter()
        .filter(|&&ms| ms >= relative_threshold)
        .cloned()
        .collect();

    if above_relative.is_empty() {
        return -144.0_f32;
    }

    // Step 4: Final integrated LUFS from surviving blocks
    let final_mean = above_relative.iter().sum::<f32>() / above_relative.len() as f32;
    mean_square_to_lufs(final_mean)
}
