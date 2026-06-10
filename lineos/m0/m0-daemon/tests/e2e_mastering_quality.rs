//! E2E Mastering Quality & Auto-Tuning Test
//! Runs a multi-instrument mix through the STFT -> NMF -> Maestro Pipeline -> iSTFT and exports a WAV file.

use sp314_dsp::spatial::mid_side::MidSideMatrix;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::StftEngine;
use std::fs::File;
use std::io::Write;

// A dense 2-second mix triggering all DSP collision scenarios
fn generate_chaos_mix(sample_rate: u32) -> Vec<f32> {
    let n = (2.0 * sample_rate as f32) as usize;
    let mut phase_bass = 0.0_f32;
    let mut phase_synth = 0.0_f32;

    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sample_rate as f32;

        // S.2 / Maestro Trigger: 50Hz Kick at 0.5s and 1.5s
        let kick = if (t >= 0.5 && t < 0.6) || (t >= 1.5 && t < 1.6) {
            let env = libm::expf(-((t % 1.0) - 0.5) * 40.0);
            libm::sinf(2.0 * core::f32::consts::PI * 50.0 * t) * env
        } else {
            0.0
        };

        // Target: Sustained 150Hz Bass
        phase_bass += 2.0 * core::f32::consts::PI * 150.0 / sample_rate as f32;
        let bass = libm::sinf(phase_bass) * 0.6;

        // S.3 Trigger: Static 440Hz Synth
        phase_synth += 2.0 * core::f32::consts::PI * 440.0 / sample_rate as f32;
        let synth = libm::sinf(phase_synth) * 0.4;

        let l = (kick + bass + synth).clamp(-1.0, 1.0);
        let r = (kick + bass).clamp(-1.0, 1.0);
        out.push(l);
        out.push(r);
    }
    out
}

#[test]
fn test_e2e_maestro_render_to_wav() {
    let sample_rate = 48000;
    let signal = generate_chaos_mix(sample_rate);

    let (mid, side) = MidSideMatrix::encode(&signal);

    // 1. Forward STFT
    let mut stft = StftEngine::new();
    let (mut complex_frames, _n_frames_stft) = stft.forward(&mid);

    let mag_frames: Vec<Vec<f32>> = complex_frames
        .iter()
        .map(|frame| {
            frame
                .iter()
                .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                .collect()
        })
        .collect();

    // 2. NMF Matrix Factorization
    let mut nmf = NmfEngine::new(3);
    nmf.fit(&mag_frames);

    // 3. Apply Heuristic Psychoacoustic Matrix (The DSP Muscle)
    nmf.resolve_low_end_clash();
    nmf.resolve_formant_clash();
    nmf.resolve_high_end_clash();

    // 4. Identify Components (Simplified auto-detection for the test)
    let n_frames = mag_frames.len();
    let e0: f32 = nmf.h[0..n_frames].iter().sum();
    let e1: f32 = nmf.h[n_frames..2 * n_frames].iter().sum();
    let (kick_c, bass_c) = if e1 > e0 { (0, 1) } else { (1, 0) };

    // 5. Maestro AI Engine: Smart Ducking (Sidechain)
    nmf.apply_smart_ducking(kick_c, bass_c);

    // 6. Matrix Reconstruction (W * ducked H)
    let n_bins = 1025;
    let mut reconstructed_mags = vec![vec![0.0f32; n_bins]; n_frames];
    for f in 0..n_frames {
        for b in 0..n_bins {
            let mut sum = 0.0;
            for c in 0..3 {
                sum += nmf.w[b * 3 + c] * nmf.h[c * n_frames + f];
            }
            reconstructed_mags[f][b] = sum;
        }
    }

    // 7. iSTFT (Re-apply original phase to the new Maestro magnitudes)
    for f in 0..n_frames {
        for b in 0..n_bins {
            let mag = reconstructed_mags[f][b];
            let orig = complex_frames[f][b];
            let orig_mag = libm::sqrtf(orig.re * orig.re + orig.im * orig.im);
            if orig_mag > 0.0 {
                complex_frames[f][b].re = (orig.re / orig_mag) * mag;
                complex_frames[f][b].im = (orig.im / orig_mag) * mag;
            } else {
                complex_frames[f][b].re = 0.0;
                complex_frames[f][b].im = 0.0;
            }
        }
    }
    let mastered_signal = stft.inverse(&complex_frames, mid.len());
    let final_stereo = MidSideMatrix::decode(&mastered_signal, &side);

    // 8. WAV Export (Minimal RIFF/WAV header writer to avoid external dependencies)
    std::fs::create_dir_all("target").unwrap();
    let mut file = File::create("target/mastered_output_stereo.wav").expect("Failed to create WAV");
    let data_size = final_stereo.len() as u32 * 4; // 32-bit float

    // RIFF Header
    file.write_all(b"RIFF").unwrap();
    file.write_all(&(36u32 + data_size).to_le_bytes()).unwrap();
    file.write_all(b"WAVE").unwrap();

    // fmt Subchunk
    file.write_all(b"fmt ").unwrap();
    file.write_all(&16u32.to_le_bytes()).unwrap(); // Subchunk1Size
    file.write_all(&3u16.to_le_bytes()).unwrap(); // AudioFormat (3 = IEEE Float)
    file.write_all(&2u16.to_le_bytes()).unwrap(); // NumChannels (2 = Stereo)
    file.write_all(&sample_rate.to_le_bytes()).unwrap(); // SampleRate
    file.write_all(&(sample_rate * 8).to_le_bytes()).unwrap(); // ByteRate
    file.write_all(&8u16.to_le_bytes()).unwrap(); // BlockAlign
    file.write_all(&32u16.to_le_bytes()).unwrap(); // BitsPerSample

    // data Subchunk
    file.write_all(b"data").unwrap();
    file.write_all(&data_size.to_le_bytes()).unwrap();
    for sample in final_stereo {
        file.write_all(&sample.clamp(-1.0, 1.0).to_le_bytes())
            .unwrap();
    }

    println!("SUCCESS! Mastered WAV file written to: target/mastered_output_stereo.wav");

    // Quality Gate assertions
    // Read output WAV and verify mastering quality
    let mut reader =
        hound::WavReader::open("target/mastered_output_stereo.wav").expect("Output WAV not found");
    let spec = reader.spec();

    // Gate: correct format
    assert_eq!(spec.channels, 2, "Must be stereo");
    assert!(spec.sample_rate >= 44100, "Sample rate too low");

    // Gate: read samples and check bounds
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    assert!(!samples.is_empty(), "Output must not be empty");

    // Gate: no NaN or Inf
    for (i, &s) in samples.iter().enumerate() {
        assert!(s.is_finite(), "Sample[{}] = {} is NaN/Inf", i, s);
    }

    // Gate: peak within range — clamped before write
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    assert!(peak > 0.001, "Output too quiet: peak={:.4}", peak);
    assert!(peak <= 1.0, "Output clips after clamp: {:.4}", peak);

    // Gate: RMS above noise floor
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    assert!(rms > 0.001, "Output RMS too low: {:.4}", rms);

    println!("Quality Gate PASS: peak={:.3}, rms={:.4}", peak, rms);
}
