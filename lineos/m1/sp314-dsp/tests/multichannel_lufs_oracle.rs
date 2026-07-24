use hound::WavReader;
use sp314_dsp::metering::LufsMeter;
use sp314_dsp::metering::MultichannelLufsMeter;

fn read_6ch_wav(path: &str) -> Vec<[f32; 6]> {
    let mut reader = WavReader::open(path).expect("failed to open wav");
    assert_eq!(reader.spec().channels, 6, "expected 6-channel wav");
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap()).collect();
    let mut frames = Vec::with_capacity(samples.len() / 6);
    for chunk in samples.chunks_exact(6) {
        let frame: [f32; 6] = core::array::from_fn(|i| chunk[i] as f32 / 32768.0);
        frames.push(frame);
    }
    frames
}

/// Skip-guard for gitignored fixtures: returns true if the fixture
/// exists, prints a regeneration hint and returns false otherwise.
fn fixture_exists(path: &str) -> bool {
    if std::path::Path::new(path).exists() {
        return true;
    }
    eprintln!(
        "SKIP: {path} missing — regenerate via \
         tests/fixtures/README.md ffmpeg commands (seed=42)"
    );
    false
}

/// (A) FFMPEG GROUND TRUTH: loud fixture == -18.0 ±0.2, quiet == -34.6 ±0.2.
#[test]
fn ground_truth_loud() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../m0/m0-daemon/tests/fixtures/lufs_5dot1_ref_loud.wav"
    );
    if !fixture_exists(path) {
        return;
    }
    let frames = read_6ch_wav(path);
    let mut meter = MultichannelLufsMeter::new();
    for frame in &frames {
        meter.process_frame(frame);
    }
    let lufs = meter.finish().unwrap();
    let delta = (lufs - (-18.0)).abs();
    println!("ground_truth_loud: measured={lufs:.4}, target=-18.0, delta={delta:.4}");
    assert!(
        delta <= 0.2,
        "Loud fixture: expected -18.0 ±0.2, got {lufs:.2} (delta {delta:.3})"
    );
}

#[test]
fn ground_truth_quiet() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../m0/m0-daemon/tests/fixtures/lufs_5dot1_ref.wav"
    );
    if !fixture_exists(path) {
        return;
    }
    let frames = read_6ch_wav(path);
    let mut meter = MultichannelLufsMeter::new();
    for frame in &frames {
        meter.process_frame(frame);
    }
    let lufs = meter.finish().unwrap();
    let delta = (lufs - (-34.6)).abs();
    println!("ground_truth_quiet: measured={lufs:.4}, target=-34.6, delta={delta:.4}");
    assert!(
        delta <= 0.2,
        "Quiet fixture: expected -34.6 ±0.2, got {lufs:.2} (delta {delta:.3})"
    );
}

/// (B) SURROUND-WEIGHT PROOF: Ls-only == -33.1 ±0.2, L-only == -34.6 ±0.2.
/// The ~1.5 LU delta proves surround weighting matches ffmpeg BS.1770-4.
#[test]
fn surround_weight_ls_only() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../m0/m0-daemon/tests/fixtures/lufs_ls_only.wav"
    );
    if !fixture_exists(path) {
        return;
    }
    let frames = read_6ch_wav(path);
    let mut meter = MultichannelLufsMeter::new();
    for frame in &frames {
        meter.process_frame(frame);
    }
    let lufs = meter.finish().unwrap();
    let delta = (lufs - (-33.1)).abs();
    println!("surround_weight_ls_only: measured={lufs:.4}, target=-33.1, delta={delta:.4}");
    assert!(
        delta <= 0.2,
        "Ls-only: expected -33.1 ±0.2, got {lufs:.2} (delta {delta:.3})"
    );
}

#[test]
fn surround_weight_l_only() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../m0/m0-daemon/tests/fixtures/lufs_l_only.wav"
    );
    if !fixture_exists(path) {
        return;
    }
    let frames = read_6ch_wav(path);
    let mut meter = MultichannelLufsMeter::new();
    for frame in &frames {
        meter.process_frame(frame);
    }
    let lufs = meter.finish().unwrap();
    let delta = (lufs - (-34.6)).abs();
    println!("surround_weight_l_only: measured={lufs:.4}, target=-34.6, delta={delta:.4}");
    assert!(
        delta <= 0.2,
        "L-only: expected -34.6 ±0.2, got {lufs:.2} (delta {delta:.3})"
    );
}

/// (C) LFE EXCLUSION: energy only on ch 3 (LFE), silence elsewhere.
/// Weight = 0.0 → meter must report silence (-144 or None).
/// No fixture needed — signal generated in-code.
#[test]
fn lfe_exclusion() {
    let sr = 48000_usize;
    let n = sr * 2; // 2 seconds
    let mut meter = MultichannelLufsMeter::new();
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 100.0 * t).sin();
        let mut frame = [0.0_f32; 6];
        frame[3] = s;
        meter.process_frame(&frame);
    }
    let lufs = meter.finish().unwrap_or(-144.0);
    println!("lfe_exclusion: measured={lufs:.4} (must be ≤ -144.0)");
    assert!(
        lufs <= -144.0,
        "LFE should be excluded (≤ -144.0), got {lufs:.2}"
    );
}

/// (D) STEREO EQUIVALENCE: L/R-only signal through both meters must agree ±0.1.
/// With only L/R active (weight 1.0) and C/LFE/Ls/Rs silent, the multichannel
/// meter reduces to the stereo case.
/// No fixture needed — signal generated in-code.
#[test]
fn stereo_equivalence() {
    let sr = 48000_usize;
    let n = sr * 2;
    let mut mc_meter = MultichannelLufsMeter::new();
    let mut st_meter = LufsMeter::new();
    let mut l_buf = Vec::with_capacity(n);
    let mut r_buf = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
        let mut frame = [0.0_f32; 6];
        frame[0] = s;
        frame[1] = s;
        mc_meter.process_frame(&frame);
        l_buf.push(s);
        r_buf.push(s);
    }
    st_meter.process_chunk(&l_buf, &r_buf);
    let mc_lufs = mc_meter.finish().unwrap();
    let st_lufs = st_meter.finish().unwrap();
    let delta = (mc_lufs - st_lufs).abs();
    println!(
        "stereo_equivalence: multichannel={mc_lufs:.4}, stereo={st_lufs:.4}, delta={delta:.6}"
    );
    assert!(
        delta <= 0.1,
        "Stereo equivalence: multichannel {mc_lufs:.2}, stereo {st_lufs:.2}, delta {delta:.4}"
    );
}
