use hound::WavReader;
use sp314_dsp::analysis::phi1_sensor::Phi1MelFrontend;

#[test]
fn test_phi2_decimate_probe() {
    let wav_path = "/tmp/phi1/eq_test_48k.wav";
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
    
    let frontend = Phi1MelFrontend::new();
    
    // OFFLINE
    let dec_off = frontend.decimate_only(&samples);
    
    // STREAMING
    let mut dec_str = Vec::new();
    let chunk = 65536;
    let history = 10240;
    
    let mut i = 0;
    loop {
        let offset = i * chunk;
        if offset >= samples.len() {
            break;
        }
        let end = (offset + chunk).min(samples.len());
        let start = offset.saturating_sub(history);
        
        let slice = &samples[start..end];
        let dec_i = frontend.decimate_only(slice);
        
        let drop_tail = if end == samples.len() { 0 } else { 20 };
        let back = if i == 0 { 0 } else { 20 };
        let skip_n = ((offset - start) / 3).saturating_sub(back);
        let take_n = dec_i.len().saturating_sub(skip_n + drop_tail);
        dec_str.extend_from_slice(&dec_i[skip_n..skip_n + take_n]);
        
        i += 1;
    }
    
    println!("Offline decimated length: {}", dec_off.len());
    println!("Streaming decimated length: {}", dec_str.len());
    
    let min_len = dec_off.len().min(dec_str.len());
    let mut max_diff = 0.0f32;
    let mut first_bad = None;
    
    for i in 0..min_len {
        let diff = (dec_off[i] - dec_str[i]).abs();
        max_diff = max_diff.max(diff);
        if diff > 1e-4 && first_bad.is_none() {
            first_bad = Some(i);
        }
    }
    
    println!("Max diff: {}", max_diff);
    
    if let Some(fb) = first_bad {
        println!("First diff > 1e-4 at index: {}", fb);
        let start_idx = fb.saturating_sub(2);
        let end_idx = (fb + 3).min(min_len);
        
        for idx in start_idx..end_idx {
            let marker = if idx == fb { "==>" } else { "   " };
            println!(
                "{} [{:05}] offline: {:<12} streaming: {:<12} diff: {}",
                marker,
                idx,
                dec_off[idx],
                dec_str[idx],
                (dec_off[idx] - dec_str[idx]).abs()
            );
        }
    }
}
