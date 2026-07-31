/// Streaming WAV → raw f32 PCM conversion with in-flight
/// certification measurement (O(1) RAM).
///
/// One read pass, five jobs: writes interleaved native-endian f32
/// raw bytes (xaak's mmap layout), and feeds the streaming
/// LufsMeter, LraCalculator, true-peak max, and the two
/// certification hashers. Byte orders copy episode_render's Pass 3
/// EXACTLY — the "identical certificate" guarantee depends on them:
///   - blake3:  left channel only, f32 LE (matches
///     certificate::blake3_pcm)
///   - sha256:  interleaved L+R, f32 BE (matches
///     ExecutionProof::hash_pcm)
use sha2::{Digest, Sha256};

/// Field names mirror EpisodeRenderResult where they overlap, so
/// the executor's StreamingCertData assembly is a 1:1 mapping.
pub struct MeasuredOutput {
    pub frames_written: usize,
    pub sample_rate: u32,
    pub output_lufs: f32,
    pub output_lra: f32,
    pub true_peak_dbtp: f32,
    pub pcm_blake3: String,
    pub output_sha256: String,
}

const CHUNK_FRAMES: usize = 4096;

pub fn wav_to_raw_measured(
    wav_path: &str,
    raw_path: &std::path::Path,
) -> Result<MeasuredOutput, String> {
    use std::io::Write;
    let mut reader = hound::WavReader::open(wav_path).map_err(|e| format!("open wav: {e}"))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err("wav has zero channels".into());
    }
    let file = std::fs::File::create(raw_path).map_err(|e| format!("create raw: {e}"))?;
    let mut w = std::io::BufWriter::new(file);

    let mut lufs_meter = sp314_dsp::metering::LufsMeter::new();
    let mut lra_calc = lineos_telemetry::lra::LraCalculator::new(spec.sample_rate);
    let mut true_peak_linear = 0f32;
    let mut blake3 = blake3::Hasher::new();
    let mut sha256 = Sha256::new();

    let mut interleaved: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * channels);
    let mut left_buf = vec![0f32; CHUNK_FRAMES];
    let mut right_buf = vec![0f32; CHUNK_FRAMES];
    let mut frames_written: usize = 0;

    let mut samples = reader.samples::<f32>();
    loop {
        interleaved.clear();
        for _ in 0..(CHUNK_FRAMES * channels) {
            match samples.next() {
                Some(s) => interleaved.push(s.map_err(|e| format!("read sample: {e}"))?),
                None => break,
            }
        }
        if interleaved.is_empty() {
            break;
        }
        let frames = interleaved.len() / channels;
        for i in 0..frames {
            left_buf[i] = interleaved[i * channels];
            right_buf[i] = if channels >= 2 {
                interleaved[i * channels + 1]
            } else {
                left_buf[i]
            };
        }

        lufs_meter.process_chunk(&left_buf[..frames], &right_buf[..frames]);
        lra_calc.process_chunk(&left_buf[..frames], &right_buf[..frames]);
        for i in 0..frames {
            true_peak_linear = true_peak_linear
                .max(left_buf[i].abs())
                .max(right_buf[i].abs());
            blake3.update(&left_buf[i].to_le_bytes());
            sha256.update(left_buf[i].to_be_bytes());
            sha256.update(right_buf[i].to_be_bytes());
        }

        for &v in &interleaved {
            w.write_all(&v.to_ne_bytes())
                .map_err(|e| format!("write raw: {e}"))?;
        }
        frames_written += frames;
    }
    w.flush().map_err(|e| format!("flush raw: {e}"))?;

    let output_lufs = lufs_meter.finish().unwrap_or(f32::NEG_INFINITY);
    let output_lra = lra_calc.compute();
    let true_peak_dbtp = if true_peak_linear > 0.0 {
        20.0 * true_peak_linear.log10()
    } else {
        f32::NEG_INFINITY
    };

    Ok(MeasuredOutput {
        frames_written,
        sample_rate: spec.sample_rate,
        output_lufs,
        output_lra,
        true_peak_dbtp,
        pcm_blake3: blake3.finalize().to_hex().to_string(),
        output_sha256: format!("{:x}", sha256.finalize()),
    })
}

/// Compatibility wrapper — the pre-C1 API. Callers migrate to
/// wav_to_raw_measured in the C3 wiring task.
pub fn wav_to_raw_pcm(wav_path: &str, raw_path: &std::path::Path) -> Result<(usize, u32), String> {
    let m = wav_to_raw_measured(wav_path, raw_path)?;
    Ok((m.frames_written, m.sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &str, src: &[f32]) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for &v in src {
            writer.write_sample(v).unwrap();
        }
        writer.finalize().unwrap();
    }

    #[test]
    fn wav_to_raw_roundtrip_is_bit_identical() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_w2r.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let raw_path_buf = tmp.path().join("test_w2r.pcm");
        let raw_path = std::path::Path::new(&raw_path_buf);
        let src: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.001).sin()).collect();
        write_test_wav(wav_path, &src);

        let m = wav_to_raw_measured(wav_path, raw_path).unwrap();
        assert_eq!(m.frames_written, 500);
        assert_eq!(m.sample_rate, 48000);

        let bytes = std::fs::read(raw_path).unwrap();
        let round: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        assert_eq!(round, src, "raw dump must be bit-identical to source");
    }

    #[test]
    fn measured_pass_matches_reference_hashes_and_peak() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_w2r_m.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let raw_path_buf = tmp.path().join("test_w2r_m.pcm");
        let raw_path = std::path::Path::new(&raw_path_buf);
        // 2 seconds of audio so LUFS gating has material (>400ms)
        let n = 96000 * 2;
        let src: Vec<f32> = (0..n).map(|i| 0.5 * (i as f32 * 0.01).sin()).collect();
        write_test_wav(wav_path, &src);

        let m = wav_to_raw_measured(wav_path, raw_path).unwrap();

        // Reference hashes computed monolithically, same byte orders
        let mut blake3_ref = blake3::Hasher::new();
        let mut sha_ref = Sha256::new();
        let mut peak_ref = 0f32;
        for f in src.chunks_exact(2) {
            blake3_ref.update(&f[0].to_le_bytes());
            sha_ref.update(f[0].to_be_bytes());
            sha_ref.update(f[1].to_be_bytes());
            peak_ref = peak_ref.max(f[0].abs()).max(f[1].abs());
        }
        assert_eq!(m.pcm_blake3, blake3_ref.finalize().to_hex().to_string());
        assert_eq!(m.output_sha256, format!("{:x}", sha_ref.finalize()));
        let peak_ref_db = 20.0 * peak_ref.log10();
        assert!((m.true_peak_dbtp - peak_ref_db).abs() < 1e-4);
        assert!(m.output_lufs.is_finite(), "2s of audio must yield LUFS");
    }
}
