use std::path::Path;
use std::time::Duration;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Lazy, seekable audio reader over Symphonia.
/// Decodes packet-by-packet on demand instead
/// of loading the whole file into memory.
///
/// Two seek modes, deliberately separate:
/// - seek_approximate: cheap, packet-boundary
///   accuracy (~tens of ms). Use when exact
///   sample position doesn't matter (e.g. the
///   Scout's representative 30s sample).
/// - seek_exact_frame: expensive (decode +
///   discard until exact frame). Use when
///   sample accuracy matters (future parallel
///   render splicing, beat-locked automation,
///   stem phase alignment).
pub struct LazyAudioReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    /// Leftover interleaved samples from the
    /// last decoded packet that didn't fit in
    /// the caller's buffer yet.
    residual_samples: Vec<f32>,
    residual_pos: usize,
}

#[derive(Debug)]
pub enum LazyReaderError {
    NoSupportedTrack,
    Symphonia(String),
    InvalidBufferLength { len: usize, channels: usize },
}

impl std::fmt::Display for LazyReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSupportedTrack => write!(f, "no supported audio track found"),
            Self::Symphonia(e) => write!(f, "symphonia error: {e}"),
            Self::InvalidBufferLength { len, channels } => write!(
                f,
                "buffer length {len} is not a \
                 multiple of channel count \
                 {channels}"
            ),
        }
    }
}
impl std::error::Error for LazyReaderError {}

type Result<T> = std::result::Result<T, LazyReaderError>;

impl LazyAudioReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file =
            std::fs::File::open(path).map_err(|e| LazyReaderError::Symphonia(e.to_string()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| LazyReaderError::Symphonia(e.to_string()))?;

        let format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or(LazyReaderError::NoSupportedTrack)?;

        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(48000);
        let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| LazyReaderError::Symphonia(e.to_string()))?;

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            residual_samples: Vec::new(),
            residual_pos: 0,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Cheap seek to the nearest packet
    /// boundary at or before `target`.
    /// Returns the actual position reached
    /// (may be up to ~tens of ms before
    /// target — Symphonia is not sample-
    /// accurate on seek).
    pub fn seek_approximate(&mut self, target: Duration) -> Result<Duration> {
        let seeked = self
            .format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time: target.into(),
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|e| LazyReaderError::Symphonia(e.to_string()))?;

        self.decoder.reset();
        self.residual_samples.clear();
        self.residual_pos = 0;

        let actual_secs = seeked.actual_ts as f64 / self.sample_rate as f64;
        Ok(Duration::from_secs_f64(actual_secs))
    }

    /// Exact seek: cheap seek_approximate to
    /// the nearest packet boundary at or before
    /// target_frame, then decodes and discards
    /// samples until the internal position
    /// exactly matches target_frame.
    /// Expensive — only use when sample
    /// accuracy matters.
    pub fn seek_exact_frame(&mut self, target_frame: u64) -> Result<()> {
        let target_secs = target_frame as f64 / self.sample_rate as f64;

        let actual_time = self.seek_approximate(Duration::from_secs_f64(target_secs))?;

        let mut current_frame =
            (actual_time.as_secs_f64() * self.sample_rate as f64).round() as u64;

        if current_frame >= target_frame {
            // Seek landed exactly on or past
            // target (can happen with WAV,
            // which has near-arbitrary seek
            // granularity). Nothing to drain.
            return Ok(());
        }

        let frames_to_skip = target_frame - current_frame;
        let mut remaining = frames_to_skip;
        let mut scratch = vec![0.0_f32; 4096 * self.channels];

        while remaining > 0 {
            let want_frames = remaining.min(scratch.len() as u64 / self.channels as u64) as usize;
            let want_samples = want_frames * self.channels;
            let got = self.fill_buffer(&mut scratch[..want_samples])?;
            if got == 0 {
                // EOF before reaching target —
                // leave position at EOF.
                break;
            }
            current_frame += got as u64;
            remaining = target_frame.saturating_sub(current_frame);
        }

        Ok(())
    }

    pub fn total_frames_hint(&self) -> Option<u64> {
        self.format
            .tracks()
            .iter()
            .find(|t| t.id == self.track_id)
            .and_then(|t| t.codec_params.n_frames)
    }

    /// Fills `buffer` with interleaved samples
    /// (L,R,L,R... for stereo).
    /// `buffer.len()` MUST be a multiple of
    /// `self.channels()`.
    /// Returns frames written (NOT samples).
    /// 0 = end of stream.
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize> {
        if !buffer.len().is_multiple_of(self.channels) {
            return Err(LazyReaderError::InvalidBufferLength {
                len: buffer.len(),
                channels: self.channels,
            });
        }

        let mut written = 0usize;

        while written < buffer.len() {
            // Drain residual first.
            if self.residual_pos < self.residual_samples.len() {
                let avail = self.residual_samples.len() - self.residual_pos;
                let need = buffer.len() - written;
                let take = avail.min(need);
                buffer[written..written + take].copy_from_slice(
                    &self.residual_samples[self.residual_pos..self.residual_pos + take],
                );
                self.residual_pos += take;
                written += take;
                if self.residual_pos == self.residual_samples.len() {
                    self.residual_samples.clear();
                    self.residual_pos = 0;
                }
                continue;
            }

            // Need more data — decode next
            // packet.
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(written / self.channels);
                }
                Err(SymError::ResetRequired) => {
                    self.decoder.reset();
                    continue;
                }
                Err(e) => {
                    return Err(LazyReaderError::Symphonia(e.to_string()));
                }
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let spec = *decoded.spec();
                    let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                    sb.copy_interleaved_ref(decoded);
                    self.residual_samples.extend_from_slice(sb.samples());
                    self.residual_pos = 0;
                }
                Err(SymError::DecodeError(_)) => {
                    // Skip malformed packet,
                    // try next.
                    continue;
                }
                Err(e) => {
                    return Err(LazyReaderError::Symphonia(e.to_string()));
                }
            }
        }
        Ok(written / self.channels)
    }

    /// Allocates and reads exactly `frames_req` frames from the current
    /// reader position, returning (left, right) vectors.
    /// Handles channel downmixing (mono -> dual mono) automatically.
    pub fn read_exact_frames_alloc(&mut self, frames_req: u64) -> Result<(Vec<f32>, Vec<f32>)> {
        let channels = self.channels();
        if channels == 0 {
            return Err(LazyReaderError::NoSupportedTrack);
        }

        let frames_to_read = frames_req as usize;
        let mut interleaved = vec![0.0_f32; frames_to_read * channels];
        let mut total_written = 0usize;

        // Scratch buffer sized as a multiple of channels (critical for fill_buffer contract)
        let mut buf = vec![0.0_f32; 4096 * channels];

        while total_written < frames_to_read {
            let got_frames = self.fill_buffer(&mut buf)?;
            if got_frames == 0 {
                break;
            }
            let remaining = frames_to_read - total_written;
            let take = got_frames.min(remaining);

            let src_end = take * channels;
            let dst_start = total_written * channels;
            interleaved[dst_start..dst_start + src_end].copy_from_slice(&buf[..src_end]);

            total_written += take;
        }

        interleaved.truncate(total_written * channels);

        let left: Vec<f32> = interleaved.iter().step_by(channels).copied().collect();
        let right: Vec<f32> = if channels >= 2 {
            interleaved
                .iter()
                .skip(1)
                .step_by(channels)
                .copied()
                .collect()
        } else {
            left.clone()
        };

        Ok((left, right))
    }
}

/// Reads a representative ~30s sample from
/// the midpoint of an audio file using lazy
/// disk seek, instead of decoding the whole
/// file. Falls back gracefully: returns None
/// if the format doesn't expose upfront
/// frame-count metadata (n_frames), letting
/// the caller fall back to the existing
/// full-decode-then-slice path.
///
/// Uses seek_exact_frame (not approximate)
/// deliberately: this guarantees sample-
/// accurate parity with the existing
/// in-memory slicing path in
/// dsp_pipeline.rs. A packet-boundary-only
/// seek could land a few frames off,
/// causing a sample-by-sample comparison
/// to mismatch wave phase even though the
/// audio content is correct. Accuracy is
/// prioritized over speed here since the
/// scout runs once per file.
///
/// Returns (left, right, sample_rate) on
/// success.
pub fn read_scout_sample(
    path: &std::path::Path,
    sample_secs: f32,
) -> Option<(Vec<f32>, Vec<f32>, u32)> {
    let mut reader = LazyAudioReader::open(path).ok()?;

    let total_frames = reader.total_frames_hint()?;
    let sr = reader.sample_rate();
    let scout_frames = (sr as f32 * sample_secs) as u64;

    let (start_frame, frames_to_read) = if total_frames > scout_frames {
        ((total_frames - scout_frames) / 2, scout_frames)
    } else {
        (0, total_frames)
    };

    if start_frame > 0 {
        reader.seek_exact_frame(start_frame).ok()?;
    }

    let channels = reader.channels();
    let mut interleaved = vec![0.0_f32; (frames_to_read as usize) * channels];
    let mut total_written = 0usize;
    let mut buf = vec![0.0_f32; 4096 * channels];

    while total_written < frames_to_read as usize {
        let got = reader.fill_buffer(&mut buf).ok()?;
        if got == 0 {
            break;
        }
        let remaining = frames_to_read as usize - total_written;
        let take = got.min(remaining);
        let src_end = take * channels;
        let dst_start = total_written * channels;
        interleaved[dst_start..dst_start + src_end].copy_from_slice(&buf[..src_end]);
        total_written += take;
    }

    interleaved.truncate(total_written * channels);

    let left: Vec<f32> = interleaved.iter().step_by(channels).copied().collect();
    let right: Vec<f32> = if channels >= 2 {
        interleaved
            .iter()
            .skip(1)
            .step_by(channels)
            .copied()
            .collect()
    } else {
        left.clone()
    };

    Some((left, right, sr))
}

impl crate::dsp::audio_source::AudioSource for LazyAudioReader {
    fn sample_rate(&self) -> u32 {
        LazyAudioReader::sample_rate(self)
    }
    fn channels(&self) -> usize {
        LazyAudioReader::channels(self)
    }
    fn total_frames_hint(&self) -> Option<u64> {
        LazyAudioReader::total_frames_hint(self)
    }
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> std::result::Result<usize, String> {
        LazyAudioReader::fill_buffer(self, buffer).map_err(|e| e.to_string())
    }
}

impl crate::dsp::seekable_provider::AudioMetadataProvider for LazyAudioReader {
    fn sample_rate(&self) -> u32 {
        self.sample_rate()
    }
    fn channels(&self) -> usize {
        self.channels()
    }
    fn total_frames_hint(&self) -> Option<u64> {
        self.total_frames_hint()
    }
}

impl crate::dsp::seekable_provider::ApproximateSeekProvider for LazyAudioReader {
    fn seek_approximate(&mut self, target: std::time::Duration) -> Result<std::time::Duration> {
        self.seek_approximate(target)
    }
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize> {
        self.fill_buffer(buffer)
    }
}

impl crate::dsp::seekable_provider::ExactSeekProvider for LazyAudioReader {
    fn seek_exact_frame(&mut self, target_frame: u64) -> Result<()> {
        self.seek_exact_frame(target_frame)
    }
    fn read_exact_frames_alloc(&mut self, frames: u64) -> Result<(Vec<f32>, Vec<f32>)> {
        self.read_exact_frames_alloc(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &std::path::Path, sr: u32, dur_secs: f32) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let n = (sr as f32 * dur_secs) as usize;
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn fill_buffer_reads_all_frames() {
        let path = std::path::Path::new("/tmp/lazy_reader_test_basic.wav");
        write_test_wav(path, 48000, 5.0);

        let mut reader = LazyAudioReader::open(path).unwrap();
        assert_eq!(reader.sample_rate(), 48000);
        assert_eq!(reader.channels(), 2);

        let mut total_frames = 0u64;
        let mut buf = vec![0.0_f32; 1024];
        loop {
            let got = reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            total_frames += got as u64;
        }

        // 5 seconds @ 48kHz = 240000 frames.
        // Allow small tolerance for decoder
        // padding/priming samples.
        let expected = 48000 * 5;
        let diff = (total_frames as i64 - expected as i64).abs();
        assert!(
            diff < 100,
            "frame count mismatch: \
             got={} expected={} diff={}",
            total_frames,
            expected,
            diff
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fill_buffer_rejects_bad_length() {
        let path = std::path::Path::new("/tmp/lazy_reader_test_bad.wav");
        write_test_wav(path, 48000, 1.0);
        let mut reader = LazyAudioReader::open(path).unwrap();
        let mut buf = vec![0.0_f32; 3]; // not
                                        // mult
                                        // of 2
        let result = reader.fill_buffer(&mut buf);
        assert!(result.is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn seek_approximate_moves_position() {
        let path = std::path::Path::new("/tmp/lazy_reader_test_seek.wav");
        write_test_wav(path, 48000, 10.0);

        let mut reader = LazyAudioReader::open(path).unwrap();
        let target = std::time::Duration::from_secs(5);
        let actual = reader.seek_approximate(target).unwrap();

        // Coarse seek should land close to
        // target (within ~1 second is a safe
        // bound for WAV, which has no real
        // packet structure — should be exact
        // or near-exact).
        let diff = (actual.as_secs_f64() - target.as_secs_f64()).abs();
        assert!(
            diff < 1.0,
            "seek landed too far from \
             target: actual={:?} \
             target={:?}",
            actual,
            target
        );

        // After seek, reading should give
        // roughly 5 seconds worth of frames
        // (240000 - seek_point), not the
        // full file.
        let mut total = 0u64;
        let mut buf = vec![0.0_f32; 1024];
        loop {
            let got = reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            total += got as u64;
        }
        assert!(
            total < 48000 * 6,
            "seek didn't skip ahead — read \
             {} frames after seeking to 5s \
             on a 10s file",
            total
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fill_buffer_preserves_waveform_shape() {
        let path = std::path::Path::new("/tmp/lazy_reader_test_shape.wav");
        write_test_wav(path, 48000, 2.0);

        let mut reader = LazyAudioReader::open(path).unwrap();

        // Read with a buffer size that does NOT
        // align with typical packet sizes (1024,
        // 1152, 4096), forcing the residual logic
        // to actually split packets mid-way.
        let mut buf = vec![0.0_f32; 666];
        let mut all_samples: Vec<f32> = Vec::new();
        loop {
            let got = reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            all_samples.extend_from_slice(
                &buf[..got * 2], // *2 for stereo
            );
        }

        // Extract left channel only
        let left: Vec<f32> = all_samples.iter().step_by(2).copied().collect();

        // The original signal is a clean 440Hz
        // sine. Verify: no discontinuities
        // (sample-to-sample jumps larger than
        // physically possible for a 440Hz sine
        // at 48kHz would indicate a dropped or
        // duplicated sample at a packet/residual
        // boundary).
        let sr = 48000.0_f32;
        let freq = 440.0_f32;
        let max_slope = 2.0 * std::f32::consts::PI * freq / sr * 0.5; // amplitude*omega,
                                                                      // generous margin
        let mut max_jump = 0.0_f32;
        for w in left.windows(2) {
            let jump = (w[1] - w[0]).abs();
            if jump > max_jump {
                max_jump = jump;
            }
        }

        println!(
            "max_jump={:.6} max_slope_bound={:.6}",
            max_jump,
            max_slope * 3.0 // 3x margin
                            // for safety
        );

        assert!(
            max_jump < max_slope * 3.0,
            "Discontinuity detected — possible \
             dropped/duplicated sample at \
             packet or residual boundary. \
             max_jump={:.6} bound={:.6}",
            max_jump,
            max_slope * 3.0
        );

        // Also verify zero-crossing count matches
        // expected for a 440Hz tone over 2s:
        // ~440*2*2 = 1760 crossings (each full
        // cycle crosses zero twice).
        let mut crossings = 0;
        for w in left.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                crossings += 1;
            }
        }
        let expected_crossings = (freq * 2.0 * 2.0) as i32;
        let diff = (crossings - expected_crossings).abs();
        println!(
            "crossings={} expected={} diff={}",
            crossings, expected_crossings, diff
        );
        assert!(
            diff < 20,
            "Zero-crossing count mismatch \
             suggests corrupted samples: \
             got={} expected={}",
            crossings,
            expected_crossings
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn seek_exact_frame_lands_precisely() {
        let path = std::path::Path::new("/tmp/lazy_reader_test_exact.wav");
        write_test_wav(path, 48000, 10.0);

        let target_frame = 240_135u64; // ~5.003s,
                                       // deliberately
                                       // NOT on a
                                       // round
                                       // boundary

        let mut reader = LazyAudioReader::open(path).unwrap();
        reader.seek_exact_frame(target_frame).unwrap();

        // After exact seek, the very next sample
        // read should correspond EXACTLY to
        // target_frame in the original signal.
        // Reconstruct expected sample value at
        // that frame directly from the known
        // sine formula (same as write_test_wav).
        let sr = 48000.0_f32;
        let t = target_frame as f32 / sr;
        let expected = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;

        let mut buf = vec![0.0_f32; 2]; // one
                                        // stereo
                                        // frame
        let got = reader.fill_buffer(&mut buf).unwrap();
        assert_eq!(got, 1, "expected to read exactly 1 frame");

        let actual = buf[0]; // left channel
        let diff = (actual - expected).abs();

        println!(
            "target_frame={} expected={:.6} \
             actual={:.6} diff={:.6}",
            target_frame, expected, actual, diff
        );

        assert!(
            diff < 0.01,
            "seek_exact_frame landed on wrong \
             sample: target_frame={} \
             expected={:.6} actual={:.6} \
             diff={:.6}",
            target_frame,
            expected,
            actual,
            diff
        );
        let _ = std::fs::remove_file(path);
    }

    fn write_test_flac(path: &str, sr: u32, dur_secs: f32) {
        let n = (sr as f32 * dur_secs) as usize;
        let mut left = Vec::with_capacity(n);
        let mut right = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sr as f32;
            let s = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            left.push(s);
            right.push(s);
        }
        sp314_dsp::io::flac_writer::FlacWriter::write(path, &left, &right, sr).unwrap();
    }

    #[test]
    fn fill_buffer_preserves_waveform_flac() {
        let path = "/tmp/lazy_reader_test_shape.flac";
        write_test_flac(path, 48000, 3.0);

        let mut reader = LazyAudioReader::open(std::path::Path::new(path)).unwrap();

        // 666-sample buffer deliberately does
        // NOT align with FLAC's typical 4096-
        // frame block size, forcing residual
        // splitting across multiple block
        // boundaries — the real stress case
        // WAV couldn't exercise.
        let mut buf = vec![0.0_f32; 666];
        let mut all_samples: Vec<f32> = Vec::new();
        loop {
            let got = reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            all_samples.extend_from_slice(&buf[..got * 2]);
        }

        let left: Vec<f32> = all_samples.iter().step_by(2).copied().collect();

        let sr = 48000.0_f32;
        let freq = 440.0_f32;
        let max_slope = 2.0 * std::f32::consts::PI * freq / sr * 0.5;
        let mut max_jump = 0.0_f32;
        for w in left.windows(2) {
            let jump = (w[1] - w[0]).abs();
            if jump > max_jump {
                max_jump = jump;
            }
        }

        println!(
            "FLAC max_jump={:.6} \
             max_slope_bound={:.6}",
            max_jump,
            max_slope * 3.0
        );

        // FLAC is lossless at 24-bit, but our
        // encoder quantizes f32->24bit, so we
        // allow slightly more tolerance than
        // the WAV (32-bit float) test.
        assert!(
            max_jump < max_slope * 4.0,
            "Discontinuity in FLAC decode — \
             possible dropped/duplicated \
             sample at block boundary. \
             max_jump={:.6} bound={:.6}",
            max_jump,
            max_slope * 4.0
        );

        let mut crossings = 0;
        for w in left.windows(2) {
            if (w[0] >= 0.0) != (w[1] >= 0.0) {
                crossings += 1;
            }
        }
        let expected_crossings = (freq * 2.0 * 3.0) as i32; // 3s dur
        let diff = (crossings - expected_crossings).abs();
        println!(
            "FLAC crossings={} expected={} \
             diff={}",
            crossings, expected_crossings, diff
        );
        assert!(
            diff < 20,
            "FLAC zero-crossing mismatch: \
             got={} expected={}",
            crossings,
            expected_crossings
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn fill_buffer_handles_real_mp3() {
        let path = std::path::Path::new("/home/aidevcon/Music/liquid .mp3");
        if !path.exists() {
            eprintln!(
                "SKIP: real MP3 fixture not \
                 found at {:?}",
                path
            );
            return;
        }

        let mut reader = LazyAudioReader::open(path).unwrap();

        assert_eq!(reader.sample_rate(), 48000);
        assert_eq!(reader.channels(), 2);

        let mut total_frames = 0u64;
        let mut buf = vec![0.0_f32; 1024];
        let mut had_nan_or_inf = false;
        loop {
            let got = reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            if buf[..got * 2].iter().any(|s| !s.is_finite()) {
                had_nan_or_inf = true;
            }
            total_frames += got as u64;
        }

        let expected_frames = (176.112_f64 * 48000.0) as u64;
        let diff = (total_frames as i64 - expected_frames as i64).abs();

        println!(
            "MP3 total_frames={} expected~={} \
             diff={} had_nan_or_inf={}",
            total_frames, expected_frames, diff, had_nan_or_inf
        );

        assert!(!had_nan_or_inf, "MP3 decode produced NaN/Inf samples");

        // MP3 frame count can legitimately vary
        // by a few hundred ms due to encoder
        // delay/padding (LAME info tag etc) —
        // generous tolerance.
        assert!(
            diff < (48000 * 2) as i64, // <2s
            "MP3 frame count wildly off: \
             got={} expected~={}",
            total_frames,
            expected_frames
        );
    }

    #[test]
    fn probe_duration_hint_availability() {
        for (label, path) in [
            ("wav", "/tmp/lazy_reader_probe.wav"),
            ("flac", "/tmp/lazy_reader_probe.flac"),
        ] {
            if label == "wav" {
                write_test_wav(std::path::Path::new(path), 48000, 3.0);
            } else {
                write_test_flac(path, 48000, 3.0);
            }
            let reader = LazyAudioReader::open(std::path::Path::new(path)).unwrap();
            println!(
                "{}: total_frames_hint={:?}",
                label,
                reader.total_frames_hint()
            );
            let _ = std::fs::remove_file(path);
        }

        let mp3_path = std::path::Path::new("/home/aidevcon/Music/liquid .mp3");
        if mp3_path.exists() {
            let reader = LazyAudioReader::open(mp3_path).unwrap();
            println!("mp3: total_frames_hint={:?}", reader.total_frames_hint());
        }
    }

    #[test]
    fn read_scout_sample_matches_full_decode_slice() {
        let path = "/tmp/lazy_reader_scout_parity.wav";
        let sr = 48000u32;
        let dur = 90.0_f32; // 90s file, scout
                            // wants 30s from
                            // middle = frames
                            // [30s..60s]
        write_test_wav(std::path::Path::new(path), sr, dur);

        // "Old way": full decode then slice
        // (replicate what dsp_pipeline.rs does
        // today, using our own reader read-all
        // as the ground truth — NOT testing
        // decode_smart directly, just proving
        // the lazy path matches a full read).
        let mut full_reader = LazyAudioReader::open(std::path::Path::new(path)).unwrap();
        let mut full_left = Vec::new();
        let mut buf = vec![0.0_f32; 4096];
        loop {
            let got = full_reader.fill_buffer(&mut buf).unwrap();
            if got == 0 {
                break;
            }
            full_left.extend(buf[..got * 2].iter().step_by(2).copied());
        }
        let total_frames = full_left.len() as u64;
        let scout_frames = (sr as f32 * 30.0) as u64;
        let expected_start = (total_frames - scout_frames) / 2;
        let expected_slice =
            &full_left[expected_start as usize..(expected_start + scout_frames) as usize];

        // "New way": lazy seek + read
        let (lazy_left, _lazy_right, lazy_sr) = read_scout_sample(std::path::Path::new(path), 30.0)
            .expect(
                "lazy scout read should succeed \
                 for WAV with n_frames metadata",
            );

        assert_eq!(lazy_sr, sr);
        assert_eq!(lazy_left.len(), scout_frames as usize);

        // Compare sample-by-sample. With
        // seek_exact_frame this should match
        // essentially exactly.
        let mut max_diff = 0.0_f32;
        let cmp_len = lazy_left.len().min(expected_slice.len());
        for i in 0..cmp_len {
            let d = (lazy_left[i] - expected_slice[i]).abs();
            if d > max_diff {
                max_diff = d;
            }
        }
        println!(
            "scout parity: max_diff={:.6} \
             (lazy_len={} expected_len={})",
            max_diff,
            lazy_left.len(),
            expected_slice.len()
        );
        assert!(
            max_diff < 0.001,
            "lazy scout sample diverges from \
             full-decode-then-slice: \
             max_diff={:.6}",
            max_diff
        );

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn read_scout_sample_short_file_returns_all() {
        // File shorter than scout window (30s)
        // should return the whole file, not
        // panic or truncate weirdly.
        let path = "/tmp/lazy_reader_scout_short.wav";
        write_test_wav(std::path::Path::new(path), 48000, 5.0);
        let (left, _right, _sr) = read_scout_sample(std::path::Path::new(path), 30.0).unwrap();
        let expected = (48000.0_f32 * 5.0) as usize;
        let diff = (left.len() as i64 - expected as i64).abs();
        assert!(
            diff < 10,
            "short file scout read: got={} \
             expected~={}",
            left.len(),
            expected
        );
        let _ = std::fs::remove_file(path);
    }
}
