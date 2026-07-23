//! StandardizedSixChannelStream — the streaming resampler for 6ch audio.
//!
//! Wraps a LazyAudioReader and delivers clean, 48 kHz, 6-channel f32 PCM
//! via the AudioSource trait — WITHOUT ever holding the whole file in RAM.
//! Structurally mirrors StandardizedAudioStream exactly (ring buffer,
//! zero-padding at EOF) but tailored strictly for 6-channel routing.

use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

use crate::dsp::audio_source::AudioSource;
use crate::dsp::stream_core::*;

pub use crate::dsp::stream_core::StandardizedStreamCore;
pub use crate::dsp::stream_core::TARGET_SR;

use crate::config::MAX_STREAM_FILE_BYTES;

pub struct StandardizedSixChannelStream {
    core: StandardizedStreamCore<6>,
}

impl StandardizedSixChannelStream {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        // File-size guard (metadata only).
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_STREAM_FILE_BYTES {
                return Err(format!(
                    "Input too long: file size {} MB exceeds streaming limit of {} MB",
                    meta.len() / (1024 * 1024),
                    MAX_STREAM_FILE_BYTES / (1024 * 1024)
                ));
            }
        }

        let reader = crate::dsp::lazy_reader::LazyAudioReader::open(path)
            .map_err(|e| format!("standardized open: {e}"))?;
        let orig_sr = reader.sample_rate();

        // Contract: must be 6 channels explicitly.
        if reader.channels() != 6 {
            return Err(format!(
                "SixChannelStream requires exactly 6 channels, got {}",
                reader.channels()
            ));
        }

        let expected_output_frames = reader.total_frames_hint().map(|src_frames| {
            if orig_sr == TARGET_SR {
                src_frames
            } else {
                ((src_frames as f64) * (TARGET_SR as f64 / orig_sr as f64)).round() as u64
            }
        });

        // Duration guard (metadata only).
        if let Some(frames) = reader.total_frames_hint() {
            let secs = frames / orig_sr.max(1) as u64;
            if secs > MAX_DURATION_SECS {
                return Err(format!(
                    "Audio duration {}s exceeds max stream length {}s",
                    secs, MAX_DURATION_SECS
                ));
            }
        }

        let resampler = if orig_sr != TARGET_SR {
            let ratio = TARGET_SR as f64 / orig_sr as f64;
            let params = SincInterpolationParameters {
                sinc_len: SINC_LEN,
                f_cutoff: 0.95,
                interpolation: SincInterpolationType::Linear,
                oversampling_factor: SINC_OVERSAMPLE,
                window: WindowFunction::BlackmanHarris2,
            };
            Some(
                SincFixedIn::<f32>::new(ratio, 2.0, params, RESAMPLE_CHUNK_FRAMES, 6)
                    .map_err(|e| format!("resampler init: {e}"))?,
            )
        } else {
            None
        };

        let est_total_frames = reader.total_frames_hint().map(|f| {
            if orig_sr == TARGET_SR {
                f
            } else {
                ((f as f64) * (TARGET_SR as f64 / orig_sr as f64)) as u64
            }
        });

        Ok(Self {
            core: StandardizedStreamCore::new(
                reader,
                resampler,
                est_total_frames,
                expected_output_frames,
                None,
            ),
        })
    }

    pub fn input_hashes(&self) -> (String, String) {
        self.core.input_hashes()
    }

    /// See `expected_output_frames` field doc. Callers needing
    /// exact A/B length parity with the source should trim their
    /// written output to this many frames (if Some) rather than
    /// however many the stream happens to produce.
    pub fn expected_output_frames(&self) -> Option<u64> {
        self.core.expected_output_frames
    }
}

impl AudioSource for StandardizedSixChannelStream {
    fn sample_rate(&self) -> u32 {
        TARGET_SR
    }

    fn channels(&self) -> usize {
        6
    }

    fn total_frames_hint(&self) -> Option<u64> {
        self.core.est_total_frames
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        self.core.fill_buffer(buffer, |core| {
            if core.reader_eof {
                return Ok(());
            }
            let want = READ_FRAMES * 6;
            if core.read_buf.len() != want {
                core.read_buf = vec![0.0; want];
            }
            let frames = core
                .reader
                .fill_buffer(&mut core.read_buf)
                .map_err(|e| e.to_string())?;
            if frames == 0 {
                core.reader_eof = true;
                return Ok(());
            }
            for f in 0..frames {
                let base = f * 6;
                core.push_input_frame([
                    core.read_buf[base],
                    core.read_buf[base + 1],
                    core.read_buf[base + 2],
                    core.read_buf[base + 3],
                    core.read_buf[base + 4],
                    core.read_buf[base + 5],
                ]);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::audio_source::AudioSource;
    use sha2::Digest;

    fn write_wav_6ch(path: &str, sr: u32, secs: f32) {
        let spec = hound::WavSpec {
            channels: 6,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let v = 0.3 * libm::sinf(2.0 * std::f32::consts::PI * 220.0 * t);
            for _ in 0..6 {
                w.write_sample(v).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    fn rms(samples: &[f32]) -> f32 {
        let sum_sq: f32 = samples.iter().map(|&x| x * x).sum();
        (sum_sq / samples.len().max(1) as f32).sqrt()
    }

    fn drive_stream_full(path: &str) -> (usize, String, String, Vec<f32>, Option<u64>) {
        let mut stream =
            StandardizedSixChannelStream::open(std::path::Path::new(path)).expect("open stream");
        let mut buf = vec![0.0f32; 4096 * 6];
        let mut total_frames = 0;
        let mut last_sample = 0.0_f32;
        let mut all_samples = Vec::new();

        loop {
            let frames = stream.fill_buffer(&mut buf).expect("fill");
            if frames == 0 {
                break;
            }
            // Check discontinuity boundary
            if total_frames > 0 {
                let delta = (buf[0] - last_sample).abs();
                assert!(delta < 0.2, "Discontinuity at chunk boundary: {}", delta);
            }
            last_sample = buf[(frames - 1) * 6];
            all_samples.extend_from_slice(&buf[..frames * 6]);
            total_frames += frames;
        }
        let (b, s) = stream.input_hashes();
        (
            total_frames,
            b,
            s,
            all_samples,
            stream.expected_output_frames(),
        )
    }

    fn batch_decode_6ch(path: &str) -> (String, String, Vec<f32>) {
        let payload = crate::handlers::decode::decode_smart(path).expect("decode_smart");
        let interleaved = match payload {
            lineos_types::AudioPayload::FiveDotOne {
                channels,
                num_frames,
                ..
            } => {
                let mut v = Vec::with_capacity(num_frames * 6);
                for i in 0..num_frames {
                    for ch in &channels {
                        v.push(ch[i]);
                    }
                }
                v
            }
            _ => panic!("expected FiveDotOne"),
        };
        let mut b = blake3::Hasher::new();
        let mut s = sha2::Sha256::new();
        for &x in &interleaved {
            b.update(&x.to_le_bytes());
            s.update(x.to_be_bytes());
        }
        (
            b.finalize().to_hex().to_string(),
            format!("{:x}", s.finalize()),
            interleaved,
        )
    }

    #[test]
    fn six_channel_parity_passthrough_48k() {
        let path = "/tmp/six_parity_48k.wav";
        write_wav_6ch(path, 48_000, 3.3);

        let (b_blake, b_sha, _batch_samples) = batch_decode_6ch(path);
        let (total_frames, s_blake, s_sha, _stream_samples, _) = drive_stream_full(path);

        assert_eq!(total_frames, (48000.0 * 3.3) as usize, "Exact 48k frames");
        assert_eq!(b_blake, s_blake, "blake3 mismatch on 48k passthrough");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on 48k passthrough");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn six_channel_parity_resampled_44k() {
        let path = "/tmp/six_parity_44k.wav";
        write_wav_6ch(path, 44_100, 3.7);

        let (_b_blake, _b_sha, batch_samples) = batch_decode_6ch(path);
        let (total_frames, _s_blake, _s_sha, stream_samples, expected_hint) =
            drive_stream_full(path);

        let cap = expected_hint.expect("hint") as usize;
        assert!(
            total_frames >= cap,
            "Stream emitted less than the expected cap"
        );
        let excess = total_frames - cap;
        assert!(
            excess < 4096,
            "Excess padding/tail exceeds acceptable bound ({} frames)",
            excess
        );

        // Note: Hash parity only holds for the 48k passthrough path.
        // The batch resampler (resample_planar_to_48k) operates on the full file,
        // while the stream resampler uses a chunked SincFixedIn.
        // Because of the Sinc filter's stateful nature and the different chunking sizes,
        // their output floats will differ very slightly beyond the 5th decimal place.
        // Therefore, we assert relative RMS proximity (< 1% error) over the capped region.
        let cap_samples = cap * 6;
        let batch_rms = rms(&batch_samples[..cap_samples.min(batch_samples.len())]);
        let stream_rms = rms(&stream_samples[..cap_samples.min(stream_samples.len())]);
        let error = (batch_rms - stream_rms).abs() / batch_rms.max(1e-9);
        assert!(
            error < 0.01,
            "RMS mismatch: batch {} vs stream {} (error: {:.4})",
            batch_rms,
            stream_rms,
            error
        );

        let _ = std::fs::remove_file(path);
    }
}
