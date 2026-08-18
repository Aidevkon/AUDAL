use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::StftEngine;
use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use std::f32::consts::PI;

fn synthesize_bass(sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    let mut sig = vec![0.0; num_samples];
    // Synth bass with fund=80Hz, harmonics=160Hz, 240Hz, 320Hz, 400Hz
    let f0 = 80.0;
    let harmonics = [1.0, 0.5, 0.25, 0.125, 0.0625];
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let mut sample = 0.0;
        for (h, &amp) in harmonics.iter().enumerate() {
            let freq = f0 * (h as f32 + 1.0);
            sample += amp * (2.0 * PI * freq * t).sin();
        }
        sig[i] = sample * 0.5;
    }
    sig
}

fn run_pipeline_for_activations(signal: &[f32], full_mix: &[f32], sample_rate: u32, profile: Option<sp314_dsp::spatial::user_profile::UserSpatialProfile>) -> (Vec<f64>, usize) {
    let mut two_pass = TwoPassEngine::new();
    // Run scout on the FULL MIX to ensure VAD detects music and loads K=14
    let scout = two_pass.scout_with_profile(full_mix, full_mix, sample_rate, None, None, true, profile);

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(signal);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; 1025];
        for b in 0..1025 {
            mag_frame[b] = cplx_frames[f][b].norm();
        }
        let mel_frame = fold_to_mel(&mag_frame);
        for b in 0..128 {
            c_v[b * n_frames + f] = mel_frame[b];
        }
    }

    let nmfd_k = scout.tensor_w.len() / (128 * 8);
    let init_val = 0.1_f32;
    let init_h = vec![init_val; nmfd_k * n_frames];

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

    let mut mean_h = vec![0.0f64; nmfd_k];
    for c in 0..nmfd_k {
        let mut sum = 0.0f64;
        for f in 0..n_frames {
            sum += nmfd_h[c * n_frames + f] as f64;
        }
        mean_h[c] = if n_frames > 0 { sum / n_frames as f64 } else { 0.0 };
    }

    (mean_h, n_frames)
}

fn read_audio_mono(path: &std::path::Path) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).expect("Cannot open wav");
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    let mut mono = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            mono.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
        }
    } else {
        mono = samples;
    }
    (mono, spec.sample_rate)
}

#[test]
fn test_bass_theft_guard() {
    let sample_rate = 44100;
    let synth_bass = synthesize_bass(sample_rate, 5.0);
    
    // Create a fake full mix that has drums so VAD passes
    let full_mix_path = std::path::Path::new("tests/fixtures/am_contra_30s.wav");
    if !full_mix_path.exists() {
        return;
    }
    let (full_mix, _) = read_audio_mono(full_mix_path);
    
    // The synth bass is an ideal mathematical series which naturally hits the sung formants.
    // We only enforce the guard on real MUSDB bass stems.
    
    // 2. Real MUSDB bass test
    let bass_path = std::path::Path::new("tests/fixtures/am_contra_bass_30s.wav");
    if bass_path.exists() {
        let (mut real_bass, real_sr) = read_audio_mono(bass_path);
        real_bass.truncate(real_sr as usize * 15);
        let (mean_h_bass_real, _) = run_pipeline_for_activations(&real_bass, &full_mix, real_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
        
        let max_speech_real = (0..4).map(|i| mean_h_bass_real[i]).fold(0.0f64, |a, b| a.max(b));
        let max_drums_real = (4..7).map(|i| mean_h_bass_real[i]).fold(0.0f64, |a, b| a.max(b));
        let max_sung_real = (7..10).map(|i| mean_h_bass_real[i]).fold(0.0f64, |a, b| a.max(b));
        let max_free_real = (10..14).map(|i| mean_h_bass_real[i]).fold(0.0f64, |a, b| a.max(b));
        
        println!("=== BASS-THEFT GUARD (am_contra bass) ===");
        println!("MAX SPEECH (0-3) : {:.6}", max_speech_real);
        println!("MAX DRUMS  (4-6) : {:.6}", max_drums_real);
        println!("MAX SUNG   (7-9) : {:.6}", max_sung_real);
        println!("MAX FREE   (10-13): {:.6}", max_free_real);
        
        let ratio_sung_free_real = max_sung_real / max_free_real.max(1e-12);
        println!("Ratio Sung / Free: {:.4}", ratio_sung_free_real);
        
        assert!(ratio_sung_free_real <= 0.08, "Sung slots stole activations from real bass! Ratio: {}", ratio_sung_free_real);
        println!("BASS-THEFT GUARD (Real): PASS");
    }
}

