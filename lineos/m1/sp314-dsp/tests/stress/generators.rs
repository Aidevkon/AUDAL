//! Deterministic test signal generator.
//! All functions use deterministic math — no rand::thread_rng().
//! Same seed → same output on all platforms.

/// Generate a pure sine wave.
/// freq_hz: frequency in Hz
/// amp_dbfs: amplitude in dBFS (0.0 = full scale)
/// duration_s: duration in seconds
/// sample_rate: samples per second
pub fn sine(freq_hz: f32, amp_dbfs: f32, duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let amp = libm::powf(10.0_f32, amp_dbfs / 20.0_f32);
    (0..n).map(|i| {
        let t = i as f32 / sample_rate as f32;
        amp * libm::sinf(2.0 * core::f32::consts::PI * freq_hz * t)
    }).collect()
}

/// Generate white noise using a deterministic LCG.
/// seed: initial seed for reproducibility
pub fn white_noise(rms_dbfs: f32, duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let amp = libm::powf(10.0_f32, rms_dbfs / 20.0_f32);
    let mut state = seed;
    (0..n).map(|_| {
        // LCG: Numerical Recipes parameters
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let normalized = (state >> 33) as f32 / (u32::MAX as f32) * 2.0 - 1.0;
        normalized * amp * 1.732 // scale to approximate target RMS
    }).collect()
}

/// Generate silence (all zeros).
pub fn silence(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    vec![0.0f32; (duration_s * sample_rate as f32) as usize]
}

/// Duplicate mono to stereo pair.
pub fn mono_to_stereo(mono: &[f32]) -> (Vec<f32>, Vec<f32>) {
    (mono.to_vec(), mono.to_vec())
}

/// Create anti-phase stereo (L = signal, R = -signal).
pub fn mono_to_antiphase_stereo(mono: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let r: Vec<f32> = mono.iter().map(|&s| -s).collect();
    (mono.to_vec(), r)
}

/// Assert no NaN or Inf in PCM.
pub fn assert_no_nan_inf(pcm: &[f32], context: &str) {
    for (i, &x) in pcm.iter().enumerate() {
        assert!(x.is_finite(),
            "{}: sample[{}] = {} (NaN or Inf)", context, i, x);
    }
}

/// Assert all samples within [-1.0, 1.0].
pub fn assert_bounded(pcm: &[f32], context: &str) {
    for (i, &x) in pcm.iter().enumerate() {
        assert!(x >= -1.0 && x <= 1.0,
            "{}: sample[{}] = {} (out of [-1,1])", context, i, x);
    }
}
