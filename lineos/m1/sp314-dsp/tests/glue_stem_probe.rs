use hound::WavReader;
use std::path::{Path, PathBuf};
use sp314_dsp::analysis::phi1_sensor::{Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor};

#[ignore]
#[test]
fn test_glue_stem_probe() {
    let files = [
        "/tmp/glue_amb/nmf5/ambience.wav",
        "/tmp/glue_amb/nmf5/harmonics.wav",
        "/tmp/glue_amb/nmf5/vocals.wav",
        "/tmp/glue_amb/nmfd8/ambience.wav",
        "/tmp/glue_amb/nmfd8/harmonics.wav",
        "/tmp/glue_amb/nmfd8/vocals.wav",
    ];

    println!("{:<35} | {:<7} | {:<7} | {:<8} | {:<8}", "file", "p_mean", "p_max", "ratio_05", "ratio_09");
    println!("{:-<73}", "");

    for file in files.iter() {
        if !Path::new(file).exists() {
            println!("{:<35} | SKIPPED", file);
            continue;
        }

        let mut reader = WavReader::open(file).unwrap();
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

        if p_vals.is_empty() {
            continue;
        }

        let mut p_mean = 0.0;
        let mut p_max = 0.0f32;
        let mut r05 = 0;
        let mut r09 = 0;

        for &p in &p_vals {
            p_mean += p;
            if p > p_max { p_max = p; }
            if p > 0.5 { r05 += 1; }
            if p > 0.9 { r09 += 1; }
        }
        p_mean /= p_vals.len() as f32;
        let ratio_05 = r05 as f32 / p_vals.len() as f32;
        let ratio_09 = r09 as f32 / p_vals.len() as f32;

        let name = Path::new(file).iter().rev().take(2).collect::<Vec<_>>().into_iter().rev().collect::<PathBuf>();
        
        println!("{:<35} | {:<7.4} | {:<7.4} | {:<8.4} | {:<8.4}", 
                 name.display(), p_mean, p_max, ratio_05, ratio_09);
    }
}
