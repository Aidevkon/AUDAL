use hound::WavReader;
use std::path::Path;
use sp314_dsp::analysis::phi1_sensor::{Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor};
use sp314_dsp::stft::two_pass::TwoPassEngine;

#[ignore]
#[test]
fn test_gamma_voice_leak() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        println!("SKIPPED: {:?} missing", input_path);
        return;
    }

    let mut reader = WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    let mut mono = Vec::new();
    if spec.sample_format == hound::SampleFormat::Float {
        let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
        if spec.channels == 2 {
            for i in 0..(samples.len() / 2) {
                mono.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
            }
        } else {
            mono = samples;
        }
    } else {
        let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
        if spec.channels == 2 {
            for i in 0..(samples.len() / 2) {
                mono.push((samples[2 * i] as f32 + samples[2 * i + 1] as f32) * 0.5 / 32768.0);
            }
        } else {
            mono = samples.into_iter().map(|s| s as f32 / 32768.0).collect();
        }
    }
    let sample_rate = spec.sample_rate;

    // 1. Τρέξε τον Φ2
    let mut fe = Phi2StreamingFrontend::new();
    let mut pcen = Phi2Pcen::new();
    let mut sensor = Phi2Sensor::new();

    let frames1 = fe.push(&mono);
    let frames2 = fe.finish();

    let mut p_vals = Vec::new();
    for mel in frames1.iter().chain(frames2.iter()) {
        let pcen_frame = pcen.process(mel);
        if let Some(p) = sensor.push_frame(&pcen_frame) {
            p_vals.push(p);
        }
    }

    // 2. Τρέξε το two_pass με use_nmfd=true και use_nmfd=false
    let mut engine = TwoPassEngine::new();
    
    for &use_nmfd in &[true, false] {
        let scout = engine.scout(&mono, &mono, sample_rate, None, None, use_nmfd);
        
        let mut voice_stem = Vec::new();
        engine.process_chunks(&mono, &scout, use_nmfd, |chunk| {
            voice_stem.extend_from_slice(&chunk.voice.mono());
        }).unwrap();

        // 3. ΕΥΘΥΓΡΑΜΜΙΣΕ
        let frame_size = 480;
        let mut speech_frames = 0;
        let mut no_speech_frames = 0;
        let mut speech_energy_sum = 0.0f64;
        let mut no_speech_energy_sum = 0.0f64;
        let mut total_energy = 0.0f64;

        for (i, &p) in p_vals.iter().enumerate() {
            let start = i * frame_size;
            let end = (start + frame_size).min(voice_stem.len());
            if start >= end {
                break;
            }

            let slice = &voice_stem[start..end];
            let mut sum_sq = 0.0f64;
            for &s in slice {
                sum_sq += (s * s) as f64;
            }
            
            total_energy += sum_sq;

            // 4. Χώρισε σε δύο ομάδες
            if p >= 0.5 {
                speech_frames += 1;
                speech_energy_sum += sum_sq;
            } else if p < 0.3 {
                no_speech_frames += 1;
                no_speech_energy_sum += sum_sq;
            }
        }

        // 5. ΤΥΠΩΣΕ
        let speech_mean_rms = if speech_frames > 0 {
            (speech_energy_sum / (speech_frames * frame_size) as f64).sqrt()
        } else {
            0.0
        };
        let no_speech_mean_rms = if no_speech_frames > 0 {
            (no_speech_energy_sum / (no_speech_frames * frame_size) as f64).sqrt()
        } else {
            0.0
        };

        let ratio_db = if no_speech_mean_rms > 1e-9 && speech_mean_rms > 1e-9 {
            20.0 * (speech_mean_rms / no_speech_mean_rms).log10()
        } else {
            0.0
        };

        let no_speech_energy_pct = if total_energy > 1e-12 {
            (no_speech_energy_sum / total_energy) * 100.0
        } else {
            0.0
        };

        println!("use_nmfd={}", use_nmfd);
        println!("  SPEECH (p >= 0.5):   {:>6} frames, Mean RMS: {:.6}", speech_frames, speech_mean_rms);
        println!("  NO_SPEECH (p < 0.3): {:>6} frames, Mean RMS: {:.6}", no_speech_frames, no_speech_mean_rms);
        println!("  Ratio (SPEECH / NO_SPEECH): {:.2} dB", ratio_db);
        println!("  NO_SPEECH Energy %: {:.1}%\n", no_speech_energy_pct);
    }
}
