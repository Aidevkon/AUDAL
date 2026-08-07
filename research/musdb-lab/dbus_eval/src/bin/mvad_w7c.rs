use hound;
use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{VadClassifier, FixedPriors};
use sp314_dsp::analysis::scout::SegmentScout;
use lineos_corpus::scout::{compute_scout_decision, smooth_and_segment, TimelineRouter, SegmentType, ScoutMeasurements};
use lineos_corpus::mfcc::MfccAnalyzer;
use std::env;

fn load_wav(path: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).expect("Failed to open wav file");
    let channels = reader.spec().channels;
    let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
    
    let mut mono = Vec::new();
    let mut left = Vec::new();
    let mut right = Vec::new();
    
    if channels == 2 {
        for chunk in samples.chunks_exact(2) {
            left.push(chunk[0]);
            right.push(chunk[1]);
            mono.push((chunk[0] + chunk[1]) * 0.5);
        }
    } else {
        for s in samples {
            left.push(s);
            right.push(s);
            mono.push(s);
        }
    }
    
    (mono, left, right)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <mix.wav> [window_start_s window_end_s]", args[0]);
        std::process::exit(1);
    }
    
    let mut args_clean = Vec::new();
    let mut dump_path: Option<String> = None;
    for a in &args {
        if a.starts_with("--dump-seg=") {
            dump_path = Some(a["--dump-seg=".len()..].to_string());
        } else {
            args_clean.push(a.clone());
        }
    }
    
    let mix_path = &args_clean[1];
    let mut has_window = false;
    let mut start_idx = 0;
    let mut end_idx = 0;
    
    if args_clean.len() >= 4 {
        has_window = true;
        let start_s: f32 = args_clean[2].parse().expect("Invalid start window");
        let end_s: f32 = args_clean[3].parse().expect("Invalid end window");
        start_idx = (start_s * 100.0).floor() as usize;
        end_idx = (end_s * 100.0).floor() as usize;
    }
    
    let (mix_mono, mix_left, mix_right) = load_wav(mix_path);
    
    // 2. MACRO PASS
    let win_samples = (5.0 * 48000.0) as usize;
    let hop_samples = (1.0 * 48000.0) as usize;
    let mut scout = SegmentScout::new();
    let mut start = 0;
    let mut decisions = Vec::new();

    while start + win_samples <= mix_mono.len() {
        let mono_slice = &mix_mono[start..start + win_samples];
        
        let mut mfcc_analyzer = MfccAnalyzer::new();
        let mut mfccs = Vec::new();
        let mut f = 0;
        while f + 1024 <= mono_slice.len() {
            mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
            f += 512;
        }
        let cepstral_flux = lineos_corpus::scout::compute_cepstral_flux(&mfccs);
        
        let meas = scout.measure(mono_slice, cepstral_flux, 48000);
        let scout_meas = ScoutMeasurements {
            cv_ioi: meas.cv_ioi,
            cepstral_flux: meas.cepstral_flux,
        };
        let decision = compute_scout_decision(&scout_meas);
        
        let t_start_sec = start as f32 / 48000.0;
        decisions.push((t_start_sec, decision, scout_meas));
        
        start += hop_samples;
    }
    
    let mut smooth_input = Vec::new();
    for (t, d, _) in &decisions {
        smooth_input.push((*t, *d));
    }
    
    let boundaries = smooth_and_segment(&smooth_input);
    let segment_map = TimelineRouter::new(boundaries.clone());
    
    if let Some(dp) = dump_path {
        use std::io::Write;
        let mut f_out = std::fs::File::create(dp).expect("Failed to create dump file");
        writeln!(f_out, "win_idx,t_start,cv_ioi,cepstral_flux,leaning,confidence").unwrap();
        for (i, (t, d, m)) in decisions.iter().enumerate() {
            writeln!(f_out, "{},{:.3},{:.5},{:.5},{:.5},{:.5}", i, t, m.cv_ioi, m.cepstral_flux, d.leaning_score, d.confidence).unwrap();
        }
        for b in &boundaries {
            let typ_str = if b.segment_type == SegmentType::Speech { "Speech" } else { "Music" };
            writeln!(f_out, "SEG,{:.3},{:.3},{},{:.5},{:.5}", b.start_sec, b.end_sec, typ_str, b.avg_leaning, b.avg_confidence).unwrap();
        }
    }

    // 3. MICRO PASS
    let mut acx = AcxCheckAnalyzer::new(48000);
    for chunk in mix_mono.chunks(1777) {
        acx.feed_chunk(chunk);
    }
    let nf_mix = acx.finish().noise_floor_db.unwrap_or(-70.0);
    
    let mut extractor = VadFeatureExtractor::new();
    let features = extractor.process_chunk(&mix_mono, &mix_left, &mix_right);
    let mut classifier = VadClassifier::new(FixedPriors);
    
    let mut total_frames_all = 0;
    let mut armed_frames_all = 0;
    let mut micro_speech_frames_all = 0;
    let mut effective_speech_frames_all = 0;
    
    let mut armed_g_frames_all = 0;
    let mut effective_g_frames_all = 0;
    
    let mut speech_frames_in = 0;
    let mut speech_frames_in_g = 0;
    let mut total_frames_in = 0;
    
    let mut speech_frames_out = 0;
    let mut speech_frames_out_g = 0;
    let mut total_frames_out = 0;
    
    for (i, f) in features.iter().enumerate() {
        let t = i as f32 / 100.0;
        let d = classifier.process(f, nf_mix);
        
        // 4. ΣΥΝΔΥΑΣΜΟΣ
        let mut armed = false;
        let mut armed_g = false;
        if let Some((idx, stype)) = segment_map.get_segment_at(t) {
            if stype == SegmentType::Speech {
                armed = true;
                if boundaries[idx].avg_confidence >= 0.4 {
                    armed_g = true;
                }
            }
        }
        
        let fired = armed && d.is_speech;
        let fired_g = armed_g && d.is_speech;
        
        total_frames_all += 1;
        if armed { armed_frames_all += 1; }
        if armed_g { armed_g_frames_all += 1; }
        if d.is_speech { micro_speech_frames_all += 1; }
        if fired { effective_speech_frames_all += 1; }
        if fired_g { effective_g_frames_all += 1; }
        
        if has_window {
            if i >= start_idx && i <= end_idx {
                total_frames_in += 1;
                if fired { speech_frames_in += 1; }
                if fired_g { speech_frames_in_g += 1; }
            } else {
                total_frames_out += 1;
                if fired { speech_frames_out += 1; }
                if fired_g { speech_frames_out_g += 1; }
            }
        }
    }
    
    let num_segments = boundaries.len();
    let num_speech_segs = boundaries.iter().filter(|b| b.segment_type == SegmentType::Speech).count();
    
    let armed_frame_ratio = if total_frames_all == 0 { 0.0 } else { armed_frames_all as f64 / total_frames_all as f64 };
    let micro_ratio = if total_frames_all == 0 { 0.0 } else { micro_speech_frames_all as f64 / total_frames_all as f64 };
    let effective_ratio = if total_frames_all == 0 { 0.0 } else { effective_speech_frames_all as f64 / total_frames_all as f64 };
    
    let armed_g_ratio = if total_frames_all == 0 { 0.0 } else { armed_g_frames_all as f64 / total_frames_all as f64 };
    let effective_g_ratio = if total_frames_all == 0 { 0.0 } else { effective_g_frames_all as f64 / total_frames_all as f64 };
    
    let mut ratio_in = -1.0;
    let mut ratio_out = -1.0;
    let mut in_g = -1.0;
    let mut out_g = -1.0;
    if has_window {
        ratio_in = if total_frames_in == 0 { 0.0 } else { speech_frames_in as f64 / total_frames_in as f64 };
        ratio_out = if total_frames_out == 0 { 0.0 } else { speech_frames_out as f64 / total_frames_out as f64 };
        in_g = if total_frames_in == 0 { 0.0 } else { speech_frames_in_g as f64 / total_frames_in as f64 };
        out_g = if total_frames_out == 0 { 0.0 } else { speech_frames_out_g as f64 / total_frames_out as f64 };
    }
    
    println!("W7C|mix={}|segments={}|armed_seg={}|armed_frame_ratio={:.3}|micro_ratio={:.3}|effective_ratio={:.3}|ratio_in={:.3}|ratio_out={:.3}|nf={:.1}|armed_g_ratio={:.3}|effective_g_ratio={:.3}|in_g={:.3}|out_g={:.3}",
        mix_path, num_segments, num_speech_segs, armed_frame_ratio, micro_ratio, effective_ratio, ratio_in, ratio_out, nf_mix, armed_g_ratio, effective_g_ratio, in_g, out_g);
}
