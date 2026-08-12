//! A/B render fixture — renders bodleasons_mid.wav through the current
//! TwoPassEngine pipeline and writes the summed stereo output to a WAV file.
//!
//! This test is #[ignore] — it's invoked explicitly by render_variants.sh.
//! The shell script runs it twice: once with the spectral path (current code)
//! and once with the scalar path (temporarily reverted code).

use sp314_dsp::stft::two_pass::{FiveStemsChunk, TwoPassEngine};

const SR: u32 = 48_000;

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bodleasons_mid.wav")
}

fn decode_wav(path: &std::path::Path) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.channels, 2, "Expected stereo WAV");
    assert_eq!(spec.sample_rate, SR, "Expected 48kHz");

    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader
            .samples::<i32>()
            .map(|s| {
                let v = s.unwrap();
                v as f32 / (1 << (spec.bits_per_sample - 1)) as f32
            })
            .collect()
    };

    let mut left = Vec::with_capacity(samples.len() / 2);
    let mut right = Vec::with_capacity(samples.len() / 2);
    for chunk in samples.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
    }
    (left, right)
}

#[test]
#[ignore]
fn render_ab_variant() {
    let path = fixture_path();
    assert!(path.exists(), "Missing fixture: {}", path.display());

    let (left, right) = decode_wav(&path);
    let mono: Vec<f32> = left
        .iter()
        .zip(right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono, &mono, SR, None, None, false);

    let mut output_left = Vec::with_capacity(mono.len());
    let mut output_right = Vec::with_capacity(mono.len());

    let callback = |stems: &FiveStemsChunk| {
        for i in 0..stems.voice.len() {
            let l = stems.voice[i]
                + stems.drums[i]
                + stems.bass[i]
                + stems.harmonics[i]
                + stems.ambience[i];
            output_left.push(l);
            // Simple stereo: duplicate mono sum (stems are mono)
            output_right.push(l);
        }
    };

    engine
        .process_slices_with_params(&mono, &left, &right, &scout, 1.0, false, callback)
        .unwrap();

    // Write WAV
    let out_path = "/tmp/ab_variant_B_spectral.wav";
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(out_path, spec).unwrap();
    for i in 0..output_left.len() {
        writer.write_sample(output_left[i]).unwrap();
        writer.write_sample(output_right[i]).unwrap();
    }
    writer.finalize().unwrap();

    // Measure LUFS
    let sq_sum: f64 = output_left.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    let rms = (sq_sum / output_left.len() as f64).sqrt();
    let rms_db = 20.0 * rms.log10();
    println!("Written: {}", out_path);
    println!("Frames: {}", output_left.len());
    println!("RMS: {:.1} dBFS", rms_db);
}
