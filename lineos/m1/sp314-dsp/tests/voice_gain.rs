use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::StftEngine;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use std::path::Path;

#[test]
fn test_voice_mask_gain() {
    let acoustic_path = Path::new("tests/fixtures/am_contra_30s.wav");
    assert!(acoustic_path.exists(), "Missing fixture: {} — in-repo tracked fixture — αν λείπει, το checkout είναι ελλιπές (git lfs / sparse checkout;)", acoustic_path.display());

    let mut reader = hound::WavReader::open(acoustic_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    let mut acoustic_sig = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            acoustic_sig.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
        }
    } else {
        acoustic_sig = samples;
    }
    acoustic_sig.truncate(spec.sample_rate as usize * 15);

    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout_with_profile(
        &acoustic_sig, 
        &acoustic_sig, 
        spec.sample_rate, 
        None, 
        None, 
        true, 
        Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music())
    );

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(&acoustic_sig);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    let mut mag_frames = vec![0.0_f32; 1025 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; 1025];
        for b in 0..1025 {
            let m = cplx_frames[f][b].norm();
            mag_frame[b] = m;
            mag_frames[f * 1025 + b] = m;
        }
        let mel_frame = fold_to_mel(&mag_frame);
        for b in 0..128 {
            c_v[b * n_frames + f] = mel_frame[b];
        }
    }

    let nmfd_k = 14;
    let init_h = vec![0.1_f32; nmfd_k * n_frames];

    let (nmfd_h, _) = nmfd_f32_h_only(
        &c_v,
        &scout.tensor_w,
        &init_h,
        128,
        nmfd_k,
        n_frames,
        scout.tau,
        12,
    );

    let nmfd_engine = NmfEngine::new(nmfd_k);
    let mask_old = nmfd_engine.nmfd_group_mask_chunk(&[0, 1, 2, 3], &nmfd_h, &scout.tensor_w, n_frames, 1025, scout.tau);
    let mask_new = nmfd_engine.nmfd_group_mask_chunk(&[0, 1, 2, 3, 7], &nmfd_h, &scout.tensor_w, n_frames, 1025, scout.tau);

    let mut energy_old = 0.0f64;
    let mut energy_new = 0.0f64;
    
    for f in 0..n_frames {
        for b in 0..1025 {
            let mag = mag_frames[f * 1025 + b] as f64;
            let v_old = mag * mask_old[f][b] as f64;
            let v_new = mag * mask_new[f][b] as f64;
            energy_old += v_old * v_old;
            energy_new += v_new * v_new;
        }
    }
    
    let rms_old = (energy_old / (n_frames * 1025) as f64).sqrt();
    let rms_new = (energy_new / (n_frames * 1025) as f64).sqrt();
    let gain_db = 20.0 * (rms_new / rms_old).log10();
    
    println!("=== VOICE MASK ENERGY GAIN (am_contra 15s) ===");
    println!("RMS OLD (Speech Only): {:.6}", rms_old);
    println!("RMS NEW (Speech+Sung): {:.6}", rms_new);
    println!("GAIN (dB): {:+.4} dB", gain_db);

    // Measured +2.13 dB baseline on 2026-08-18 (tolerance -0.5 dB)
    assert!(gain_db >= 2.13 - 0.5, "Voice mask energy gain regressed! Gain: {:.4} dB", gain_db);
}
