use hound;
use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::{StftEngine, N_BINS};
use std::fs;
use std::path::Path;

fn read_wav_mono(path: &Path) -> (Vec<f32>, u32) {
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

fn write_wav_mono_f32(path: &Path, samples: &[f32], sample_rate: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        writer.write_sample(s).unwrap();
    }
}

fn compute_rms(signal: &[f32]) -> f32 {
    if signal.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = signal.iter().map(|&x| x * x).sum();
    (sum_sq / signal.len() as f32).sqrt()
}

/// ΔΙΟΡΘΩΘΗΚΕ: το test έδινε ΟΛΟΚΛΗΡΟ το αρχείο στο
/// scout, ενώ το προϊόν δίνει τα πρώτα 30s
/// (dsp_pipeline.rs:449). Η διαφορά είναι δραματική:
/// scout(ΟΛΟ) → harmonics 500Hz-2k = 18.0%,
/// scout(30s) → 86.2%. Με σταθερό K=8, περισσότερο
/// υλικό δίνει πιο ΓΕΝΙΚΑ components. Οι μετρήσεις
/// του W17 που βασίστηκαν σε αυτό το test ΔΕΝ
/// αντιστοιχούσαν στο προϊόν.
#[test]
#[ignore]
fn w17_stem_spectrum() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: /tmp/w9/podcast_realistic.wav missing");
        return;
    }

    let out_dir = Path::new("/tmp/w17_groups");
    fs::create_dir_all(out_dir).unwrap();

    let (signal, sample_rate) = read_wav_mono(input_path);
    let mut two_pass = TwoPassEngine::new();
    let scout_window = (30 * sample_rate as usize).min(signal.len());
    let scout = two_pass.scout(&signal[..scout_window], &signal[..scout_window], sample_rate, None, None, true);

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(&signal);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; N_BINS];
        for b in 0..N_BINS {
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

    let nmfd_engine = NmfEngine::new(nmfd_k);

    let groupings = vec![
        ("G_all", vec![0, 1, 2, 3], vec![scout.nmfd_harmonics_idx]),
        ("G_top2", vec![0, 2], vec![scout.nmfd_harmonics_idx, 1, 3]),
        ("G_top3", vec![0, 2, 3], vec![scout.nmfd_harmonics_idx, 1]),
    ];

    println!("\n{:<7} | {:>7} | {:>7} | {:>7} | {:>7} | {:>7} | {:>9}",
        "group", "<150", "150-500", "500-2k", "2k-6k", ">6k", "voice_rms");
    println!("{}", "-".repeat(68));

    for (name, voice_comps, harm_comps) in groupings {
        let voice_mask = nmfd_engine.nmfd_group_mask_chunk(
            &voice_comps,
            &nmfd_h,
            &scout.tensor_w,
            n_frames,
            N_BINS,
            scout.tau,
        );

        let harm_mask = if harm_comps.len() == 1 {
            nmfd_engine.nmfd_component_mask_chunk(
                harm_comps[0],
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            )
        } else {
            nmfd_engine.nmfd_group_mask_chunk(
                &harm_comps,
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            )
        };

        let mut voice_cplx = cplx_frames.clone();
        let mut harm_cplx = cplx_frames.clone();

        for f in 0..n_frames {
            for b in 0..N_BINS {
                let vm = voice_mask[f][b].min(1.0);
                voice_cplx[f][b].re *= vm;
                voice_cplx[f][b].im *= vm;

                let hm = harm_mask[f][b].min(1.0);
                harm_cplx[f][b].re *= hm;
                harm_cplx[f][b].im *= hm;
            }
        }

        let mut stft_v = StftEngine::new();
        let voice_sig = stft_v.inverse(&voice_cplx, signal.len());
        let voice_rms = compute_rms(&voice_sig);

        let mut stft_h = StftEngine::new();
        let harm_sig = stft_h.inverse(&harm_cplx, signal.len());

        // Write wav files
        write_wav_mono_f32(&out_dir.join(format!("{}_voice.wav", name)), &voice_sig, sample_rate);
        write_wav_mono_f32(&out_dir.join(format!("{}_harmonics.wav", name)), &harm_sig, sample_rate);

        // Compute frequency band energies for harmonics
        let mut energies = [0.0f64; 5];
        for f in 0..n_frames {
            for b in 0..N_BINS {
                let freq = b as f32 * sample_rate as f32 / 2048.0;
                let cplx = harm_cplx[f][b];
                let mag_sq = (cplx.re * cplx.re + cplx.im * cplx.im) as f64;

                if freq < 150.0 {
                    energies[0] += mag_sq;
                } else if freq < 500.0 {
                    energies[1] += mag_sq;
                } else if freq < 2000.0 {
                    energies[2] += mag_sq;
                } else if freq < 6000.0 {
                    energies[3] += mag_sq;
                } else {
                    energies[4] += mag_sq;
                }
            }
        }

        let total_e: f64 = energies.iter().sum();
        let pcts: Vec<f64> = if total_e > 1e-12 {
            energies.iter().map(|&e| e / total_e * 100.0).collect()
        } else {
            vec![0.0; 5]
        };

        println!("{:<7} | {:>6.1}% | {:>6.1}% | {:>6.1}% | {:>6.1}% | {:>6.1}% | {:>9.6}",
            name, pcts[0], pcts[1], pcts[2], pcts[3], pcts[4], voice_rms);
    }

    // Generate HTML A/B player
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>W17 Stem Groupings A/B Audition</title>
    <style>
        body { font-family: sans-serif; padding: 20px; background: #121212; color: #e0e0e0; }
        .section { margin-bottom: 30px; padding: 20px; background: #1e1e1e; border-radius: 8px; }
        h1 { color: #fff; }
        h2 { color: #bb86fc; margin-top: 0; }
        .player { margin: 10px 0; padding: 10px; background: #2d2d2d; border-radius: 4px; }
        audio { width: 100%; margin-top: 5px; }
    </style>
</head>
<body>
    <h1>W17 Voice Groupings Audition</h1>
    
    <div class="section">
        <h2>VOICE STEMS</h2>
        <div class="player">
            <strong>G_all (components [0,1,2,3])</strong>
            <audio controls loop src="G_all_voice.wav"></audio>
        </div>
        <div class="player">
            <strong>G_top3 (components [0,2,3])</strong>
            <audio controls loop src="G_top3_voice.wav"></audio>
        </div>
        <div class="player">
            <strong>G_top2 (components [0,2])</strong>
            <audio controls loop src="G_top2_voice.wav"></audio>
        </div>
    </div>

    <div class="section">
        <h2>HARMONICS STEMS</h2>
        <div class="player">
            <strong>G_all Harmonics (component [6])</strong>
            <audio controls loop src="G_all_harmonics.wav"></audio>
        </div>
        <div class="player">
            <strong>G_top3 Harmonics (components [6,1])</strong>
            <audio controls loop src="G_top3_harmonics.wav"></audio>
        </div>
        <div class="player">
            <strong>G_top2 Harmonics (components [6,1,3])</strong>
            <audio controls loop src="G_top2_harmonics.wav"></audio>
        </div>
    </div>
</body>
</html>"#;

    fs::write(out_dir.join("index.html"), html).unwrap();
    println!("Wrote stems and HTML player to /tmp/w17_groups/");
}
