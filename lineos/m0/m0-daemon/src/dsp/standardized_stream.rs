//! StandardizedAudioStream — the "second floor"
//! of the streaming pipeline.
//!
//! Wraps a LazyAudioReader and delivers clean,
//! 48 kHz, stereo, sanitized f32 audio via the
//! AudioSource trait — WITHOUT ever holding the
//! whole file in RAM. It reproduces exactly what
//! decode_smart does in batch (channel normalize
//! → resample → sanitize), so the input hash it
//! feeds the certificate is bit-identical to the
//! Music path for the same file.
//!
//! ## The parity-critical ring design
//!
//! decode_smart resamples the WHOLE track in
//! 1024-frame chunks and zero-pads ONLY the final
//! partial chunk. To match that byte-for-byte
//! while streaming, this stream:
//!   - accumulates normalized frames in an input
//!     ring until it has EXACTLY 1024,
//!   - feeds exactly 1024 to rubato, keeps the
//!     remainder,
//!   - zero-pads to 1024 ONLY when the underlying
//!     reader hits EOF with a partial remainder
//!     (once, like batch),
//!   - then runs process_partial for the tail.
//!
//! Padding mid-stream (whenever the ring is short)
//! would corrupt the resampled output and break
//! hash parity — so we never do that.
//!
//! Both rings are bounded, so RAM stays O(1)
//! regardless of file length.

use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction};

use crate::dsp::audio_source::AudioSource;
use crate::dsp::signal_health::{DeadAirSummary, SignalHealthMonitor};
use crate::dsp::stream_core::*;

pub use crate::dsp::stream_core::TARGET_SR;

// Same guards as decode_smart.
use crate::config::MAX_STREAM_FILE_BYTES;

pub struct StandardizedAudioStream {
    core: StandardizedStreamCore<2>,
    orig_ch: usize,
}

impl StandardizedAudioStream {
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
        let orig_ch = reader.channels();

        let expected_output_frames = reader.total_frames_hint().map(|src_frames| {
            if orig_sr == TARGET_SR {
                src_frames
            } else {
                ((src_frames as f64) * (TARGET_SR as f64 / orig_sr as f64)).round() as u64
            }
        });

        // Duration guard (metadata only — never
        // reads the audio).
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
                SincFixedIn::<f32>::new(ratio, 2.0, params, RESAMPLE_CHUNK_FRAMES, 2)
                    .map_err(|e| format!("resampler init: {e}"))?,
            )
        } else {
            None
        };

        // Estimate output frames for the hint:
        // orig_frames * ratio.
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
                Some(SignalHealthMonitor::new(TARGET_SR)),
            ),
            orig_ch,
        })
    }

    pub fn set_tap(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.core.set_tap(path)
    }

    /// Finalize input hashes (call after the
    /// stream is fully drained).
    pub fn input_hashes(&self) -> (String, String) {
        self.core.input_hashes()
    }

    pub fn tier1_verdict(
        &self,
        timing: crate::dsp::signal_health::VerdictTiming,
    ) -> Result<(), String> {
        self.core.health.as_ref().unwrap().tier1_verdict(timing)
    }

    pub fn tier2_verdict(&self) -> Result<(), String> {
        self.core.health.as_ref().unwrap().tier2_verdict()
    }

    /// Consume the stream and return the bounded
    /// dead-air summary (events capped, totals
    /// exact — O(1) memory on any duration).
    pub fn into_dead_air(self) -> DeadAirSummary {
        self.core.health.unwrap().finish()
    }

    /// See `expected_output_frames` field doc. Callers needing
    /// exact A/B length parity with the source should trim their
    /// written output to this many frames (if Some) rather than
    /// however many the stream happens to produce.
    pub fn expected_output_frames(&self) -> Option<u64> {
        self.core.expected_output_frames
    }
}

impl AudioSource for StandardizedAudioStream {
    fn sample_rate(&self) -> u32 {
        TARGET_SR
    }

    fn channels(&self) -> usize {
        2
    }

    fn total_frames_hint(&self) -> Option<u64> {
        self.core.est_total_frames
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let orig_ch = self.orig_ch;
        self.core.fill_buffer(buffer, |core| {
            if core.reader_eof {
                return Ok(());
            }
            let want = READ_FRAMES * orig_ch;
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
                let base = f * orig_ch;
                let (l, r) = match orig_ch {
                    1 => {
                        let s = core.read_buf[base];
                        (s, s) // mono → dup
                    }
                    2 => (core.read_buf[base], core.read_buf[base + 1]),
                    _ => {
                        // downmix: even→L, odd→R
                        let mut ls = 0.0f32;
                        let mut le = 0usize;
                        let mut rs = 0.0f32;
                        let mut ro = 0usize;
                        let mut c = 0;
                        while c < orig_ch {
                            if c % 2 == 0 {
                                ls += core.read_buf[base + c];
                                le += 1;
                            } else {
                                rs += core.read_buf[base + c];
                                ro += 1;
                            }
                            c += 1;
                        }
                        (
                            sanitize_sample(ls / le.max(1) as f32),
                            sanitize_sample(rs / ro.max(1) as f32),
                        )
                    }
                };
                core.push_input_frame([l, r]);
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::audio_source::AudioSource;
    use blake3::Hasher as Blake3Hasher;
    use sha2::{Digest, Sha256};

    /// Hash an interleaved f32 buffer exactly the
    /// way decode_node does: blake3 LE, sha256 BE.
    fn hash_interleaved(interleaved: &[f32]) -> (String, String) {
        let mut b = Blake3Hasher::new();
        let mut s = Sha256::new();
        for &x in interleaved {
            b.update(&x.to_le_bytes());
            s.update(x.to_be_bytes());
        }
        (
            b.finalize().to_hex().to_string(),
            format!("{:x}", s.finalize()),
        )
    }

    /// Write a stereo WAV at `sr` with a simple
    /// tone so decode_smart and the stream have
    /// identical input.
    fn write_wav(path: &str, sr: u32, secs: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let v = 0.3 * libm::sinf(2.0 * std::f32::consts::PI * 220.0 * t);
            w.write_sample(v).unwrap(); // L
            w.write_sample(v).unwrap(); // R
        }
        w.finalize().unwrap();
    }

    /// Drive the stream to EOF and return its
    /// finalized input hashes.
    fn stream_hashes(path: &str) -> (String, String) {
        let mut stream =
            StandardizedAudioStream::open(std::path::Path::new(path)).expect("open stream");
        // Pull in 4096-frame (×2 sample) chunks
        // until drained.
        let mut buf = vec![0.0f32; 4096 * 2];
        loop {
            let frames = stream.fill_buffer(&mut buf).expect("fill");
            if frames == 0 {
                break;
            }
        }
        stream.input_hashes()
    }

    /// Batch reference: decode_smart → interleave
    /// → hash (the Music path's input identity).
    fn batch_hashes(path: &str) -> (String, String) {
        let payload = crate::handlers::decode::decode_smart(path).expect("decode_smart");
        let interleaved = match payload {
            lineos_types::AudioPayload::Stereo(buf) => {
                let mut v = Vec::with_capacity(buf.num_frames * 2);
                for i in 0..buf.num_frames {
                    v.push(buf.left[i]);
                    v.push(buf.right[i]);
                }
                v
            }
            _ => panic!("expected stereo"),
        };
        hash_interleaved(&interleaved)
    }

    #[test]
    fn parity_resampled_44k() {
        // 44.1k → triggers resample + ring +
        // EOF zero-pad + tail flush. The hardest
        // path. Must be bit-identical to batch.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("std_parity_44k.wav");
        let path = path.to_str().unwrap();
        write_wav(path, 44_100, 3.7);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(
            b_blake, s_blake,
            "blake3 mismatch: streaming resample \
             is not bit-identical to batch"
        );
        assert_eq!(
            b_sha, s_sha,
            "sha256 mismatch: streaming resample \
             is not bit-identical to batch"
        );
    }

    #[test]
    fn parity_passthrough_48k() {
        // 48k → resampler is None (passthrough).
        // Exercises the no-resample path.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("std_parity_48k.wav");
        let path = path.to_str().unwrap();
        write_wav(path, 48_000, 3.3);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on 48k passthrough");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on 48k passthrough");
    }

    /// Write a MONO wav — the most common
    /// podcast upload (single-mic voice memo).
    fn write_wav_mono(path: &str, sr: u32, secs: f32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let v = 0.3 * libm::sinf(2.0 * std::f32::consts::PI * 220.0 * t);
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn parity_mono_44k() {
        // Mono 44.1k: exercises BOTH the
        // mono→stereo normalize path AND the
        // resampler. The most common real
        // podcast upload. Must match batch.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("std_parity_mono44.wav");
        let path = path.to_str().unwrap();
        write_wav_mono(path, 44_100, 3.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(
            b_blake, s_blake,
            "blake3 mismatch on mono 44.1k — the \
             mono→stereo path diverges from batch"
        );
        assert_eq!(b_sha, s_sha, "sha256 mismatch on mono 44.1k");
    }

    #[test]
    fn parity_mono_48k() {
        // Mono 48k: mono→stereo normalize with
        // NO resampler (passthrough). Isolates
        // the channel path from the resample path.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("std_parity_mono48.wav");
        let path = path.to_str().unwrap();
        write_wav_mono(path, 48_000, 3.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on mono 48k passthrough");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on mono 48k");
    }

    #[test]
    fn parity_downsample_96k() {
        // 96k → 48k: ratio < 1 (downsample).
        // Different resampler regime than upsample.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("std_parity_96k.wav");
        let path = path.to_str().unwrap();
        write_wav(path, 96_000, 2.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on 96k downsample");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on 96k downsample");
    }
}
