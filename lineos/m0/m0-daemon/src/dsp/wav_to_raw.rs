/// Streaming WAV → raw f32 PCM conversion (O(1) RAM).
/// Reads the WAV sample-by-sample via hound and writes interleaved
/// native-endian f32 bytes — the layout xaak's playback mmaps.
/// Returns (frames_per_channel, sample_rate).
pub fn wav_to_raw_pcm(wav_path: &str, raw_path: &std::path::Path) -> Result<(usize, u32), String> {
    use std::io::Write;
    let mut reader = hound::WavReader::open(wav_path).map_err(|e| format!("open wav: {e}"))?;
    let spec = reader.spec();
    let file = std::fs::File::create(raw_path).map_err(|e| format!("create raw: {e}"))?;
    let mut w = std::io::BufWriter::new(file);
    let mut samples_written: usize = 0;
    for s in reader.samples::<f32>() {
        let v = s.map_err(|e| format!("read sample: {e}"))?;
        w.write_all(&v.to_ne_bytes())
            .map_err(|e| format!("write raw: {e}"))?;
        samples_written += 1;
    }
    w.flush().map_err(|e| format!("flush raw: {e}"))?;
    Ok((samples_written / spec.channels as usize, spec.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_to_raw_roundtrip_is_bit_identical() {
        let wav_path = "/tmp/test_w2r.wav";
        let raw_path = std::path::Path::new("/tmp/test_w2r.pcm");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let src: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.001).sin()).collect();
        let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
        for &v in &src {
            writer.write_sample(v).unwrap();
        }
        writer.finalize().unwrap();

        let (frames, sr) = wav_to_raw_pcm(wav_path, raw_path).unwrap();
        assert_eq!(frames, 500); // 1000 interleaved samples / 2 channels
        assert_eq!(sr, 48000);

        let bytes = std::fs::read(raw_path).unwrap();
        let round: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(round, src, "raw dump must be bit-identical to source");
        let _ = std::fs::remove_file(wav_path);
        let _ = std::fs::remove_file(raw_path);
    }
}
