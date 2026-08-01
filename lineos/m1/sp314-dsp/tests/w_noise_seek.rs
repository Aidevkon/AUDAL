use sp314_dsp::analysis::acx_check::AcxCheckAnalyzer;
use sp314_dsp::stft::raw_pcm_source::RawPcmFileSource;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

#[test]
fn test_w_noise_seek_depth() {
    let url = "https://github.com/voxserv/audio_quality_testing_samples/raw/master/mono_44100/156550__acclivity__a-dream-within-a-dream.wav";
    let wav_path = "/tmp/narration_dream.wav";

    // Download if missing
    if !std::path::Path::new(wav_path).exists() {
        let status = Command::new("curl")
            .args(&["-sL", "-o", wav_path, url])
            .status()
            .expect("Failed to run curl");
        assert!(status.success(), "Failed to download dream fixture");
    }

    // Read via hound
    let mut reader = hound::WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let mut mono: Vec<f32> = if spec.sample_format == hound::SampleFormat::Int {
        reader
            .samples::<i16>()
            .map(|s| s.unwrap() as f32 / 32768.0)
            .collect()
    } else {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    };

    // AcxCheck runs on the mono float array
    let sample_rate = spec.sample_rate;
    let mut acx = AcxCheckAnalyzer::new(sample_rate);
    acx.feed_chunk(&mono);
    let result = acx.finish();

    // The known truth: ~ -74.32 dBFS
    assert!(
        (result.noise_floor_db.unwrap() - -74.32).abs() < 1.0,
        "ACX noise floor must match oracle"
    );

    let window_start_frame = result.quietest_window_start_frame.unwrap();
    let window_start_sec = window_start_frame as f32 / sample_rate as f32;
    // Expected to be around 64.9s
    assert!(
        (window_start_sec - 64.9).abs() < 1.0,
        "Quietest window must be near 64.9s"
    );

    // Write raw PCM dump (f32 LE) to test RawPcmFileSource
    let mut temp_file = NamedTempFile::new().unwrap();
    let raw_bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(mono.as_ptr() as *const u8, mono.len() * 4) };
    temp_file.write_all(raw_bytes).unwrap();
    temp_file.flush().unwrap();
    let temp_path = temp_file.into_temp_path();

    // 1 channel since it's mono
    let source = RawPcmFileSource::new(&temp_path, 1).unwrap();

    // Read 500ms window
    let len_frames = (0.5 * sample_rate as f32) as usize;
    let window = source.read_window(window_start_frame, len_frames);

    assert_eq!(window.len(), len_frames);

    // Compute RMS
    let sum_sq: f32 = window.iter().map(|&x| x * x).sum();
    let rms = (sum_sq / window.len() as f32).sqrt();
    let rms_db = 20.0 * rms.log10();

    println!(
        "W_NOISE|SEEK|depth_sec={:.3}|depth_frame={}|rms_db={:.2}",
        window_start_sec, window_start_frame, rms_db
    );

    // Assert RMS is near -74.3 dB
    assert!(
        (rms_db - -74.3).abs() < 3.0,
        "Seek window RMS must match expected noise floor"
    );
}
