use hound::WavReader;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader;
use sp314_dsp::dsp::control_bus::Ducker;
use std::path::{Path, PathBuf};
use std::fs;

struct TestMemorySource {
    data: Vec<f32>,
    offset: usize,
}

impl sp314_dsp::stft::sliding_overlap_reader::ChunkSource for TestMemorySource {
    fn channels(&self) -> usize {
        2
    }
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let remain = self.data.len() - self.offset;
        if remain == 0 {
            return Ok(0);
        }
        let to_read = remain.min(buffer.len());
        buffer[..to_read].copy_from_slice(&self.data[self.offset..self.offset + to_read]);
        self.offset += to_read;
        Ok(to_read / 2)
    }
}

fn find_test_wav() -> Option<PathBuf> {
    let primary = Path::new("/tmp/w7a/beds/Skelpolu_-_Resurrection__other.wav");
    if primary.exists() {
        return Some(primary.to_path_buf());
    }
    let dir = Path::new("/tmp/w7a/beds");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("wav") {
                return Some(path);
            }
        }
    }
    None
}

fn find_w6b_mix_wav() -> Option<PathBuf> {
    let dir = Path::new("/tmp/w6b");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.file_name().and_then(|s| s.to_str()).map_or(false, |s| s.ends_with("__snrp3")) {
                let mix_path = path.join("mix.wav");
                if mix_path.exists() {
                    return Some(mix_path);
                }
            }
        }
    }
    None
}

fn dz(p: f32, t: f32) -> f32 {
    if p < t { 0.0 } else { (p - t) / (1.0 - t) }
}

fn shifted_logistic(p: f32, p0: f32, k: f32) -> f32 {
    let sig = |x: f32| 1.0 / (1.0 + (-x).exp());
    let base = sig(-k * p0);
    let num = sig(k * (p - p0)) - base;
    (num / (1.0 - base)).max(0.0)
}

#[test]
fn test_phi1_duck_compare() {
    let wav_path = match find_test_wav() {
        Some(p) => p,
        None => {
            println!("SKIPPED: no wav files found in /tmp/w7a/beds/");
            return;
        }
    };

    let mut reader = WavReader::open(&wav_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32).collect(),
    };

    let mono_signal: Vec<f32> = if spec.channels == 2 {
        samples.chunks_exact(2).map(|c| (c[0] + c[1]) * 0.5).collect()
    } else {
        samples.clone()
    };

    let interleaved: Vec<f32> = if spec.channels == 1 {
        let mut inter = Vec::with_capacity(samples.len() * 2);
        for s in samples.iter() {
            inter.push(*s);
            inter.push(*s);
        }
        inter
    } else {
        samples.clone()
    };

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono_signal, spec.sample_rate, None, None, false);

    let reader_on = SlidingOverlapReader::new(
        TestMemorySource {
            data: interleaved,
            offset: 0,
        },
        10240,
    );

    let mut all_dsp_p: Vec<f32> = Vec::new();
    let mut all_phi1_p: Vec<f32> = Vec::new();

    let mut observer = |obs: sp314_dsp::analysis::vad_model::VadObservation| {
        all_dsp_p.push(obs.posterior);
        all_phi1_p.push(obs.phi1_p.unwrap_or(f32::NAN));
    };

    let _ = engine
        .process_stream_with_params(
            reader_on,
            &scout,
            1.0,
            false,
            false,
            &[],
            spec.sample_rate as f32,
            None,
            Some(&mut observer),
            true, // phi1_enabled: true
            |_chunk| {},
        )
        .unwrap();

    let mut speech_phi1_p: Vec<(u64, f32)> = Vec::new();

    if let Some(w6b_path) = find_w6b_mix_wav() {
        let mut reader = WavReader::open(&w6b_path).unwrap();
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
            hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32).collect(),
        };

        let mono_signal: Vec<f32> = if spec.channels == 2 {
            samples.chunks_exact(2).map(|c| (c[0] + c[1]) * 0.5).collect()
        } else {
            samples.clone()
        };

        let interleaved: Vec<f32> = if spec.channels == 1 {
            let mut inter = Vec::with_capacity(samples.len() * 2);
            for s in samples.iter() {
                inter.push(*s);
                inter.push(*s);
            }
            inter
        } else {
            samples.clone()
        };

        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&mono_signal, spec.sample_rate, None, None, false);

        let reader_on = SlidingOverlapReader::new(
            TestMemorySource {
                data: interleaved,
                offset: 0,
            },
            10240,
        );

        let mut observer = |obs: sp314_dsp::analysis::vad_model::VadObservation| {
            if let Some(p) = obs.phi1_p {
                speech_phi1_p.push((obs.frame_index, p));
            } else {
                speech_phi1_p.push((obs.frame_index, f32::NAN));
            }
        };

        let _ = engine
            .process_stream_with_params(
                reader_on,
                &scout,
                1.0,
                false,
                false,
                &[],
                spec.sample_rate as f32,
                None,
                Some(&mut observer),
                true, // phi1_enabled: true
                |_chunk| {},
            )
            .unwrap();
    }

    println!("\nShifted Logistic vs Deadzone comparison on speech mix (w6b):");
    println!("curve                   | MEAN g IN | MIN g IN  | MEAN g OUT | frames OUT g<0.99");

    let speech_curves: &[(&str, Box<dyn Fn(f32) -> f32>)] = &[
        ("deadzone 0.30", Box::new(|p| dz(p, 0.30))),
        ("shifted p0=0.35 k=12", Box::new(|p| shifted_logistic(p, 0.35, 12.0))),
        ("shifted p0=0.35 k=20", Box::new(|p| shifted_logistic(p, 0.35, 20.0))),
        ("shifted p0=0.30 k=25", Box::new(|p| shifted_logistic(p, 0.30, 25.0))),
    ];

    for (name, f) in speech_curves {
        let mut d = Ducker::new(spec.sample_rate as f32);
        let mut sum_g_in = 0.0f64;
        let mut min_g_in = f32::INFINITY;
        let mut count_in = 0usize;

        let mut sum_g_out = 0.0f64;
        let mut count_out = 0usize;
        let mut count_out_99 = 0usize;

        for &(frame_idx, p) in &speech_phi1_p {
            let p_trans = if p.is_finite() { f(p) } else { f32::NAN };
            let g = d.update(p_trans);

            if frame_idx >= 1000 && frame_idx < 2000 {
                sum_g_in += g as f64;
                if g < min_g_in { min_g_in = g; }
                count_in += 1;
            } else {
                sum_g_out += g as f64;
                if g < 0.99 { count_out_99 += 1; }
                count_out += 1;
            }
        }

        let mean_g_in = if count_in > 0 { sum_g_in / count_in as f64 } else { 0.0 };
        let mean_g_out = if count_out > 0 { sum_g_out / count_out as f64 } else { 0.0 };

        println!("{:23} | {:.6}  | {:.6}  | {:.6}   | {:17}", name, mean_g_in, min_g_in, mean_g_out, count_out_99);
    }

    println!("\nShifted Logistic vs Deadzone comparison on pure music bed (Skelpolu):");
    println!("curve                   | MEAN g (bed) | frames g<0.99 (bed)");

    for (name, f) in speech_curves {
        let mut d = Ducker::new(spec.sample_rate as f32);
        let mut sum_g = 0.0f64;
        let mut count_99 = 0usize;
        let mut n = 0usize;
        for &p in &all_phi1_p {
            let p_trans = if p.is_finite() { f(p) } else { f32::NAN };
            let g = d.update(p_trans);
            sum_g += g as f64;
            if g < 0.99 { count_99 += 1; }
            n += 1;
        }
        let mean_g = if n > 0 { sum_g / n as f64 } else { 0.0 };
        println!("{:23} | {:.6}       | {:19}", name, mean_g, count_99);
    }

    println!("\nPoint evaluation of curves (target g = 1 - 0.749 * f(p)):");
    let points = [0.00f32, 0.05, 0.10, 0.20, 0.30, 0.35, 0.50, 0.70, 0.90];
    print!("p                   : ");
    for &p in &points {
        print!("{:6.2} ", p);
    }
    println!();

    print!("deadzone 0.30       : ");
    for &p in &points {
        let g = 1.0 - 0.749 * dz(p, 0.30);
        print!("{:6.4} ", g);
    }
    println!();

    print!("shifted 0.35/k20    : ");
    for &p in &points {
        let g = 1.0 - 0.749 * shifted_logistic(p, 0.35, 20.0);
        print!("{:6.4} ", g);
    }
    println!();
}
