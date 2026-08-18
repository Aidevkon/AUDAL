use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::StftEngine;
use std::path::Path;

fn read_audio_mono(path: &Path) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).expect(&format!("Cannot open wav: {:?}", path));
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

fn run_pipeline_for_activations(signal: &[f32], sample_rate: u32, profile: Option<sp314_dsp::spatial::user_profile::UserSpatialProfile>) -> (Vec<f64>, usize) {
    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout_with_profile(signal, signal, sample_rate, None, None, true, profile);

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
    let nmfd_iter = 12;
    let init_val = 0.1_f32;
    let init_h = vec![init_val; nmfd_k * n_frames];

    for c in 0..nmfd_k {
        let mut sum_w = 0.0_f64;
        for m in 0..128 {
            for tau in 0..scout.tau {
                sum_w += scout.tensor_w[m * nmfd_k * scout.tau + c * scout.tau + tau] as f64;
            }
        }
        println!("DEBUG W sum for slot {}: {:.6}", c, sum_w);
    }

    let (nmfd_h, _) = nmfd_f32_h_only(
        &c_v,
        &scout.tensor_w,
        &init_h,
        128,
        nmfd_k,
        n_frames,
        scout.tau,
        nmfd_iter,
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

#[test]
fn test_w18_nmfd_guard() {
    let speech_path = Path::new("/tmp/w9/podcast_realistic.wav");
    let music_path = Path::new("/tmp/blue/comp0.wav");

    if !speech_path.exists() {
        println!("SKIPPED: speech missing");
        return;
    }
    
    // Take 30s max
    let (mut speech_sig, speech_sr) = read_audio_mono(speech_path);
    speech_sig.truncate(speech_sr as usize * 30);
    
    let (mean_h_speech, _) = run_pipeline_for_activations(&speech_sig, speech_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_podcast()));

    println!("=== PODCAST ACTIVATIONS ===");
    for c in 0..mean_h_speech.len() {
        println!("Slot {}: {:.6}", c, mean_h_speech[c]);
    }
    
    let max_speech = (0..4).map(|i| mean_h_speech[i]).fold(0.0f64, |a, b| a.max(b));
    let max_drums = if mean_h_speech.len() > 6 { (4..7).map(|i| mean_h_speech[i]).fold(0.0f64, |a, b| a.max(b)) } else { 0.0 };
    
    let ratio = max_drums / max_speech.max(1e-12);
    println!("MAX SPEECH (0-3): {:.6}", max_speech);
    println!("MAX DRUMS (4-6) (Free slots): {:.6}", max_drums);
    
    assert_eq!(mean_h_speech.len(), 8, "Podcast should completely bypass drum templates");
    println!("PODCAST GUARD: PASS (bypassed)");
    
    if music_path.exists() {
        let (mut music_sig, music_sr) = read_audio_mono(music_path);
        music_sig.truncate(music_sr as usize * 30);
        
        let (mean_h_music, _) = run_pipeline_for_activations(&music_sig, music_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
        assert_eq!(mean_h_music.len(), 11, "Music should load 3 drum templates");
        
        let max_drums = (4..7).map(|i| mean_h_music[i]).fold(0.0f64, |a, b| a.max(b));
        assert!(max_drums < 1e-5, "Synthetic track should have near-zero drum activations, got {:e}", max_drums);
        println!("SYNTHETIC DRUMS GUARD: PASS");
    }
    
    let acoustic_path = Path::new("tests/fixtures/am_contra_30s.wav");
    if acoustic_path.exists() {
        let (mut acoustic_sig, acoustic_sr) = read_audio_mono(acoustic_path);
        
        let (mean_h_ac, _) = run_pipeline_for_activations(&acoustic_sig, acoustic_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
        assert_eq!(mean_h_ac.len(), 11, "Music should load 3 drum templates");
        
        let max_drums = (4..7).map(|i| mean_h_ac[i]).fold(0.0f64, |a, b| a.max(b));
        assert!(max_drums > 1e-6, "Acoustic track should have healthy drum activations, got {:e}", max_drums);
        println!("ACOUSTIC DRUMS GUARD: PASS");
    }
}
