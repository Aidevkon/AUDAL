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

fn run_pipeline_for_activations(signal: &[f32], sample_rate: u32) -> (Vec<f64>, usize) {
    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout(signal, sample_rate, None, None, true);

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

    let nmfd_k = 8;
    let nmfd_iter = 12;
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
#[ignore]
fn w17_voice_activation() {
    let speech_path = Path::new("/tmp/w17_pure/speech_only.wav");
    let music_path = Path::new("/tmp/w17_pure/music_only.wav");

    if !speech_path.exists() || !music_path.exists() {
        println!("SKIPPED: /tmp/w17_pure files missing");
        return;
    }

    let (speech_sig, speech_sr) = read_audio_mono(speech_path);
    let (music_sig, music_sr) = read_audio_mono(music_path);

    let (mean_h_speech, _) = run_pipeline_for_activations(&speech_sig, speech_sr);
    let (mean_h_music, _) = run_pipeline_for_activations(&music_sig, music_sr);

    println!("\n{:<5} | {:>18} | {:>17} | {:>8}", "comp", "mean_h_ΣΤΟ_SPEECH", "mean_h_ΣΤΟ_MUSIC", "ratio");
    println!("{}", "-".repeat(56));

    for c in 0..8 {
        let ms = mean_h_speech[c];
        let mm = mean_h_music[c];
        let ratio = if mm > 1e-12 { ms / mm } else { 0.0 };
        println!("{:<5} | {:>18.6} | {:>17.6} | {:>8.2}", c, ms, mm, ratio);
    }
}
