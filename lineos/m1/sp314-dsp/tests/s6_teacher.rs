use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use serde_json::Value;

use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{VadClassifier, FixedPriors};

// Basic symphonia helper copied/adapted from fixture_factory.rs
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

fn decode_audio(path: &str) -> Vec<f32> {
    let src = File::open(path).expect("Failed to open file");
    let mss = MediaSourceStream::new(Box::new(src), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("flac");

    let meta_opts: MetadataOptions = Default::default();
    let fmt_opts: FormatOptions = Default::default();
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &fmt_opts, &meta_opts)
        .expect("Failed to probe");

    let mut decoder = symphonia::default::get_codecs()
        .make(&probed.format.default_track().unwrap().codec_params, &DecoderOptions::default())
        .expect("Failed to make decoder");

    let mut out = Vec::new();
    loop {
        match probed.format.next_packet() {
            Ok(packet) => {
                let decoded = decoder.decode(&packet).unwrap();
                let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                sample_buf.copy_interleaved_ref(decoded);
                out.extend_from_slice(sample_buf.samples());
            }
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(_) => break,
        }
    }
    
    // Stereo to Mono by keeping only the left channel, same as before? Or averaging?
    // In our pipeline we take channels[0]. Let's just do `out.iter().step_by(2).copied().collect()` if it's stereo.
    // Assuming FLACs are mono because they were saved as mono. Actually our FlacWriter writes stereo.
    out.chunks_exact(2).map(|x| (x[0] + x[1]) * 0.5).collect()
}

fn load_pmap(path: &str) -> Vec<f32> {
    let mut file = File::open(path).expect("Failed to open pmap");
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    let v: Value = serde_json::from_str(&contents).unwrap();
    let probs = v["probs"].as_array().unwrap();
    probs.iter().map(|p| p.as_f64().unwrap() as f32).collect()
}

#[test]
fn test_s6_teacher() {
    let base_audio = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
    let base_pmaps = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../research/silero-lab/pmaps");

    let mixes = vec!["mix_1", "mix_2", "mix_3"];
    let snrs = vec!["-6", "-15", "-25"];
    
    let mut silero_recalls = Vec::new();
    let mut silero_falsepos = Vec::new();
    let mut house_recalls = Vec::new();
    let mut house_falsepos = Vec::new();
    
    println!("FRAME ALIGNMENT MATH:");
    println!("Silero VAD consumes audio at 16,000 Hz with a 512-sample chunk size.");
    println!("Frame rate = 16000 / 512 = 31.25 fps");
    println!("House VAD consumes audio at 48,000 Hz with a 1536-sample hop size.");
    println!("Frame rate = 48000 / 1536 = 31.25 fps");
    println!("The frames align perfectly 1:1.\n");

    for m in &mixes {
        let voice_id = m.replace("mix", "voice");
        let truth_pmap_path = format!("{}/{}.flac.pmap.json", base_pmaps, voice_id);
        let truth_pmap = load_pmap(&truth_pmap_path);
        let truth_map: Vec<bool> = truth_pmap.iter().map(|&p| p >= 0.5).collect();

        for snr in &snrs {
            let mix_name = format!("{}_snr{}", m, snr);
            let audio_path = format!("{}/{}.flac", base_audio, mix_name);
            let silero_pmap_path = format!("{}/{}.flac.pmap.json", base_pmaps, mix_name);

            // 1. Silero Judge
            let silero_pmap = load_pmap(&silero_pmap_path);
            let silero_map: Vec<bool> = silero_pmap.iter().map(|&p| p >= 0.5).collect();

            // 2. House Judge
            let audio = decode_audio(&audio_path);
            let mut ext = VadFeatureExtractor::new();
            let feats = ext.process_chunk(&audio, &audio, &audio);
            let mut vad = VadClassifier::new(FixedPriors);
            let mut house_map = Vec::new();
            for f in &feats {
                house_map.push(vad.process(f, -144.0).is_speech);
            }

            // Align lengths
            let min_len = truth_map.len().min(silero_map.len()).min(house_map.len());
            
            // Calculate Silero metrics
            let mut s_true_pos = 0;
            let mut s_false_pos = 0;
            let mut truth_pos = 0;
            let mut truth_neg = 0;
            
            let mut h_true_pos = 0;
            let mut h_false_pos = 0;

            for i in 0..min_len {
                let t = truth_map[i];
                let s = silero_map[i];
                let h = house_map[i];

                if t {
                    truth_pos += 1;
                    if s { s_true_pos += 1; }
                    if h { h_true_pos += 1; }
                } else {
                    truth_neg += 1;
                    if s { s_false_pos += 1; }
                    if h { h_false_pos += 1; }
                }
            }

            let s_recall = 100.0 * s_true_pos as f32 / truth_pos.max(1) as f32;
            let s_fp = 100.0 * s_false_pos as f32 / truth_neg.max(1) as f32;
            let h_recall = 100.0 * h_true_pos as f32 / truth_pos.max(1) as f32;
            let h_fp = 100.0 * h_false_pos as f32 / truth_neg.max(1) as f32;

            silero_recalls.push(s_recall);
            silero_falsepos.push(s_fp);
            house_recalls.push(h_recall);
            house_falsepos.push(h_fp);

            println!("S6CMP|mix={}|judge=silero|recall={:.2}|falsepos={:.2}", mix_name, s_recall, s_fp);
            println!("S6CMP|mix={}|judge=house|recall={:.2}|falsepos={:.2}", mix_name, h_recall, h_fp);
        }
    }
    
    // Summary row
    let s_mean_recall = silero_recalls.iter().sum::<f32>() / 9.0;
    let s_mean_fp = silero_falsepos.iter().sum::<f32>() / 9.0;
    let h_mean_recall = house_recalls.iter().sum::<f32>() / 9.0;
    let h_mean_fp = house_falsepos.iter().sum::<f32>() / 9.0;
    
    println!("\nSUMMARY:");
    println!("S6CMP|mix=MEAN|judge=silero|recall={:.2}|falsepos={:.2}", s_mean_recall, s_mean_fp);
    println!("S6CMP|mix=MEAN|judge=house|recall={:.2}|falsepos={:.2}", h_mean_recall, h_mean_fp);
    
    println!("\nHARD ROW (mix_2_snr-6):");
    println!("S6CMP|mix=mix_2_snr-6|judge=silero|recall={:.2}|falsepos={:.2}", silero_recalls[3], silero_falsepos[3]);
    println!("S6CMP|mix=mix_2_snr-6|judge=house|recall={:.2}|falsepos={:.2}", house_recalls[3], house_falsepos[3]);
}
