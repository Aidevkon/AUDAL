//! Validation harness for AcxCheckAnalyzer against real narration files.
//!
//! Not a CI test — real files live outside the repo, so this is #[ignore]d
//! and run by hand:
//!
//!   ACX_WAV=/tmp/narration_dream.wav cargo test -p sp314-dsp \
//!     --test acx_check_real_file -- --ignored --nocapture
//!
//! Prints the three ACX numbers for side-by-side comparison with
//! (a) ffmpeg astats (RMS/peak cross-check) and
//! (b) Audacity's ACX Check plugin (the noise-floor reference).
//! The WAV is read verbatim — no resampling, no channel conversion —
//! because the external tools must see the identical samples.
//!
//! Validated 2026-07-30 on two real narration files (Freesound/voxserv,
//! public domain, mono 16-bit 44.1 kHz), against ffmpeg astats (RMS and
//! peak: match to 0.01 dB) and an independent Python/scipy oracle for the
//! noise floor (tests/tools/acx_nf_oracle.py — match to 0.02 dB, which is
//! the arithmetic difference between our RBJ cascade and scipy's butter
//! design, not an algorithmic one):
//!
//!   narration_dream.wav     peak -6.80  rms -30.01  floor -74.32 dBFS
//!   narration_crossing.wav  peak -6.03  rms -28.40  floor -94.58 dBFS
//!
//! The files are deliberately NOT repo fixtures (11 MB of binary for an
//! ignored test). Re-fetch them with:
//!   curl -sL -o /tmp/narration_dream.wav "https://github.com/voxserv/audio_quality_testing_samples/raw/master/mono_44100/156550__acclivity__a-dream-within-a-dream.wav"
//!   curl -sL -o /tmp/narration_crossing.wav "https://github.com/voxserv/audio_quality_testing_samples/raw/master/mono_44100/382326__scott-simpson__crossing-the-bar.wav"

use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;

#[test]
#[ignore = "needs ACX_WAV env pointing at a local file"]
fn acx_check_real_file() {
    let path = std::env::var("ACX_WAV").expect("set ACX_WAV=/path/to/file.wav");
    let mut reader = hound::WavReader::open(&path).expect("open wav");
    let spec = reader.spec();
    println!("\nfile: {path}");
    println!(
        "format: {} Hz, {} ch, {} bit, {:?}",
        spec.sample_rate, spec.channels, spec.bits_per_sample, spec.sample_format
    );

    let mut analyzer = AcxCheckAnalyzer::new(spec.sample_rate);
    let mut n: u64 = 0;

    // Mono downmix if stereo ((l+r)/2 — same convention as the trunk);
    // 16/24-bit int and 32-bit float all normalised to f32 in [-1, 1].
    match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut buf = Vec::with_capacity(8192);
            let mut frame: Vec<f32> = Vec::with_capacity(spec.channels as usize);
            for s in reader.samples::<f32>() {
                frame.push(s.expect("sample"));
                if frame.len() == spec.channels as usize {
                    buf.push(frame.iter().sum::<f32>() / frame.len() as f32);
                    frame.clear();
                    if buf.len() == 8192 {
                        analyzer.feed_chunk(&buf);
                        n += buf.len() as u64;
                        buf.clear();
                    }
                }
            }
            analyzer.feed_chunk(&buf);
            n += buf.len() as u64;
        }
        hound::SampleFormat::Int => {
            let scale = 1.0f32 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            let mut buf = Vec::with_capacity(8192);
            let mut frame: Vec<f32> = Vec::with_capacity(spec.channels as usize);
            for s in reader.samples::<i32>() {
                frame.push(s.expect("sample") as f32 * scale);
                if frame.len() == spec.channels as usize {
                    buf.push(frame.iter().sum::<f32>() / frame.len() as f32);
                    frame.clear();
                    if buf.len() == 8192 {
                        analyzer.feed_chunk(&buf);
                        n += buf.len() as u64;
                        buf.clear();
                    }
                }
            }
            analyzer.feed_chunk(&buf);
            n += buf.len() as u64;
        }
    }

    let secs = n as f64 / spec.sample_rate as f64;
    let report = analyzer.finish();
    println!("samples fed: {n} ({secs:.1} s)");
    println!("\n=== AcxCheckAnalyzer ===");
    println!("sample peak : {:>8.2} dBFS   (ACX limit: <= -3.0)", report.sample_peak_db);
    println!("RMS         : {:>8.2} dBFS   (ACX window: -23.0 .. -18.0)", report.rms_db);
    match report.noise_floor_db {
        Some(nf) => println!("noise floor : {:>8.2} dBFS   (ACX limit: <= -60.0)", nf),
        None => println!("noise floor :     None   (file under 1 s)"),
    }
    println!("passes_acx  : {}", report.passes_acx());
    println!("\ncompare RMS/peak against:  ffmpeg -i {path} -af astats=metadata=1:reset=0 -f null -");
    println!("compare noise floor against: Audacity -> Analyze -> ACX Check");
}
