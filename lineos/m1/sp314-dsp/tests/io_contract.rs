#![cfg(feature = "cli")]
#![allow(deprecated)]
// tests/io_contract.rs

use approx::assert_abs_diff_eq;
use sp314_dsp::io::{WavReader, WavWriter};
use std::fs;

#[test]
fn wav_roundtrip_preserves_samples() {
    let path = "tests/fixtures/test_roundtrip.wav";
    let len = 1000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        left[i] = s;
        right[i] = -s;
    }

    WavWriter::write(path, &left, &right, 48000).unwrap();

    let decoded = WavReader::read(path).unwrap();

    assert_eq!(decoded.left.len(), len);
    for i in 0..len {
        assert_abs_diff_eq!(decoded.left[i], left[i], epsilon = 1e-6);
        assert_abs_diff_eq!(decoded.right[i], right[i], epsilon = 1e-6);
    }

    let _ = fs::remove_file(path);
}

#[test]
fn wav_reader_handles_mono() {
    let path = "tests/fixtures/test_mono.wav";

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };

    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _i in 0..100 {
        writer.write_sample(0.5f32).unwrap();
    }
    writer.finalize().unwrap();

    let decoded = WavReader::read(path).unwrap();

    assert_eq!(decoded.num_channels, 1);
    assert_eq!(decoded.left, decoded.right);
    assert_eq!(decoded.left.len(), 100);
    assert_abs_diff_eq!(decoded.left[0], 0.5, epsilon = 1e-6);

    let _ = fs::remove_file(path);
}

#[test]
fn wav_reader_handles_16bit_pcm() {
    let path = "tests/fixtures/test_16bit.wav";

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    writer.write_sample(16384i16).unwrap();
    writer.finalize().unwrap();

    let decoded = WavReader::read(path).unwrap();

    assert_abs_diff_eq!(decoded.left[0], 0.5, epsilon = 1e-4);

    let _ = fs::remove_file(path);
}

fn mastered_path(input: &str, ext: &str) -> String {
    if let Some(pos) = input.rfind(".wav") {
        let (base, _ext) = input.split_at(pos);
        format!("{}_mastered.{}", base, ext)
    } else if let Some(pos) = input.rfind(".WAV") {
        let (base, _ext) = input.split_at(pos);
        format!("{}_mastered.{}", base, ext)
    } else {
        format!("{}_mastered.{}", input, ext)
    }
}

#[test]
fn wav_output_path_logic() {
    assert_eq!(mastered_path("song.wav", "wav"), "song_mastered.wav");
    assert_eq!(
        mastered_path("path/to/song.wav", "wav"),
        "path/to/song_mastered.wav"
    );
}

#[test]
fn flac_output_path_logic() {
    assert_eq!(mastered_path("song.wav", "flac"), "song_mastered.flac");
    assert_eq!(mastered_path("song.WAV", "flac"), "song_mastered.flac");
    assert_eq!(mastered_path("song", "wav"), "song_mastered.wav");
}

#[test]
fn flac_roundtrip_preserves_samples() {
    use claxon::FlacReader;
    use sp314_dsp::io::FlacWriter;

    let path = "tests/fixtures/test_roundtrip.flac";
    let len = 1000;
    let mut left = vec![0.0; len];
    let mut right = vec![0.0; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        left[i] = s;
        right[i] = -s;
    }

    FlacWriter::write(path, &left, &right, 48000).unwrap();

    let mut reader = FlacReader::open(path).unwrap();
    let streaminfo = reader.streaminfo();
    assert_eq!(streaminfo.channels, 2);
    assert_eq!(streaminfo.sample_rate, 48000);
    assert_eq!(streaminfo.bits_per_sample, 24);

    let mut samples = reader.samples();
    for i in 0..len {
        let l_i32 = samples.next().unwrap().unwrap();
        let r_i32 = samples.next().unwrap().unwrap();

        let l_f32 = l_i32 as f32 / 8388607.0_f32;
        let r_f32 = r_i32 as f32 / 8388607.0_f32;

        let tol = 1.0 / 8388608.0 * 2.0;
        assert_abs_diff_eq!(l_f32, left[i], epsilon = tol);
        assert_abs_diff_eq!(r_f32, right[i], epsilon = tol);
    }

    let _ = fs::remove_file(path);
}
