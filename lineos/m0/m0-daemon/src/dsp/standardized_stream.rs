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

use blake3::Hasher as Blake3Hasher;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use sha2::{Digest, Sha256};

use crate::dsp::audio_source::AudioSource;
use crate::dsp::lazy_reader::LazyAudioReader;
use crate::dsp::signal_health::{DeadAirSummary, SignalHealthMonitor};

// Must match decode_smart byte-for-byte.
const TARGET_SR: u32 = 48_000;
const SINC_LEN: usize = 256;
const SINC_OVERSAMPLE: usize = 256;
const RESAMPLE_CHUNK_FRAMES: usize = 1024;

// Same guards as decode_smart.
const MAX_FILE_BYTES: u64 = 500 * 1024 * 1024;
const MAX_DURATION_SECS: u64 = 720;

// How many source frames to pull per reader call.
const READ_FRAMES: usize = 4096;

#[inline(always)]
fn sanitize_sample(s: f32) -> f32 {
    if s.is_nan() || s.is_infinite() {
        0.0
    } else {
        s.clamp(-1.0, 1.0)
    }
}

pub struct StandardizedAudioStream {
    reader: LazyAudioReader,
    orig_ch: usize,

    // None when orig_sr == 48k (passthrough).
    resampler: Option<SincFixedIn<f32>>,

    // Input ring: normalized (stereo) frames
    // waiting to reach 1024 for rubato. Stored
    // de-interleaved (one Vec per channel) since
    // that's what rubato consumes.
    in_l: Vec<f32>,
    in_r: Vec<f32>,

    // Output ring: resampled + sanitized
    // interleaved frames waiting to be served.
    out_ring: std::collections::VecDeque<f32>,

    // Raw pull buffer from the reader (interleaved
    // at orig_ch), reused each pull — bounded.
    read_buf: Vec<f32>,

    reader_eof: bool,
    flushed: bool,

    // Input identity hashes (over the FINAL
    // 48k/stereo/sanitized interleaved samples —
    // the same point decode_node hashes).
    blake3: Blake3Hasher,
    sha256: Sha256,

    health: SignalHealthMonitor,

    // Delivered-frame accounting for the trait's
    // total_frames_hint (best-effort).
    est_total_frames: Option<u64>,
}

impl StandardizedAudioStream {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        // File-size guard (metadata only).
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_FILE_BYTES {
                return Err(format!(
                    "Input too large: {} bytes \
                     exceeds {}MB limit",
                    meta.len(),
                    MAX_FILE_BYTES / (1024 * 1024)
                ));
            }
        }

        let reader = LazyAudioReader::open(path).map_err(|e| format!("standardized open: {e}"))?;
        let orig_sr = reader.sample_rate();
        let orig_ch = reader.channels();

        // Duration guard (metadata only — never
        // reads the audio).
        if let Some(frames) = reader.total_frames_hint() {
            let secs = frames / orig_sr.max(1) as u64;
            if secs > MAX_DURATION_SECS {
                return Err(format!(
                    "Input too long: {}s exceeds \
                     {}s (12min) limit",
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
            reader,
            orig_ch,
            resampler,
            in_l: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_r: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            out_ring: std::collections::VecDeque::new(),
            read_buf: Vec::new(),
            reader_eof: false,
            flushed: false,
            blake3: Blake3Hasher::new(),
            sha256: Sha256::new(),
            health: SignalHealthMonitor::new(TARGET_SR),
            est_total_frames,
        })
    }

    /// Pull one batch from the reader, channel-
    /// normalize to stereo, and append to the
    /// input ring (de-interleaved).
    fn pull_and_normalize(&mut self) -> Result<(), String> {
        if self.reader_eof {
            return Ok(());
        }
        // buffer length must be a multiple of ch.
        let want = READ_FRAMES * self.orig_ch;
        if self.read_buf.len() != want {
            self.read_buf = vec![0.0; want];
        }
        let frames = self
            .reader
            .fill_buffer(&mut self.read_buf)
            .map_err(|e| e.to_string())?;
        if frames == 0 {
            self.reader_eof = true;
            return Ok(());
        }
        let ch = self.orig_ch;
        for f in 0..frames {
            let base = f * ch;
            let (l, r) = match ch {
                1 => {
                    let s = self.read_buf[base];
                    (s, s) // mono → dup
                }
                2 => (self.read_buf[base], self.read_buf[base + 1]),
                _ => {
                    // downmix: even→L, odd→R
                    let mut ls = 0.0f32;
                    let mut le = 0usize;
                    let mut rs = 0.0f32;
                    let mut ro = 0usize;
                    let mut c = 0;
                    while c < ch {
                        if c % 2 == 0 {
                            ls += self.read_buf[base + c];
                            le += 1;
                        } else {
                            rs += self.read_buf[base + c];
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
            self.in_l.push(l);
            self.in_r.push(r);
        }
        Ok(())
    }

    /// Resample one full 1024-frame block from the
    /// front of the input ring and push the result
    /// (sanitized, interleaved) to the output ring.
    /// `pad` = zero-pad a short final block (EOF).
    fn process_one_block(&mut self, pad: bool) -> Result<(), String> {
        let have = self.in_l.len();
        if have == 0 {
            return Ok(());
        }
        let take = have.min(RESAMPLE_CHUNK_FRAMES);

        let (l_in, r_in): (Vec<f32>, Vec<f32>) = if take == RESAMPLE_CHUNK_FRAMES {
            (
                self.in_l[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_r[..RESAMPLE_CHUNK_FRAMES].to_vec(),
            )
        } else if pad {
            let mut l = self.in_l[..take].to_vec();
            let mut r = self.in_r[..take].to_vec();
            if self.resampler.is_some() {
                l.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                r.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
            }
            (l, r)
        } else {
            // Not enough for a full block and
            // not EOF — wait for more input.
            return Ok(());
        };

        // Drain the consumed frames from the ring.
        self.in_l.drain(..take);
        self.in_r.drain(..take);

        if let Some(rs) = self.resampler.as_mut() {
            let wave_in = vec![l_in, r_in];
            let wave_out = rs
                .process(&wave_in, None)
                .map_err(|e| format!("resample: {e}"))?;
            self.push_output(&wave_out[0], &wave_out[1]);
        } else {
            // Passthrough (already 48k).
            self.push_output(&l_in, &r_in);
        }
        Ok(())
    }

    /// Sanitize + interleave a resampled block,
    /// hash it, feed the health monitor, and
    /// queue it for delivery.
    fn push_output(&mut self, l: &[f32], r: &[f32]) {
        let mut interleaved = Vec::with_capacity(l.len() * 2);
        for (ls, rs) in l.iter().zip(r.iter()) {
            let ls = sanitize_sample(*ls);
            let rs = sanitize_sample(*rs);
            // Hash at this point = decode_node's
            // hash point (post-resample, post-
            // sanitize, interleaved). blake3 LE,
            // sha256 BE — exact parity.
            self.blake3.update(&ls.to_le_bytes());
            self.blake3.update(&rs.to_le_bytes());
            self.sha256.update(ls.to_be_bytes());
            self.sha256.update(rs.to_be_bytes());
            interleaved.push(ls);
            interleaved.push(rs);
        }
        self.health.observe(&interleaved);
        self.out_ring.extend(interleaved);
    }

    /// At EOF: pad+process the final short block,
    /// then flush the resampler tail. Idempotent.
    fn flush(&mut self) -> Result<(), String> {
        if self.flushed {
            return Ok(());
        }
        // Final partial block, zero-padded once.
        self.process_one_block(true)?;
        // Resampler tail (internal Sinc delay).
        if let Some(rs) = self.resampler.as_mut() {
            if let Ok(tail) = rs.process_partial::<Vec<f32>>(None, None) {
                if !tail[0].is_empty() {
                    let l = tail[0].clone();
                    let r = tail[1].clone();
                    self.push_output(&l, &r);
                }
            }
        }
        self.flushed = true;
        Ok(())
    }

    /// Finalize input hashes (call after the
    /// stream is fully drained).
    pub fn input_hashes(&self) -> (String, String) {
        let b = self.blake3.finalize().to_hex().to_string();
        let s = format!("{:x}", self.sha256.clone().finalize());
        (b, s)
    }

    pub fn tier1_verdict(&self) -> Result<(), String> {
        self.health.tier1_verdict()
    }

    pub fn tier2_verdict(&self) -> Result<(), String> {
        self.health.tier2_verdict()
    }

    /// Consume the stream and return the bounded
    /// dead-air summary (events capped, totals
    /// exact — O(1) memory on any duration).
    pub fn into_dead_air(self) -> DeadAirSummary {
        self.health.finish()
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
        self.est_total_frames
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let want_samples = buffer.len();
        // Keep producing until we can satisfy the
        // request or the stream is fully drained.
        while self.out_ring.len() < want_samples {
            if !self.reader_eof {
                self.pull_and_normalize()?;
                // Process every full block the ring
                // now holds (no padding mid-stream).
                while self.in_l.len() >= RESAMPLE_CHUNK_FRAMES {
                    self.process_one_block(false)?;
                }
            } else {
                // Reader drained: pad final block +
                // tail flush, once.
                self.flush()?;
                break;
            }
        }

        let n = want_samples.min(self.out_ring.len());
        for slot in buffer.iter_mut().take(n) {
            *slot = self.out_ring.pop_front().unwrap_or(0.0);
        }
        // Return FRAMES (stereo → /2).
        Ok(n / 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::audio_source::AudioSource;

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
        let path = "/tmp/std_parity_44k.wav";
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
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parity_passthrough_48k() {
        // 48k → resampler is None (passthrough).
        // Exercises the no-resample path.
        let path = "/tmp/std_parity_48k.wav";
        write_wav(path, 48_000, 3.3);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on 48k passthrough");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on 48k passthrough");
        let _ = std::fs::remove_file(path);
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
        let path = "/tmp/std_parity_mono44.wav";
        write_wav_mono(path, 44_100, 3.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(
            b_blake, s_blake,
            "blake3 mismatch on mono 44.1k — the \
             mono→stereo path diverges from batch"
        );
        assert_eq!(b_sha, s_sha, "sha256 mismatch on mono 44.1k");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parity_mono_48k() {
        // Mono 48k: mono→stereo normalize with
        // NO resampler (passthrough). Isolates
        // the channel path from the resample path.
        let path = "/tmp/std_parity_mono48.wav";
        write_wav_mono(path, 48_000, 3.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on mono 48k passthrough");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on mono 48k");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parity_downsample_96k() {
        // 96k → 48k: ratio < 1 (downsample).
        // Different resampler regime than upsample.
        let path = "/tmp/std_parity_96k.wav";
        write_wav(path, 96_000, 2.5);

        let (b_blake, b_sha) = batch_hashes(path);
        let (s_blake, s_sha) = stream_hashes(path);

        assert_eq!(b_blake, s_blake, "blake3 mismatch on 96k downsample");
        assert_eq!(b_sha, s_sha, "sha256 mismatch on 96k downsample");
        let _ = std::fs::remove_file(path);
    }
}
