//! Deterministic test signal generator.
#![allow(dead_code)]
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
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            amp * libm::sinf(2.0 * core::f32::consts::PI * freq_hz * t)
        })
        .collect()
}

/// Generate white noise using a deterministic LCG.
/// seed: initial seed for reproducibility
pub fn white_noise(rms_dbfs: f32, duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let amp = libm::powf(10.0_f32, rms_dbfs / 20.0_f32);
    let mut state = seed;
    (0..n)
        .map(|_| {
            // LCG: Numerical Recipes parameters
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let normalized = (state >> 33) as f32 / (u32::MAX as f32) * 2.0 - 1.0;
            normalized * amp * 1.732 // scale to approximate target RMS
        })
        .collect()
}

/// Generate silence (all zeros).
pub fn silence(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    vec![0.0f32; (duration_s * sample_rate as f32) as usize]
}

/// Duplicate mono to stereo pair.
pub fn mono_to_stereo(mono: &[f32]) -> (Vec<f32>, Vec<f32>) {
    (mono.to_vec(), mono.to_vec())
}

/// Assert no NaN or Inf in PCM.
pub fn assert_no_nan_inf(pcm: &[f32], context: &str) {
    for (i, &x) in pcm.iter().enumerate() {
        assert!(
            x.is_finite(),
            "{}: sample[{}] = {} (NaN or Inf)",
            context,
            i,
            x
        );
    }
}

/// Assert all samples within [-1.0, 1.0].
pub fn assert_bounded(pcm: &[f32], context: &str) {
    for (i, &x) in pcm.iter().enumerate() {
        assert!(
            (-1.0..=1.0).contains(&x),
            "{}: sample[{}] = {} (out of [-1,1])",
            context,
            i,
            x
        );
    }
}

/// Simulate a kick drum: impulse + exponential decay + harmonics.
/// Models the broadband character of a real kick (sub thump + click).
pub fn kick_drum(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Sub thump: 60Hz with fast exponential decay
            let sub =
                libm::sinf(2.0 * core::f32::consts::PI * 60.0 * t) * libm::expf(-t * 30.0) * 0.8;
            // Body: 120Hz harmonic
            let body =
                libm::sinf(2.0 * core::f32::consts::PI * 120.0 * t) * libm::expf(-t * 50.0) * 0.4;
            // Click: 2kHz transient attack
            let click =
                libm::sinf(2.0 * core::f32::consts::PI * 2000.0 * t) * libm::expf(-t * 200.0) * 0.3;
            (sub + body + click).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Simulate a bass line: fundamental + rich harmonics.
/// Models a real bass guitar or 808 sustained note.
pub fn bass_line(freq_hz: f32, duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Fundamental
            let f1 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * t) * 0.6;
            // 2nd harmonic
            let f2 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * 2.0 * t) * 0.3;
            // 3rd harmonic
            let f3 = libm::sinf(2.0 * core::f32::consts::PI * freq_hz * 3.0 * t) * 0.15;
            (f1 + f2 + f3).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Simulate hi-hat: bandpass white noise (high frequency).
pub fn hi_hat(duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    // White noise — hi-hats are broadband but high frequency
    // The NMF will separate this into high-centroid component
    white_noise(-6.0, duration_s, sample_rate, seed)
}

/// Generate a snare drum with a mathematically perfect exponential reverb tail.
/// hit: sharp transient (fast decay).
/// tail: broadband noise decaying via e^(-alpha * t).
pub fn snare_with_reverb(duration_s: f32, sample_rate: u32, seed: u64) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut state = seed;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // LCG noise
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((state >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0;

            // Transient Hit: sharp 20ms decay
            let hit = noise * libm::expf(-t * 150.0);
            // Reverb Tail: 1.0s RT60 (alpha ~ 6.9)
            let tail = noise * libm::expf(-t * 6.9) * 0.5;

            (hit + tail).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate a mix where Kick and Bass are at the EXACT same frequency (50Hz).
/// kick: fast decaying 50Hz sine.
/// bass: sustained 50Hz sine.
pub fn kick_and_bass_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Kick: 50Hz transient (decays in ~50ms)
            let kick = libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * libm::expf(-t * 40.0);
            // Bass: Sustained 50Hz note (starts after 100ms to avoid phase cancellation at hit)
            let bass = if t > 0.1 {
                libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * 0.5
            } else {
                0.0
            };

            (kick + bass).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate a mix where a Hi-Hat and Vocal Sibilance ("S") share the exact same high-frequency noise profile.
pub fn hihat_and_sibilance_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    // FIXED: Use u32 to properly overflow and generate noise
    let mut state = 42u32;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // 32-bit LCG
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;

            let hat_env = if t > 0.1 && t < 0.15 {
                libm::expf(-(t - 0.1) * 100.0)
            } else {
                0.0
            };
            let sib_env = if t > 0.4 && t < 0.55 {
                libm::expf(-(t - 0.4) * 20.0)
            } else {
                0.0
            };

            (noise * hat_env + noise * sib_env * 0.8).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate a mix where a Static Synth and a Vibrating Vocal share the exact same base frequency (440Hz).
/// synth: Perfectly static 440Hz sine wave (0.1s to 0.4s).
/// vocal: 440Hz sine wave with 15Hz vibrato at 6Hz rate (0.6s to 0.9s).
pub fn vocal_and_synth_clash(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut phase_static = 0.0_f32;
    let mut phase_vib = 0.0_f32;

    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;

            // Synth: Static 440Hz
            let synth_env = if t > 0.1 && t < 0.4 { 1.0 } else { 0.0 };
            phase_static += 2.0 * core::f32::consts::PI * 440.0 / sample_rate as f32;
            let synth = libm::sinf(phase_static) * synth_env;

            // Vocal: 440Hz base + 15Hz Depth Vibrato at 6Hz Rate
            let vib_env = if t > 0.6 && t < 0.9 { 1.0 } else { 0.0 };
            let current_freq = 440.0 + 15.0 * libm::sinf(2.0 * core::f32::consts::PI * 6.0 * t);
            phase_vib += 2.0 * core::f32::consts::PI * current_freq / sample_rate as f32;
            let vocal = libm::sinf(phase_vib) * vib_env;

            (synth + vocal).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Generate a clean mix: 50Hz Kick transient and 150Hz Sustained Bass.
pub fn clean_kick_and_bass(duration_s: f32, sample_rate: u32) -> Vec<f32> {
    let n = (duration_s * sample_rate as f32) as usize;
    let mut phase_bass = 0.0_f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Kick: 50Hz transient at t=0.1s
            let kick = if t >= 0.1 {
                libm::sinf(2.0 * core::f32::consts::PI * 50.0 * (t - 0.1))
                    * libm::expf(-(t - 0.1) * 40.0)
            } else {
                0.0
            };
            // Bass: Sustained 150Hz
            phase_bass += 2.0 * core::f32::consts::PI * 150.0 / sample_rate as f32;
            let bass = libm::sinf(phase_bass) * 0.5;
            (kick + bass).clamp(-1.0, 1.0)
        })
        .collect()
}
