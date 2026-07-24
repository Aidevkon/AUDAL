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

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        Instant::now(),
        make_head(),
        None,
        None,
        "qa-abort".to_string(),
        state_tmp.path().to_str().unwrap(),
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

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        Instant::now(),
        make_head(),
        None,
        None,
        "qa-healthy".to_string(),
        state_tmp.path().to_str().unwrap(),
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

    // NOTE: proves the tier1/scout path passes for a REAL spoken-word MP3
    // (ishaiaTEST.mp3) via symphonia, not a synthetic WAV.
    //
    // DORMANT EDGE CASE (verified 2026-07-03, not a live bug):
    // read_scout_sample early-returns None if total_frames_hint() is None,
    // which would abort run_dsp. But NO seekable disk file triggers this:
    // symphonia derives n_frames from filesize even for CBR MP3 with no Xing
    // header (probed: `ffmpeg -write_xing 0` still yielded Some). hint=None
    // arises only for non-seekable sources (network/pipe) or corrupt
    // containers. m0-daemon only accepts a disk path (audio_path), so this
    // does not fire in current use. It becomes relevant only if streaming/
    // non-seekable input is added; fix then (designed, not implemented):
    // try_scout_from(sr*30, scout).or_else(from 0) — deterministic, avoids
    // intro, EOF-guarded. See scout-stationarity-assumption.md.
    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &make_req(path, "podcast"), // Triggers streaming path
        std::time::Instant::now(),
        make_head(),
        None,
        None,
        "qa-real-mp3".to_string(),
        state_tmp.path().to_str().unwrap(),
    );

    assert!(
        result.is_ok(),
        "run_dsp failed for REAL healthy MP3 podcast: {:?}",
        result.err()
    );
}

fn push_tone(out: &mut Vec<f32>, dur: f32, sr: u32) {
    let n = (sr as f32 * dur) as usize;
    for _ in 0..n {
        out.push(0.5);
        out.push(0.5);
    }
}

fn push_silence(out: &mut Vec<f32>, dur: f32, sr: u32) {
    let n = (sr as f32 * dur) as usize;
    for _ in 0..n {
        out.push(0.0);
        out.push(0.0);
    }
}

#[test]
fn run_dsp_counts_dead_air_gaps() {
    let sr = 48000u32;
    let mut samples = Vec::new();

    // 10s tone -> 4s gap -> 10s tone -> 4s gap -> 10s tone -> 2s silence (NOT gap) -> 10s tone
    push_tone(&mut samples, 10.0, sr);
    push_silence(&mut samples, 4.0, sr);
    push_tone(&mut samples, 10.0, sr);
    push_silence(&mut samples, 4.0, sr);
    push_tone(&mut samples, 10.0, sr);
    push_silence(&mut samples, 2.0, sr);
    push_tone(&mut samples, 10.0, sr);

    let path = "/tmp/qa_dead_air_gaps.wav";
    write_wav(&samples, sr, path);

    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &make_req(path, "podcast"),
        Instant::now(),
        make_head(),
        None,
        None,
        "qa-dead-air".to_string(),
        state_tmp.path().to_str().unwrap(),
    );

    assert!(
        result.is_ok(),
        "run_dsp failed for dead-air test: {:?}",
        result.err()
    );
    let (blob, _, _, _, _) = result.unwrap();

    let count = blob.dead_air.total_count;
    let sec = blob.dead_air.total_sec;
    let longest = blob.dead_air.longest_sec;

    println!("DEAD_AIR_COUNT: {}", count);
    println!("DEAD_AIR_SEC: {:.2}", sec);
    println!("DEAD_AIR_LONGEST: {:.2}", longest);

    assert_eq!(count, 2, "Expected exactly 2 dead air events");
    assert!(
        (7.0..=9.0).contains(&sec),
        "Expected total_sec between 7.0 and 9.0, got {}",
        sec
    );
}
