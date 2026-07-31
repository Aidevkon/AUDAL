//! Spectral Reconstruction Oracle — 6α validation.
//!
//! Tests that `apply_spectral_mask_to_chunk` (spectral masking + iSTFT) produces
//! correct time-domain output, compared against the old broadband-scalar
//! `apply_mask_to_chunk` path.
//!
//! Four tests:
//!   a. NULL TEST: all-ones mask => reconstruction ≈ input
//!   b. PARTITION TEST: sum of 5 masked stems ≈ input
//!   c. DETERMINISM: twice => byte-equal
//!   d. SPECTRAL PROOF: two-tone separation vs scalar path

use sp314_dsp::stft::nmf::{NmfEngine, N_COMPONENTS};
use sp314_dsp::stft::two_pass::apply_spectral_mask_to_chunk;
use sp314_dsp::stft::{StftEngine, StreamingStftEncoder, FFT_SIZE, HOP_SIZE, N_BINS};

/// Generate a deterministic test signal: chirp + seeded noise.
/// Length chosen to exercise multiple STFT frames.
fn make_test_signal(n: usize) -> Vec<f32> {
    let mut signal = vec![0.0_f32; n];
    // Chirp: 200 Hz -> 4000 Hz over the signal length
    let f0 = 200.0_f32;
    let f1 = 4000.0_f32;
    let sr = 48000.0_f32;
    for i in 0..n {
        let t = i as f32 / sr;
        let phase = 2.0 * core::f32::consts::PI * t * (f0 + (f1 - f0) * t * sr / (2.0 * n as f32));
        signal[i] = 0.5 * libm::sinf(phase);
    }
    // Add deterministic "noise" (LCG)
    let mut seed: u32 = 42;
    for i in 0..n {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = (seed as f32 / u32::MAX as f32) * 2.0 - 1.0;
        signal[i] += 0.1 * noise;
    }
    signal
}

/// Forward STFT via StreamingStftEncoder (same path as process_single_chunk).
/// Returns complex frames.
fn forward_stft(signal: &[f32]) -> Vec<Vec<rustfft::num_complex::Complex<f32>>> {
    let mut enc = StreamingStftEncoder::new();
    let mut frames = enc.feed_chunk(signal);
    frames.extend(enc.finish());
    frames
}

/// Extract magnitudes from complex frames.
fn magnitudes(frames: &[Vec<rustfft::num_complex::Complex<f32>>]) -> Vec<Vec<f32>> {
    frames
        .iter()
        .map(|f| {
            f.iter()
                .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                .collect()
        })
        .collect()
}

/// Build all-ones mask with shape [n_frames][N_BINS].
fn ones_mask(n_frames: usize) -> Vec<Vec<f32>> {
    vec![vec![1.0_f32; N_BINS]; n_frames]
}

// ─────────────────────────────────────────────────────────────────────
// a. NULL TEST
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_null_mask_reconstruction() {
    let signal = make_test_signal(32768);
    let complex_frames = forward_stft(&signal);
    let n_frames = complex_frames.len();
    let mask = ones_mask(n_frames);

    let reconstructed = apply_spectral_mask_to_chunk(&complex_frames, &mask, signal.len());

    assert_eq!(
        reconstructed.len(),
        signal.len(),
        "Output length mismatch: got {} expected {}",
        reconstructed.len(),
        signal.len()
    );

    let mut max_abs_err = 0.0_f32;
    let mut max_abs_idx = 0usize;
    let mut sum_sq_err = 0.0_f64;
    let mut sum_sq_sig = 0.0_f64;

    for i in 0..signal.len() {
        let err = (reconstructed[i] - signal[i]).abs();
        if err > max_abs_err {
            max_abs_err = err;
            max_abs_idx = i;
        }
        sum_sq_err += (err as f64) * (err as f64);
        sum_sq_sig += (signal[i] as f64) * (signal[i] as f64);
    }

    let snr_db = if sum_sq_err > 0.0 {
        10.0 * (sum_sq_sig / sum_sq_err).log10()
    } else {
        f64::INFINITY
    };

    println!("NULL TEST (all-ones mask):");
    println!("  Signal length: {} samples", signal.len());
    println!("  STFT frames: {}", n_frames);
    println!(
        "  Max abs error: {:.6e} at sample {}",
        max_abs_err, max_abs_idx
    );
    println!("  SNR: {:.1} dB", snr_db);
    println!(
        "  Signal range: [{:.4}, {:.4}]",
        signal.iter().cloned().fold(f32::INFINITY, f32::min),
        signal.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
    );

    // Landscape printed — no threshold pinned yet (F-061 law).
}

// ─────────────────────────────────────────────────────────────────────
// b. PARTITION TEST
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_partition_sum_reconstruction() {
    let signal = make_test_signal(32768);
    let complex_frames = forward_stft(&signal);
    let mag_frames = magnitudes(&complex_frames);
    let n_frames = complex_frames.len();

    // Run NMF: fit on magnitudes, then get masks
    let mut nmf = NmfEngine::default();
    let w = nmf.fit(&mag_frames);
    let h = nmf.transform(&w, &mag_frames);
    nmf.h = h;

    // Verify masks sum to ~1.0 per bin
    let mut max_mask_sum_deviation = 0.0_f32;
    let masks: Vec<Vec<Vec<f32>>> = (0..N_COMPONENTS)
        .map(|c| nmf.component_mask_chunk(c, &nmf.h, n_frames, N_BINS))
        .collect();

    for f in 0..n_frames {
        for b in 0..N_BINS {
            let sum: f32 = masks.iter().map(|m| m[f][b]).sum();
            let dev = (sum - 1.0).abs();
            if dev > max_mask_sum_deviation {
                max_mask_sum_deviation = dev;
            }
        }
    }
    println!("PARTITION TEST:");
    println!(
        "  Max mask sum deviation from 1.0: {:.6e}",
        max_mask_sum_deviation
    );

    // Reconstruct each stem spectrally and sum
    let mut summed = vec![0.0_f32; signal.len()];
    for c in 0..N_COMPONENTS {
        let stem = apply_spectral_mask_to_chunk(&complex_frames, &masks[c], signal.len());
        for (s, &v) in summed.iter_mut().zip(stem.iter()) {
            *s += v;
        }
    }

    let mut max_abs_err = 0.0_f32;
    let mut sum_sq_err = 0.0_f64;
    let mut sum_sq_sig = 0.0_f64;
    for i in 0..signal.len() {
        let err = (summed[i] - signal[i]).abs();
        if err > max_abs_err {
            max_abs_err = err;
        }
        sum_sq_err += (err as f64) * (err as f64);
        sum_sq_sig += (signal[i] as f64) * (signal[i] as f64);
    }
    let snr_db = if sum_sq_err > 0.0 {
        10.0 * (sum_sq_sig / sum_sq_err).log10()
    } else {
        f64::INFINITY
    };

    println!(
        "  Sum of {} stems max abs error: {:.6e}",
        N_COMPONENTS, max_abs_err
    );
    println!("  Sum SNR: {:.1} dB", snr_db);
}

// ─────────────────────────────────────────────────────────────────────
// c. DETERMINISM
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_spectral_reconstruction_determinism() {
    let signal = make_test_signal(16384);
    let complex_frames = forward_stft(&signal);
    let mask = ones_mask(complex_frames.len());

    let out1 = apply_spectral_mask_to_chunk(&complex_frames, &mask, signal.len());
    let out2 = apply_spectral_mask_to_chunk(&complex_frames, &mask, signal.len());

    assert_eq!(out1.len(), out2.len());
    for (a, b) in out1.iter().zip(out2.iter()) {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "Determinism broken: spectral reconstruction differs"
        );
    }
    println!("DETERMINISM: byte-equal confirmed ({} samples)", out1.len());
}

// ─────────────────────────────────────────────────────────────────────
// d. SPECTRAL PROOF — two-tone separation
// ─────────────────────────────────────────────────────────────────────

#[test]
fn test_spectral_separation_proof() {
    let sr = 48000.0_f32;
    let n = 32768usize;

    // Two-tone signal: 440 Hz + 3000 Hz, equal amplitude
    let signal: Vec<f32> = (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            0.5 * libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t)
                + 0.5 * libm::sinf(2.0 * core::f32::consts::PI * 3000.0 * t)
        })
        .collect();

    let complex_frames = forward_stft(&signal);
    let n_frames = complex_frames.len();

    // Build a handcrafted mask: keep bins below 1000 Hz, zero above.
    // Bin frequency: b * (sr / FFT_SIZE) = b * 23.4375 Hz
    // 1000 Hz → bin 42.67 → use cutoff at bin 43
    let cutoff_bin = 43usize;
    let mut low_mask = vec![vec![0.0_f32; N_BINS]; n_frames];
    for f in 0..n_frames {
        for b in 0..N_BINS {
            low_mask[f][b] = if b <= cutoff_bin { 1.0 } else { 0.0 };
        }
    }

    // SPECTRAL path: per-bin masking + iSTFT
    let spectral_stem = apply_spectral_mask_to_chunk(&complex_frames, &low_mask, n);

    // SCALAR path: broadband scalar (old apply_mask_to_chunk behaviour)
    // Collapse mask to single weight per frame, multiply time-domain
    let scalar_stem: Vec<f32> = {
        let frame_weights: Vec<f32> = low_mask
            .iter()
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect();
        (0..n)
            .map(|i| {
                let pos = i as f32 * (n_frames - 1).max(1) as f32 / n.max(1) as f32;
                let idx0 = pos.floor() as usize;
                let idx1 = (idx0 + 1).min(frame_weights.len().saturating_sub(1));
                let frac = pos - idx0 as f32;
                let w0 = frame_weights.get(idx0).copied().unwrap_or(0.0);
                let w1 = frame_weights.get(idx1).copied().unwrap_or(0.0);
                let w = w0 * (1.0 - frac) + w1 * frac;
                signal[i] * w
            })
            .collect()
    };

    // Measure 3kHz energy in each path.
    // DFT energy around bin 128 (3000 / 23.4375 ≈ 128)
    // Use a proper STFT to measure energy in the 3kHz region.
    let spectral_3k_energy = measure_band_energy(&spectral_stem, sr, 2500.0, 3500.0);
    let scalar_3k_energy = measure_band_energy(&scalar_stem, sr, 2500.0, 3500.0);
    let original_3k_energy = measure_band_energy(&signal, sr, 2500.0, 3500.0);

    let spectral_440_energy = measure_band_energy(&spectral_stem, sr, 400.0, 500.0);
    let scalar_440_energy = measure_band_energy(&scalar_stem, sr, 400.0, 500.0);
    let original_440_energy = measure_band_energy(&signal, sr, 400.0, 500.0);

    let spectral_rejection_db = if spectral_3k_energy > 1e-20 {
        10.0 * (original_3k_energy / spectral_3k_energy).log10()
    } else {
        f64::INFINITY
    };
    let scalar_rejection_db = if scalar_3k_energy > 1e-20 {
        10.0 * (original_3k_energy / scalar_3k_energy).log10()
    } else {
        f64::INFINITY
    };

    let spectral_passband_db = if spectral_440_energy > 1e-20 {
        10.0 * (original_440_energy / spectral_440_energy).log10()
    } else {
        f64::INFINITY
    };
    let scalar_passband_db = if scalar_440_energy > 1e-20 {
        10.0 * (original_440_energy / scalar_440_energy).log10()
    } else {
        f64::INFINITY
    };

    println!("SPECTRAL PROOF (440 Hz + 3 kHz, low-pass mask < 1 kHz):");
    println!("  --- 3 kHz rejection (higher = better) ---");
    println!("    Spectral path: {:.1} dB", spectral_rejection_db);
    println!("    Scalar path:   {:.1} dB", scalar_rejection_db);
    println!(
        "    Δ (spectral advantage): {:.1} dB",
        spectral_rejection_db - scalar_rejection_db
    );
    println!("  --- 440 Hz passband loss (lower = better) ---");
    println!("    Spectral path: {:.2} dB", spectral_passband_db);
    println!("    Scalar path:   {:.2} dB", scalar_passband_db);
    println!("  --- Raw energy values ---");
    println!("    Original 3kHz:  {:.6e}", original_3k_energy);
    println!("    Spectral 3kHz:  {:.6e}", spectral_3k_energy);
    println!("    Scalar 3kHz:    {:.6e}", scalar_3k_energy);
    println!("    Original 440Hz: {:.6e}", original_440_energy);
    println!("    Spectral 440Hz: {:.6e}", spectral_440_energy);
    println!("    Scalar 440Hz:   {:.6e}", scalar_440_energy);
}

/// Measure energy in a frequency band using STFT analysis.
fn measure_band_energy(signal: &[f32], sr: f32, lo_hz: f32, hi_hz: f32) -> f64 {
    let mut engine = StftEngine::new();
    let (frames, n_frames) = engine.forward(signal);
    let bin_hz = sr / FFT_SIZE as f32;
    let lo_bin = (lo_hz / bin_hz).floor() as usize;
    let hi_bin = ((hi_hz / bin_hz).ceil() as usize).min(N_BINS - 1);

    let mut energy = 0.0_f64;
    for f in 0..n_frames {
        for b in lo_bin..=hi_bin {
            let mag =
                libm::sqrtf(frames[f][b].re * frames[f][b].re + frames[f][b].im * frames[f][b].im);
            energy += (mag as f64) * (mag as f64);
        }
    }
    energy
}
