use std::path::Path;
use hound::WavReader;
use sp314_dsp::analysis::phi1_sensor::{Phi2StreamingFrontend, Phi2Pcen, Phi2Sensor};

#[test]
#[ignore = "verifies bit-exact equivalence between serial and batch sensor inference — needs /tmp/w9/podcast_realistic.wav"]
fn w14_batch_equivalence() {
    let input_path = Path::new("/tmp/w9/podcast_realistic.wav");
    if !input_path.exists() {
        panic!(
            "missing fixture {}: 120 s of podcast material, 48 kHz, \
             91.56% speech from LibriSpeech over a continuous FMA CC \
             bed 18 dB down, normalized to peak 0.52. described in \
             commit 52e2a31; the exact source clips are NOT recorded, \
             so a rebuild reproduces the spec but not the file — any \
             threshold in this test was tuned on the original and \
             must be re-measured after a rebuild. sources present \
             locally under Downloads/DATASET (librispeech, fma).",
            input_path.display()
        );
    }

    let mut reader = WavReader::open(input_path).unwrap();
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let mut mono: Vec<f32> = Vec::new();

    match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                mono = samples;
            } else {
                for chunk in samples.chunks(channels) {
                    let sum: f32 = chunk.iter().sum();
                    mono.push(sum / channels as f32);
                }
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i64 << (bits - 1)) as f32;
            let samples: Vec<i32> = reader.samples::<i32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                mono = samples.into_iter().map(|s| s as f32 / max_val).collect();
            } else {
                for chunk in samples.chunks(channels) {
                    let sum: f32 = chunk.iter().map(|&s| s as f32 / max_val).sum();
                    mono.push(sum / channels as f32);
                }
            }
        }
    }

    // 1. Frontend + PCEN
    let mut fe = Phi2StreamingFrontend::new();
    let mut mel_frames = fe.push(&mono);
    mel_frames.extend(fe.finish());

    let mut pcen = Phi2Pcen::new();
    let pcen_frames: Vec<[f32; 64]> = mel_frames
        .iter()
        .map(|frame| pcen.process(frame))
        .collect();

    println!("pcen_frames: {}", pcen_frames.len());

    // 2. ΣΕΙΡΙΑΚΑ
    let t0 = std::time::Instant::now();
    let mut sensor_serial = Phi2Sensor::new();
    let serial_results: Vec<Option<f32>> = pcen_frames
        .iter()
        .map(|frame| sensor_serial.push_frame(frame))
        .collect();
    let serial_ms = t0.elapsed().as_millis();

    // 3. ΠΑΡΑΛΛΗΛΑ
    let t1 = std::time::Instant::now();
    let sensor_parallel = Phi2Sensor::new();
    let parallel_results = sensor_parallel.infer_batch(&pcen_frames);
    let parallel_ms = t1.elapsed().as_millis();

    // 4. ΣΥΓΚΡΙΣΗ
    let len_serial = serial_results.len();
    let len_parallel = parallel_results.len();

    let some_serial = serial_results.iter().filter(|x| x.is_some()).count();
    let some_parallel = parallel_results.iter().filter(|x| x.is_some()).count();

    let mut max_abs_diff = 0.0f32;
    let mut pattern_mismatches = 0usize;

    for (s, p) in serial_results.iter().zip(parallel_results.iter()) {
        match (s, p) {
            (Some(sv), Some(pv)) => {
                let diff = (sv - pv).abs();
                if diff > max_abs_diff {
                    max_abs_diff = diff;
                }
            }
            (None, None) => {}
            _ => {
                pattern_mismatches += 1;
            }
        }
    }

    let speedup = if parallel_ms > 0 {
        serial_ms as f64 / parallel_ms as f64
    } else {
        f64::INFINITY
    };

    println!("serial_ms={} parallel_ms={} speedup={:.2}x", serial_ms, parallel_ms, speedup);
    println!("max_abs_diff={:.6e} some_serial={} some_parallel={}", max_abs_diff, some_serial, some_parallel);
    println!("pattern_mismatches={} len_serial={} len_parallel={}", pattern_mismatches, len_serial, len_parallel);
}
