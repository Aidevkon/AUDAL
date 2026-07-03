use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

fn write_wav(samples: &[f32], sr: u32, path: &str) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

fn generate_podcast_fixture(sr: u32, dur: f32) -> Vec<f32> {
    // Podcast: mono voice (speech range 300Hz-3kHz), minimal dynamics
    let n = (sr as f32 * dur) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let voice = (2.0 * std::f32::consts::PI * 600.0 * t).sin() * 0.3
            + (2.0 * std::f32::consts::PI * 1200.0 * t).sin() * 0.15
            + (2.0 * std::f32::consts::PI * 2400.0 * t).sin() * 0.1;
        let mix = voice.clamp(-1.0, 1.0);
        // Mono content — same L and R
        out.push(mix);
        out.push(mix);
    }
    out
}

fn make_req(path: &str, preset: &str) -> MasterRequest {
    MasterRequest {
        audio_path: path.to_string(),
        preset_id: preset.to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: Some(0.5),
        chaos_seed: None,
        project_id: None,
        track_id: None,
        mix_levels: None,
        preview_id: None,
    }
}

fn make_head() -> Arc<ArcSwap<DspState>> {
    Arc::new(ArcSwap::from_pointee(DspState::default()))
}

#[test]
fn test_run_dsp_aborts_on_digital_silence() {
    let sr = 48000u32;
    // 40 seconds of absolute silence (0.0) -> should exceed 30s tier1 limit
    let silent_samples = vec![0.0f32; (sr * 40 * 2) as usize];
    let path = "/tmp/qa_silence_abort.wav";
    write_wav(&silent_samples, sr, path);

    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        Instant::now(),
        make_head(),
        None,
        None,
        "qa-abort".to_string(),
    );

    // Assert it fails due to digital silence
    assert!(
        result.is_err(),
        "Expected run_dsp to fail on digital silence, but it returned Ok"
    );
    let err = result.unwrap_err();
    println!("ACTUAL ABORT ERROR: {}", err);
    assert!(
        err.contains("Input validation failed") || err.contains("digital silence"),
        "Expected digital silence abort, got error: {}",
        err
    );
}

#[test]
fn test_run_dsp_passes_healthy_podcast() {
    let sr = 48000u32;
    // 40 seconds of healthy audio
    let healthy = generate_podcast_fixture(sr, 40.0);
    let path = "/tmp/qa_healthy_podcast.wav";
    write_wav(&healthy, sr, path);

    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        Instant::now(),
        make_head(),
        None,
        None,
        "qa-healthy".to_string(),
    );

    assert!(
        result.is_ok(),
        "run_dsp failed for healthy podcast: {:?}",
        result.err()
    );
}

#[test]
fn test_run_dsp_passes_real_mp3_podcast() {
    let path = "/home/aidevcon/Music/ishaiaTEST.mp3";

    // Safety check: if the file isn't present on the machine running the tests, skip gracefully
    if !std::path::Path::new(path).exists() {
        println!("SKIP: Real MP3 fixture not found at {}", path);
        return;
    }

    // KNOWN GAP (HONESTY NOTE):
    // This test proves that the tier1/scout fail-fast check passes for a REAL spoken-word
    // MP3 (ishaiaTEST.mp3) decoded via symphonia, rather than a synthetic WAV.
    // HOWEVER, it only passes because this specific MP3 yields a `Some(frames)` from
    // `total_frames_hint()`.
    // BUG: If an MP3 lacks frame metadata (e.g., pure CBR without Xing, live stream),
    // `read_scout_sample` returns `None`, causing an immediate hard Error in run_dsp
    // ("could not read audio"). This bypassing of tier1/fallback is a known bug that
    // MUST be fixed separately, as Wave 3 streaming (OLA) applies only to the render pass
    // and will not fix the 30s bounded scout read.
    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        std::time::Instant::now(),
        make_head(),
        None,
        None,
        "qa-real-mp3".to_string(),
    );

    assert!(
        result.is_ok(),
        "run_dsp failed for REAL healthy MP3 podcast: {:?}",
        result.err()
    );
}
