use m0d::handlers::decode::decode_audio;
use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Label {
    Speech,
    NonSpeech,
    Unlabeled,
}

fn prepare(path: &str) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let audio = decode_audio(path).expect("decode failed");
    let mut left = Vec::with_capacity(audio.samples.len() / 2);
    let mut right = Vec::with_capacity(audio.samples.len() / 2);
    let mut mono = Vec::with_capacity(audio.samples.len() / 2);
    for chunk in audio.samples.chunks_exact(2) {
        let l = chunk[0];
        let r = chunk[1];
        left.push(l);
        right.push(r);
        mono.push((l + r) * 0.5);
    }
    (mono, left, right)
}

fn weak_labels(mono: &[f32], sr: u32) -> (Vec<Label>, f32) {
    let win_samples = (sr / 10) as usize; // 100ms
    let mut rms_wins = Vec::new();
    for chunk in mono.chunks(win_samples) {
        rms_wins.push(sp314_dsp::analysis::dynamics::rms_db(chunk));
    }

    let mut floor_db;
    let block_500ms = (sr / 2) as usize;
    if mono.len() >= block_500ms {
        let step = win_samples;
        let mut min_rms = f32::MAX;
        for i in (0..=mono.len() - block_500ms).step_by(step) {
            let chunk = &mono[i..i + block_500ms];
            let r = sp314_dsp::analysis::dynamics::rms_db(chunk);
            if r < min_rms {
                min_rms = r;
            }
        }
        floor_db = min_rms;
    } else {
        floor_db = rms_wins.iter().copied().fold(f32::MAX, f32::min);
    }

    if floor_db > 0.0 || floor_db < -144.0 {
        floor_db = -144.0;
    }

    let mut labels = Vec::with_capacity(rms_wins.len());
    for &r in &rms_wins {
        if r > floor_db + 20.0 {
            labels.push(Label::Speech);
        } else if r < floor_db + 6.0 {
            labels.push(Label::NonSpeech);
        } else {
            labels.push(Label::Unlabeled);
        }
    }

    (labels, floor_db)
}

fn run_vad(
    mono: &[f32],
    left: &[f32],
    right: &[f32],
    floor_db: f32,
) -> Vec<(f32, bool, [f32; 4], f32)> {
    let mut extractor = VadFeatureExtractor::new();
    let mut classifier = VadClassifier::new(FixedPriors);
    let mut results = Vec::new();

    let slab_size = 48000;
    for i in (0..mono.len()).step_by(slab_size) {
        let end = (i + slab_size).min(mono.len());
        let m_slab = &mono[i..end];
        let l_slab = &left[i..end];
        let r_slab = &right[i..end];

        let feats = extractor.process_chunk(m_slab, l_slab, r_slab);
        for f in feats {
            let decision = classifier.process(&f, floor_db);
            let ctx = sp314_dsp::analysis::vad_model::VadContext {
                noise_floor_dbfs: floor_db,
                rms_delta_30ms: decision.rms_delta_30ms,
            };
            let terms = FixedPriors.log_odds_terms(&f, &ctx);
            let snr = f.rms_db - floor_db;
            results.push((decision.posterior, decision.is_speech, terms, snr));
        }
    }
    results
}

fn print_stats(name: &str, class_name: &str, mut posts: Vec<f32>, right_side_is_high: bool) {
    if posts.is_empty() {
        println!("VADVAL|{}|{}|count=0", name, class_name);
        return;
    }
    posts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let count = posts.len();
    let sum: f32 = posts.iter().sum();
    let mean = sum / count as f32;
    let p10 = posts[(count as f32 * 0.1).floor() as usize];
    let p50 = posts[(count as f32 * 0.5).floor() as usize];
    let p90 = posts[(count as f32 * 0.9).floor() as usize];
    let p99 = posts[(count as f32 * 0.99).min((count - 1) as f32).floor() as usize];

    let right_side = if right_side_is_high {
        posts.iter().filter(|&&p| p >= 0.5).count()
    } else {
        posts.iter().filter(|&&p| p < 0.5).count()
    };
    let pct_right = (right_side as f32 / count as f32) * 100.0;

    println!(
        "VADVAL|{}|{}|count={}|mean={:.4}|p10={:.4}|p50={:.4}|p90={:.4}|p99={:.4}|pct_right={:.1}%",
        name, class_name, count, mean, p10, p50, p90, p99, pct_right
    );
}

fn report(
    name: &str,
    labels: Option<&[Label]>,
    frames: &[(f32, bool, [f32; 4], f32)],
    floor_db: f32,
) {
    println!("VADVAL|{}|FLOOR_DB|{:.2}", name, floor_db);

    let mut window_posteriors = Vec::new();
    let mut window_frames = Vec::new();
    for chunk in frames.chunks(10) {
        let avg: f32 = chunk.iter().map(|(p, ..)| p).sum::<f32>() / chunk.len() as f32;
        window_posteriors.push(avg);
        window_frames.push(chunk);
    }

    let mut class_frames = std::collections::HashMap::new();
    let mut add_frames = |class: &str, window_idx: usize| {
        let e = class_frames
            .entry(class.to_string())
            .or_insert_with(Vec::new);
        for f in window_frames[window_idx] {
            e.push(*f);
        }
    };

    let mut nonspeech_windows = Vec::new();

    if let Some(lbls) = labels {
        let mut speech_posts = Vec::new();
        let mut non_speech_posts = Vec::new();
        let mut unlabeled_count = 0;

        let n = lbls.len().min(window_posteriors.len());
        for i in 0..n {
            let p = window_posteriors[i];
            let class = match lbls[i] {
                Label::Speech => {
                    speech_posts.push(p);
                    "SPEECH"
                }
                Label::NonSpeech => {
                    non_speech_posts.push(p);
                    nonspeech_windows.push((i, p));
                    "NONSPEECH"
                }
                Label::Unlabeled => {
                    unlabeled_count += 1;
                    "UNLABELED"
                }
            };
            add_frames(class, i);
        }

        print_stats(name, "SPEECH", speech_posts, true);
        print_stats(name, "NONSPEECH", non_speech_posts, false);
        println!("VADVAL|{}|UNLABELED|count={}", name, unlabeled_count);
    } else {
        print_stats(name, "MUSIC", window_posteriors.clone(), false);
        for i in 0..window_posteriors.len() {
            add_frames("MUSIC", i);
        }
    }

    nonspeech_windows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    for (i, _) in nonspeech_windows.iter().take(20) {
        add_frames("NONSPEECH_TOP20", *i);
    }

    for class in ["SPEECH", "NONSPEECH", "MUSIC", "NONSPEECH_TOP20"] {
        if let Some(fs) = class_frames.get(class) {
            if fs.is_empty() {
                continue;
            }
            let mut sum_snr = 0.0;
            let mut sum_flat = 0.0;
            let mut sum_ms = 0.0;
            let mut sum_drms = 0.0;
            let mut snrs = Vec::new();

            for &(_, _, terms, snr) in fs {
                sum_snr += terms[0];
                sum_flat += terms[1];
                sum_ms += terms[2];
                sum_drms += terms[3];
                snrs.push(snr);
            }

            let count = fs.len() as f32;
            let m_snr = sum_snr / count;
            let m_flat = sum_flat / count;
            let m_ms = sum_ms / count;
            let m_drms = sum_drms / count;
            let m_total = m_snr + m_flat + m_ms + m_drms;

            println!(
                "VADVAL|{}|TERMS|{}|l_snr={:.4}|l_flat={:.4}|l_ms={:.4}|l_drms={:.4}|total={:.4}",
                name, class, m_snr, m_flat, m_ms, m_drms, m_total
            );

            snrs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let c = snrs.len();
            let p10 = snrs[(c as f32 * 0.1).floor() as usize];
            let p50 = snrs[(c as f32 * 0.5).floor() as usize];
            let p90 = snrs[(c as f32 * 0.9).floor() as usize];
            println!(
                "VADVAL|{}|SNR|{}|p10={:.4}|p50={:.4}|p90={:.4}",
                name, class, p10, p50, p90
            );
        }
    }

    let mut hist = [0usize; 10];
    for p in &window_posteriors {
        let mut b = (*p * 10.0) as usize;
        if b == 10 {
            b = 9;
        }
        if b < 10 {
            hist[b] += 1;
        }
    }
    print!("VADVAL|{}|HISTOGRAM|", name);
    for (i, count) in hist.iter().enumerate() {
        print!("0.{}-0.{}:{} ", i, i + 1, count);
    }
    println!();

    let is_speech_count = frames.iter().filter(|(_, is_sp, ..)| *is_sp).count();
    let is_speech_pct = (is_speech_count as f32 / frames.len() as f32) * 100.0;
    println!("VADVAL|{}|IS_SPEECH_FRAMES|pct={:.1}%", name, is_speech_pct);
}

fn ensure_downloaded(url: &str, out_path: &str) -> bool {
    if !Path::new(out_path).exists() {
        match std::process::Command::new("curl")
            .arg("-sL")
            .arg("-o")
            .arg(out_path)
            .arg(url)
            .status()
        {
            Ok(status) if status.success() => true,
            _ => {
                println!("VADVAL|WARNING|failed to download {}", url);
                false
            }
        }
    } else {
        true
    }
}

#[test]
#[ignore]
fn vad_narration_dream() {
    let url = "https://github.com/voxserv/audio_quality_testing_samples/raw/master/mono_44100/156550__acclivity__a-dream-within-a-dream.wav";
    let path = "/tmp/narration_dream.wav";
    if !ensure_downloaded(url, path) {
        return;
    }
    let (mono, left, right) = prepare(path);
    let (labels, floor_db) = weak_labels(&mono, 48000);
    let frames = run_vad(&mono, &left, &right, floor_db);
    assert!(!frames.is_empty(), "must have frames");
    assert!(
        labels.iter().any(|&l| l != Label::Unlabeled),
        "must have labeled windows"
    );
    report("dream", Some(&labels), &frames, floor_db);
}

#[test]
#[ignore]
fn vad_narration_crossing() {
    let url = "https://github.com/voxserv/audio_quality_testing_samples/raw/master/mono_44100/382326__scott-simpson__crossing-the-bar.wav";
    let path = "/tmp/narration_crossing.wav";
    if !ensure_downloaded(url, path) {
        return;
    }
    let (mono, left, right) = prepare(path);
    let (labels, floor_db) = weak_labels(&mono, 48000);
    let frames = run_vad(&mono, &left, &right, floor_db);
    assert!(!frames.is_empty(), "must have frames");
    assert!(
        labels.iter().any(|&l| l != Label::Unlabeled),
        "must have labeled windows"
    );
    report("crossing", Some(&labels), &frames, floor_db);
}

#[test]
fn vad_music_negative() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav");
    let (mono, left, right) = prepare(path.to_str().unwrap());
    let (_, floor_db) = weak_labels(&mono, 48000);
    let frames = run_vad(&mono, &left, &right, floor_db);
    assert!(!frames.is_empty(), "must have frames");
    report("music_negative", None, &frames, floor_db);
}
