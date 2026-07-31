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

fn run_vad(mono: &[f32], left: &[f32], right: &[f32], floor_db: f32) -> Vec<(f32, bool)> {
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
            results.push((decision.posterior, decision.is_speech));
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

fn report(name: &str, labels: Option<&[Label]>, frames: &[(f32, bool)]) {
    let mut window_posteriors = Vec::new();
    for chunk in frames.chunks(10) {
        let avg: f32 = chunk.iter().map(|(p, _)| p).sum::<f32>() / chunk.len() as f32;
        window_posteriors.push(avg);
    }

    if let Some(lbls) = labels {
        let mut speech_posts = Vec::new();
        let mut non_speech_posts = Vec::new();
        let mut unlabeled_count = 0;

        let n = lbls.len().min(window_posteriors.len());
        for i in 0..n {
            let p = window_posteriors[i];
            match lbls[i] {
                Label::Speech => speech_posts.push(p),
                Label::NonSpeech => non_speech_posts.push(p),
                Label::Unlabeled => unlabeled_count += 1,
            }
        }

        print_stats(name, "SPEECH", speech_posts, true);
        print_stats(name, "NONSPEECH", non_speech_posts, false);
        println!("VADVAL|{}|UNLABELED|count={}", name, unlabeled_count);
    } else {
        print_stats(name, "MUSIC", window_posteriors.clone(), false);
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

    let is_speech_count = frames.iter().filter(|(_, is_sp)| *is_sp).count();
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
    report("dream", Some(&labels), &frames);
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
    report("crossing", Some(&labels), &frames);
}

#[test]
fn vad_music_negative() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav");
    let (mono, left, right) = prepare(path.to_str().unwrap());
    let (_, floor_db) = weak_labels(&mono, 48000);
    let frames = run_vad(&mono, &left, &right, floor_db);
    assert!(!frames.is_empty(), "must have frames");
    report("music_negative", None, &frames);
}
