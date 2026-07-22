//! StandardizedSixChannelStream — the streaming resampler for 6ch audio.
//!
//! Wraps a LazyAudioReader and delivers clean, 48 kHz, 6-channel f32 PCM
//! via the AudioSource trait — WITHOUT ever holding the whole file in RAM.
//! Structurally mirrors StandardizedAudioStream exactly (ring buffer,
//! zero-padding at EOF) but tailored strictly for 6-channel routing.

use blake3::Hasher as Blake3Hasher;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use sha2::{Digest, Sha256};

use crate::dsp::audio_source::AudioSource;
use crate::dsp::lazy_reader::LazyAudioReader;

// Must match decode_smart byte-for-byte.
pub const TARGET_SR: u32 = 48_000;
const SINC_LEN: usize = 256;
const SINC_OVERSAMPLE: usize = 256;
const RESAMPLE_CHUNK_FRAMES: usize = 1024;

use crate::config::MAX_FILE_BYTES;
// Copied from StandardizedAudioStream for identical behavior.
// F-046: 720s contradicts the 8h target — both limits revisited
// together, deliberately not changed here.
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

pub struct StandardizedSixChannelStream {
    reader: LazyAudioReader,

    // None when orig_sr == 48k (passthrough).
    resampler: Option<SincFixedIn<f32>>,

    // Input ring: frames waiting to reach 1024 for rubato.
    // Stored de-interleaved (one Vec per channel).
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    in_c: Vec<f32>,
    in_lfe: Vec<f32>,
    in_ls: Vec<f32>,
    in_rs: Vec<f32>,

    // Output ring: resampled + sanitized interleaved
    // 6-channel frames waiting to be served.
    out_ring: std::collections::VecDeque<f32>,

    // Raw pull buffer from the reader (interleaved at 6ch).
    read_buf: Vec<f32>,

    reader_eof: bool,
    flushed: bool,

    // Input identity hashes (over the FINAL sanitized interleaved bytes).
    blake3: Blake3Hasher,
    sha256: Sha256,

    est_total_frames: Option<u64>,
    expected_output_frames: Option<u64>,
}

impl StandardizedSixChannelStream {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        // File-size guard (metadata only).
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > MAX_FILE_BYTES {
                return Err(format!(
                    "Input too large: {} bytes exceeds {}MB limit",
                    meta.len(),
                    MAX_FILE_BYTES / (1024 * 1024)
                ));
            }
        }

        let reader = LazyAudioReader::open(path).map_err(|e| format!("standardized open: {e}"))?;
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
                    "Input too long: {}s exceeds {}s (12min) limit",
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
            reader,
            resampler,
            in_l: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_r: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_c: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_lfe: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_ls: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            in_rs: Vec::with_capacity(RESAMPLE_CHUNK_FRAMES * 2),
            out_ring: std::collections::VecDeque::new(),
            read_buf: Vec::new(),
            reader_eof: false,
            flushed: false,
            blake3: Blake3Hasher::new(),
            sha256: Sha256::new(),
            est_total_frames,
            expected_output_frames,
        })
    }

    fn pull_and_normalize(&mut self) -> Result<(), String> {
        if self.reader_eof {
            return Ok(());
        }
        let want = READ_FRAMES * 6;
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
        for f in 0..frames {
            let base = f * 6;
            self.in_l.push(self.read_buf[base]);
            self.in_r.push(self.read_buf[base + 1]);
            self.in_c.push(self.read_buf[base + 2]);
            self.in_lfe.push(self.read_buf[base + 3]);
            self.in_ls.push(self.read_buf[base + 4]);
            self.in_rs.push(self.read_buf[base + 5]);
        }
        Ok(())
    }

    fn process_one_block(&mut self, pad: bool) -> Result<(), String> {
        let have = self.in_l.len();
        if have == 0 {
            return Ok(());
        }
        let take = have.min(RESAMPLE_CHUNK_FRAMES);

        let (l_in, r_in, c_in, lfe_in, ls_in, rs_in) = if take == RESAMPLE_CHUNK_FRAMES {
            (
                self.in_l[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_r[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_c[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_lfe[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_ls[..RESAMPLE_CHUNK_FRAMES].to_vec(),
                self.in_rs[..RESAMPLE_CHUNK_FRAMES].to_vec(),
            )
        } else if pad {
            let mut l = self.in_l[..take].to_vec();
            let mut r = self.in_r[..take].to_vec();
            let mut c = self.in_c[..take].to_vec();
            let mut lfe = self.in_lfe[..take].to_vec();
            let mut ls = self.in_ls[..take].to_vec();
            let mut rs = self.in_rs[..take].to_vec();
            if self.resampler.is_some() {
                l.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                r.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                c.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                lfe.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                ls.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                rs.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
            }
            (l, r, c, lfe, ls, rs)
        } else {
            return Ok(());
        };

        self.in_l.drain(..take);
        self.in_r.drain(..take);
        self.in_c.drain(..take);
        self.in_lfe.drain(..take);
        self.in_ls.drain(..take);
        self.in_rs.drain(..take);

        if let Some(rs_mut) = self.resampler.as_mut() {
            let wave_in = vec![l_in, r_in, c_in, lfe_in, ls_in, rs_in];
            let wave_out = rs_mut
                .process(&wave_in, None)
                .map_err(|e| format!("resample: {e}"))?;
            self.push_output(
                &wave_out[0],
                &wave_out[1],
                &wave_out[2],
                &wave_out[3],
                &wave_out[4],
                &wave_out[5],
            );
        } else {
            self.push_output(&l_in, &r_in, &c_in, &lfe_in, &ls_in, &rs_in);
        }
        Ok(())
    }

    fn push_output(
        &mut self,
        l: &[f32],
        r: &[f32],
        c: &[f32],
        lfe: &[f32],
        ls: &[f32],
        rs: &[f32],
    ) {
        let mut interleaved = Vec::with_capacity(l.len() * 6);
        for i in 0..l.len() {
            let sl = sanitize_sample(l[i]);
            let sr = sanitize_sample(r[i]);
            let sc = sanitize_sample(c[i]);
            let slfe = sanitize_sample(lfe[i]);
            let sls = sanitize_sample(ls[i]);
            let srs = sanitize_sample(rs[i]);

            // Hash at this point = decode_node's hash point.
            // blake3 LE, sha256 BE — exact parity with old 6ch arm.
            self.blake3.update(&sl.to_le_bytes());
            self.sha256.update(sl.to_be_bytes());

            self.blake3.update(&sr.to_le_bytes());
            self.sha256.update(sr.to_be_bytes());

            self.blake3.update(&sc.to_le_bytes());
            self.sha256.update(sc.to_be_bytes());

            self.blake3.update(&slfe.to_le_bytes());
            self.sha256.update(slfe.to_be_bytes());

            self.blake3.update(&sls.to_le_bytes());
            self.sha256.update(sls.to_be_bytes());

            self.blake3.update(&srs.to_le_bytes());
            self.sha256.update(srs.to_be_bytes());

            interleaved.push(sl);
            interleaved.push(sr);
            interleaved.push(sc);
            interleaved.push(slfe);
            interleaved.push(sls);
            interleaved.push(srs);
        }
        self.out_ring.extend(interleaved);
    }

    fn flush(&mut self) -> Result<(), String> {
        if self.flushed {
            return Ok(());
        }
        while !self.in_l.is_empty() {
            self.process_one_block(true)?;
        }
        if let Some(rs) = self.resampler.as_mut() {
            if let Ok(tail) = rs.process_partial::<Vec<f32>>(None, None) {
                if !tail[0].is_empty() {
                    self.push_output(&tail[0], &tail[1], &tail[2], &tail[3], &tail[4], &tail[5]);
                }
            }
        }
        self.flushed = true;
        Ok(())
    }

    pub fn input_hashes(&self) -> (String, String) {
        let b = self.blake3.finalize().to_hex().to_string();
        let s = format!("{:x}", self.sha256.clone().finalize());
        (b, s)
    }

    /// See `expected_output_frames` field doc. Callers needing
    /// exact A/B length parity with the source should trim their
    /// written output to this many frames (if Some) rather than
    /// however many the stream happens to produce.
    pub fn expected_output_frames(&self) -> Option<u64> {
        self.expected_output_frames
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
        self.est_total_frames
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let want_samples = buffer.len();
        while self.out_ring.len() < want_samples {
            if !self.reader_eof {
                self.pull_and_normalize()?;
                while self.in_l.len() >= RESAMPLE_CHUNK_FRAMES {
                    self.process_one_block(false)?;
                }
            } else {
                self.flush()?;
                break;
            }
        }

        let n = want_samples.min(self.out_ring.len());
        for slot in buffer.iter_mut().take(n) {
            *slot = self.out_ring.pop_front().unwrap_or(0.0);
        }
        // Return FRAMES (6 channels → /6).
        Ok(n / 6)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::audio_source::AudioSource;

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
