/// W17 diagnostic: isolate flacenc 0.3.1 bloat bug.
///
/// h8.flac = 1,287,664,344 bytes from 11,520,000 samples (37× raw 24-bit PCM).
/// Same signal re-encoded by libsndfile → 10,449,041 bytes (normal).
/// This test reproduces the issue and narrows the trigger.
use sp314_dsp::io::flac_encode::flac_encode;

fn encode_and_measure(samples: &[i32], channels: usize, sample_rate: usize) -> usize {
    let mut f32_samples = Vec::with_capacity(samples.len());
    for &s in samples {
        f32_samples.push(s as f32 / 8388607.0);
    }
    let bytes = flac_encode(&f32_samples, sample_rate as u32, channels as u32).expect("encode failed");
    bytes.len()
}

fn read_raw_i32(path: &str) -> Vec<i32> {
    let raw = std::fs::read(path).unwrap();
    raw.chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
#[ignore = "διαγνωστικό του flacenc 0.3.1 bloat (δες doc): θέλει /tmp/w17_nonorm/h8_i32.raw + h1_i32.raw (F-072). ΑΓΝΩΣΤΟΣ ΤΡΟΠΟΣ ΑΝΑΚΑΤΑΣΚΕΥΗΣ — χειροκίνητο fixture (F-078). Το ξυπνά: scripts/audio_wire.sh · scripts/run-ignored.sh"]
fn w17_flacenc_bloat_repro() {
    let h8_path = "/tmp/w17_nonorm/h8_i32.raw";
    let h1_path = "/tmp/w17_nonorm/h1_i32.raw";

    assert!(std::path::Path::new(h8_path).exists(), "Missing fixture: {h8_path} (+ {h1_path}) — ΑΓΝΩΣΤΟΣ ΤΡΟΠΟΣ ΑΝΑΚΑΤΑΣΚΕΥΗΣ — χειροκίνητο fixture (F-078), δεν βρέθηκε συνταγή");

    let samples_h8 = read_raw_i32(h8_path);
    let samples_h1 = read_raw_i32(h1_path);

    println!("\n=== flacenc bloat reproduction ===");
    println!("h8 samples: {}", samples_h8.len());
    println!("h1 samples: {}", samples_h1.len());

    // Step 1: Full h8 signal
    let t = std::time::Instant::now();
    let result_h8 = encode_and_measure(&samples_h8, 2, 48000);
    let h8_dur = t.elapsed();
    println!(
        "h8 FULL: output_bytes={} ratio={:.4} time={:.1}s",
        result_h8,
        result_h8 as f64 / (samples_h8.len() as f64 * 3.0),
        h8_dur.as_secs_f64()
    );

    // Step 2: Full h1 signal (should be normal)
    let t = std::time::Instant::now();
    let result_h1 = encode_and_measure(&samples_h1, 2, 48000);
    let h1_dur = t.elapsed();
    println!(
        "h1 FULL: output_bytes={} ratio={:.4} time={:.1}s",
        result_h1,
        result_h1 as f64 / (samples_h1.len() as f64 * 3.0),
        h1_dur.as_secs_f64()
    );

    // Step 3: Binary search for bloat trigger — shorten h8
    // Encode progressively longer slices to find where bloat starts
    let frames_10s = 48000 * 10 * 2;  // 10 seconds worth of interleaved samples
    for seconds in [5, 10, 30, 60, 90, 120] {
        let n = (48000 * seconds * 2).min(samples_h8.len());
        let t = std::time::Instant::now();
        let result = encode_and_measure(&samples_h8[..n], 2, 48000);
        let dur = t.elapsed();
        let ratio = result as f64 / (n as f64 * 3.0);
        let bloat = if ratio > 1.5 { "BLOAT!" } else { "ok" };
        println!(
            "h8 first {}s ({} samples): output_bytes={} ratio={:.4} time={:.1}s {}",
            seconds, n, result, ratio, dur.as_secs_f64(), bloat
        );

        // If this is already bloated, find the exact second
        if ratio > 1.5 && seconds > 5 {
            let prev_s = match seconds {
                10 => 5,
                30 => 10,
                60 => 30,
                90 => 60,
                120 => 90,
                _ => 0,
            };
            for s in (prev_s+1)..seconds {
                let n2 = (48000 * s * 2).min(samples_h8.len());
                let r2 = encode_and_measure(&samples_h8[..n2], 2, 48000);
                let ratio2 = r2 as f64 / (n2 as f64 * 3.0);
                if ratio2 > 1.5 {
                    println!("  → Bloat starts at {}s: output={} ratio={:.4}", s, r2, ratio2);
                    break;
                }
            }
            break;  // Found the bloat region
        }
    }

    // Step 4: Test if mono vs stereo matters
    let mono_h8: Vec<i32> = samples_h8.iter().step_by(2).copied().collect();
    let result_mono = encode_and_measure(&mono_h8, 1, 48000);
    println!(
        "h8 MONO: output_bytes={} ratio={:.4}",
        result_mono,
        result_mono as f64 / (mono_h8.len() as f64 * 3.0)
    );
}
