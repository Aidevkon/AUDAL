/// W17 diagnostic: narrow down the exact second where flacenc bloat starts
use sp314_dsp::io::flac_encode::flac_encode;

fn encode_measure(samples: &[i32], channels: usize) -> usize {
    let mut f32_samples = Vec::with_capacity(samples.len());
    for &s in samples {
        f32_samples.push(s as f32 / 8388607.0);
    }
    let bytes = flac_encode(&f32_samples, 48000, channels as u32).expect("encode failed");
    bytes.len()
}

fn read_raw_i32(path: &str) -> Vec<i32> {
    let raw = std::fs::read(path).unwrap();
    raw.chunks_exact(4)
        .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
#[ignore]
fn w17_flacenc_narrow() {
    let samples = read_raw_i32("/tmp/w17_nonorm/h8_i32.raw");

    println!("\n=== Narrowing flacenc bloat (60s-90s) ===");

    // Second-by-second from 60 to 85
    for s in 60..=85 {
        let n = (48000 * s * 2).min(samples.len());
        let t = std::time::Instant::now();
        let result = encode_measure(&samples[..n], 2);
        let dur = t.elapsed();
        let ratio = result as f64 / (n as f64 * 3.0);
        let marker = if ratio > 1.5 { " ← BLOAT" } else { "" };
        println!(
            "first {}s ({} samples): {} bytes  ratio={:.4}  ({:.1}s){}",
            s, n, result, ratio, dur.as_secs_f64(), marker
        );
        if ratio > 1.5 {
            // Found the boundary! Now try block-by-block (4096 frames = 0.085s)
            let prev_s = s - 1;
            let prev_n = 48000 * prev_s * 2;
            // Try adding 1 second of data from the boundary
            let step = 48000 * 2;  // 1 second of interleaved stereo
            for frac in 0..10 {
                let test_n = prev_n + step * frac / 10;
                if test_n >= n { break; }
                let r = encode_measure(&samples[..test_n], 2);
                let ratio2 = r as f64 / (test_n as f64 * 3.0);
                let marker2 = if ratio2 > 1.5 { " ← BLOAT" } else { "" };
                println!(
                    "  {}s + {}/10: {} bytes  ratio={:.4}{}",
                    prev_s, frac, r, ratio2, marker2
                );
            }
            break;
        }
    }

    // Also test: if we SKIP the problematic region, does the rest encode fine?
    // Take samples from 80s-120s only
    let skip_start = 48000 * 80 * 2;
    let skip_end = samples.len();
    let tail = &samples[skip_start..skip_end];
    let result_tail = encode_measure(tail, 2);
    let ratio_tail = result_tail as f64 / (tail.len() as f64 * 3.0);
    println!(
        "\nTail only (80s-120s): {} bytes  ratio={:.4}",
        result_tail, ratio_tail
    );

    // And test: the problematic region alone (60s-80s)
    let mid_start = 48000 * 60 * 2;
    let mid_end = 48000 * 80 * 2;
    let mid = &samples[mid_start..mid_end];
    let result_mid = encode_measure(mid, 2);
    let ratio_mid = result_mid as f64 / (mid.len() as f64 * 3.0);
    println!(
        "Mid only (60s-80s): {} bytes  ratio={:.4}",
        result_mid, ratio_mid
    );
}
