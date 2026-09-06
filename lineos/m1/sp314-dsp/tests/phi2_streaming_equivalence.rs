use hound::WavReader;
use sp314_dsp::analysis::phi1_sensor::{Phi1MelFrontend, Phi2StreamingFrontend};

#[test]
fn test_phi2_streaming_equivalence() {
    let wav_path = "/tmp/phi1/eq_test_48k.wav";
    // ⚠ F-071: ΠΕΡΝΑΕΙ ΚΕΝΟ ΣΤΟ CI. Το fixture
    // /tmp/phi1/eq_test_48k.wav δεν υπάρχει και ΔΕΝ ΑΝΑΠΑΡΑΓΕΤΑΙ
    // (καμία συνταγή πουθενά στο repo — μετρήθηκε 25/08).
    // ΔΕΝ γίνεται panic ΓΙΑΤΙ τρέχει σε τρία CI workflows με
    // cargo test --workspace (ci.yml:59 · constitutional-gates.yml:61 ·
    // red-freeze.yml:37)· μόνιμα κόκκινο CI είναι ο ίδιος μηχανισμός με
    // μόνιμα πράσινο ψεύτικο.
    // ΞΥΠΝΑΕΙ ΟΤΑΝ: το fixture μπει in-repo (F-072).
    // ΜΕΤΡΙΕΤΑΙ ΑΠΟ: scripts/empty-pass-lint.sh
    if !std::path::Path::new(wav_path).exists() {
        println!("SKIPPED: {} not found", wav_path);
        return;
    }

    let mut reader = WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap() as f32).collect(),
    };
    
    // OFFLINE
    let mut frontend_offline = Phi1MelFrontend::new();
    let offline_frames = frontend_offline.compute_all_power(&samples);
    
    // STREAMING
    let mut fe = Phi2StreamingFrontend::new();
    let mut streaming_frames = Vec::new();
    
    let chunk = 65536;
    let mut i = 0;
    loop {
        let offset = i * chunk;
        if offset >= samples.len() {
            break;
        }
        let end = (offset + chunk).min(samples.len());
        
        streaming_frames.extend(fe.push(&samples[offset..end]));
        
        i += 1;
    }
    streaming_frames.extend(fe.finish());
    
    // COMPARISON
    println!("Offline frames: {}", offline_frames.len());
    println!("Streaming frames: {}", streaming_frames.len());
    
    let mut max_diff = 0.0f32;
    let mut max_frame = 0;
    let mut max_band = 0;
    let mut max_off_val = 0.0;
    let mut max_str_val = 0.0;
    
    let min_frames = offline_frames.len().min(streaming_frames.len());
    for f in 0..min_frames {
        for b in 0..64 {
            let diff = (offline_frames[f][b] - streaming_frames[f][b]).abs();
            if diff > max_diff {
                max_diff = diff;
                max_frame = f;
                max_band = b;
                max_off_val = offline_frames[f][b];
                max_str_val = streaming_frames[f][b];
            }
        }
    }
    
    println!("Max diff: {}", max_diff);
    if max_diff > 0.0 {
        println!("Max diff at frame {}, band {}", max_frame, max_band);
        println!("Offline value: {}", max_off_val);
        println!("Streaming value: {}", max_str_val);
    }
    
    let mut first_bad = None;
    for f in 0..min_frames {
        let d: f32 = (0..64)
            .map(|b| (offline_frames[f][b] - streaming_frames[f][b]).abs())
            .fold(0.0, f32::max);
        if d > 1e-3 && first_bad.is_none() {
            first_bad = Some((f, d));
        }
    }
    println!("first frame with diff>1e-3: {:?}", first_bad);

    for g in 0..(min_frames / 50 + 1) {
        let lo = g * 50;
        let hi = ((g + 1) * 50).min(min_frames);
        if lo >= hi { break; }
        let mut mx = 0.0f32;
        for f in lo..hi {
            for b in 0..64 {
                mx = mx.max((offline_frames[f][b] - streaming_frames[f][b]).abs());
            }
        }
        println!("frames {}-{}: max {}", lo, hi - 1, mx);
    }
    
    assert_eq!(offline_frames.len(), streaming_frames.len(), "frame count mismatch");
    assert!(max_diff < 1e-4, "streaming != offline: max|Δ| = {}", max_diff);
}
