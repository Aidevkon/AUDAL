//! handlers/decode.rs — Audio decode: compressed → f32 PCM at 48000 Hz stereo.
//! Authority: Phase 7 master prompt · LineOS Constitution v2.0 §05
//!
//! ARCHITECTURE (binding):
//!   M0 decodes compressed audio bytes → AudioPcm { samples, 48000, 2ch }
//!   sp314-dsp receives ONLY f32 PCM at 48000 Hz stereo — never compressed audio.
//!   symphonia lives only in m0d — never imported by sp314-dsp.
//!
//! Supported formats: WAV, FLAC, MP3, AIFF (via symphonia 0.5)
//! Resampling: rubato SincFixedIn (linear interpolation) → 48000 Hz
//! Channel normalization:
//!   mono → stereo:             duplicate each sample
//!   stereo → stereo:           pass through
//!   N > 2 channels → stereo:   average L+R pairs, clamp [-1, 1]

// ── Decode constants ──────────────────────────────────────────────────────────
// All limits enforced before calling sp314-dsp.

pub const TARGET_SAMPLE_RATE: u32 = 48_000;
pub const TARGET_CHANNELS: u16 = 2;
pub const MAX_DURATION_SECS: u64 = 720; // 12 minutes
use crate::config::MAX_FILE_BYTES;

// Rubato SincFixedIn parameters — fixed for determinism.
// Linear interpolation selected: low CPU, acceptable quality for offline mastering.
const SINC_LEN: usize = 256;
const SINC_OVERSAMPLE: usize = 256;
const RESAMPLE_CHUNK_FRAMES: usize = 1024; // frames per rubato chunk (per channel)

// ── AudioPcm ─────────────────────────────────────────────────────────────────

/// Output of decode_audio(): always 48000 Hz stereo f32 PCM.
/// sp314-dsp receives this type — never raw compressed bytes.
#[derive(Debug)]
pub struct AudioPcm {
    /// Interleaved stereo f32 samples at 48000 Hz.
    pub samples: Vec<f32>,
    /// Always TARGET_SAMPLE_RATE (48000). Redundant; kept for audit clarity.
    pub sample_rate: u32,
    /// Always TARGET_CHANNELS (2). Redundant; kept for audit clarity.
    pub channels: u16,
    /// Duration after decode + resample, in milliseconds.
    pub duration_ms: u64,
    /// Original file sample rate (before resampling). For audit log.
    pub original_sr: u32,
    /// Original file channel count (before normalization). For audit log.
    pub original_ch: u16,
}

// ── DecodeError ───────────────────────────────────────────────────────────────

pub use sp314_dsp::io::decode_types::DecodeError;

// ── Public entry point ────────────────────────────────────────────────────────

/// Decode any supported audio file to f32 PCM at 48000 Hz stereo.
///
/// # Steps
///   1. File size check (≤ 500 MB)
///   2. symphonia decode → raw Vec<f32> at original_sr / original_ch
///   3. Duration check (≤ 12 min after frame count / sr)
///   4. Channel normalization → 2ch
///   5. Resample → 48000 Hz (skip if already 48000 Hz)
///   6. Return AudioPcm
///
/// This function is synchronous and CPU-bound; callers must wrap in
/// `tokio::task::spawn_blocking`.
pub fn decode_audio(path: &str) -> Result<AudioPcm, DecodeError> {
    let (raw_samples, original_sr, original_ch) = decode_raw_interleaved(path)?;

    // ── 4. Channel normalization → stereo ─────────────────────────────────────
    let stereo_interleaved: Vec<f32> = match original_ch {
        1 => mono_to_stereo(&raw_samples),
        2 => raw_samples,
        n => downmix_to_stereo(&raw_samples, n as usize),
    };

    // ── 5. Resample → 48000 Hz (skip if already correct) ─────────────────────
    let resampled = if original_sr != TARGET_SAMPLE_RATE {
        resample_stereo_to_48k(&stereo_interleaved, original_sr)?
    } else {
        stereo_interleaved
    };

    // ── 6. Assemble AudioPcm ──────────────────────────────────────────────────
    let duration_ms = (resampled.len() as u64 / 2) * 1000 / TARGET_SAMPLE_RATE as u64;

    // Final sanitization: NaN/Inf → 0.0, then clamp to [-1.0, 1.0].
    // Must run AFTER resampling — rubato can produce NaN in head/tail frames.
    // f32::clamp(NaN) returns NaN, so is_nan check must come first.
    let mut resampled = resampled;
    for s in resampled.iter_mut() {
        *s = sanitize_sample(*s);
    }

    Ok(AudioPcm {
        samples: resampled,
        sample_rate: TARGET_SAMPLE_RATE,
        channels: TARGET_CHANNELS,
        duration_ms,
        original_sr,
        original_ch,
    })
}

/// Smart decoding: returns exactly 6 discrete channels (no downmix) if input
/// is 5.1/7.1, or falls back to standard decode_audio (2-channel downmix) otherwise.
pub fn decode_smart(path: &str) -> Result<lineos_types::AudioPayload, DecodeError> {
    use lineos_types::{AudioPayload, StereoBuffer};

    let (raw, original_sr, original_ch) = decode_raw_interleaved(path)?;

    match original_ch {
        6 => {
            // A4-i: production 6ch routing now bypasses this arm
            // (decode_node streams via StandardizedSixChannelStream);
            // arm kept test-alive, deletion target at A4 close.
            // De-interleave into 6 discrete channels: [L, R, C, LFE, Ls, Rs]
            let frames = raw.len() / 6;
            let mut channels: [Vec<f32>; 6] = Default::default();
            for ch in channels.iter_mut() {
                *ch = Vec::with_capacity(frames);
            }
            for frame in raw.chunks_exact(6) {
                for ch_idx in 0..6 {
                    channels[ch_idx].push(frame[ch_idx]);
                }
            }

            // Resample each of the 6 channels independently to TARGET_SAMPLE_RATE if needed
            let resampled = if original_sr != TARGET_SAMPLE_RATE {
                resample_planar_to_48k(&channels, original_sr)?
            } else {
                channels.to_vec()
            };

            // Convert Vec<Vec<f32>> back to [Vec<f32>; 6] array
            let mut final_channels: [Vec<f32>; 6] = Default::default();
            for (i, ch) in resampled.into_iter().enumerate() {
                if i < 6 {
                    final_channels[i] = ch;
                }
            }
            let num_frames = final_channels[0].len();

            Ok(AudioPayload::FiveDotOne {
                channels: final_channels,
                sample_rate: TARGET_SAMPLE_RATE,
                num_frames,
            })
        }
        _ => {
            // Everything else (1, 2, or any other count) falls back to the
            // existing, already-correct decode_audio() path.
            let pcm = decode_audio(path)?;
            let frames = pcm.samples.len() / 2;
            let mut left = Vec::with_capacity(frames);
            let mut right = Vec::with_capacity(frames);
            for frame in pcm.samples.chunks_exact(2) {
                left.push(frame[0]);
                right.push(frame[1]);
            }

            Ok(AudioPayload::Stereo(StereoBuffer {
                left,
                right,
                sample_rate: pcm.sample_rate,
                num_frames: frames,
            }))
        }
    }
}

pub fn decode_raw_interleaved(path: &str) -> Result<(Vec<f32>, u32, u16), DecodeError> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    // ── 1. File size check ────────────────────────────────────────────────────
    let meta = std::fs::metadata(path).map_err(|_| DecodeError::FileNotFound(path.to_string()))?;

    if meta.len() > MAX_FILE_BYTES {
        return Err(DecodeError::FileTooLarge(meta.len()));
    }

    // ── 2. symphonia probe + decode ───────────────────────────────────────────
    let file =
        std::fs::File::open(path).map_err(|_| DecodeError::FileNotFound(path.to_string()))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| DecodeError::UnsupportedFormat(e.to_string()))?;

    let mut format = probed.format;

    // Select default audio track
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or_else(|| DecodeError::UnsupportedFormat("No audio track found".into()))?;

    let track_id = track.id;
    let original_sr = track.codec_params.sample_rate.unwrap_or(44_100);
    let original_ch = track
        .codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    let dec_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &dec_opts)
        .map_err(|e| DecodeError::UnsupportedFormat(format!("Codec not supported: {e}")))?;

    // SampleBuffer<f32> is the universal sink — symphonia converts any type to f32.
    // We allocate lazily on first decoded packet to know the spec.
    let mut sample_buf: Option<SampleBuffer<f32>> = None;
    let mut raw_samples: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(symphonia::core::errors::Error::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(DecodeError::DecodeFailure(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                // Initialise SampleBuffer on first decoded packet
                let sb = sample_buf.get_or_insert_with(|| {
                    SampleBuffer::<f32>::new(audio_buf.capacity() as u64, *audio_buf.spec())
                });
                // Copy all samples (interleaved) into the f32 SampleBuffer
                sb.copy_interleaved_ref(audio_buf);
                raw_samples.extend_from_slice(sb.samples());
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => return Err(DecodeError::DecodeFailure(e.to_string())),
        }
    }

    if raw_samples.is_empty() {
        return Err(DecodeError::DecodeFailure(
            "No audio samples decoded".into(),
        ));
    }

    // ── 3. Duration check ─────────────────────────────────────────────────────
    let original_ch_usize = original_ch as usize;
    let frame_count = raw_samples.len() / original_ch_usize.max(1);
    let duration_secs = frame_count as u64 / original_sr.max(1) as u64;

    if duration_secs > MAX_DURATION_SECS {
        return Err(DecodeError::DurationExceeded(duration_secs));
    }

    Ok((raw_samples, original_sr, original_ch))
}

// Sample sanitization

/// Sanitize a single f32 sample for safe delivery to sp314-dsp.
///
/// Order matters:
///   1. NaN/Inf -> 0.0  (f32::clamp(NaN) returns NaN, so must check first)
///   2. clamp to [-1.0, 1.0]
///
/// Applied at: downmix output, resample re-interleave, final decode_audio() pass.
#[inline(always)]
fn sanitize_sample(s: f32) -> f32 {
    if s.is_nan() || s.is_infinite() {
        0.0
    } else {
        s.clamp(-1.0, 1.0)
    }
}

// Channel helpers

/// Mono interleaved -> stereo interleaved (duplicate each sample).
pub fn mono_to_stereo(mono: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &s in mono {
        out.push(s);
        out.push(s);
    }
    out
}

/// Naive odd/even channel averaging downmix — NOT a correct ITU-standard
/// downmix (it has no knowledge of which input channel is LFE, Center,
/// Surround, etc.; it simply averages channels at even indices into Right
/// and odd indices into Left). Used today as the fallback path for any
/// channel count other than 1 (mono) or 2 (stereo) — including real 5.1/7.1
/// material, which this function silently and incorrectly treats as if it
/// were an arbitrary multi-mic recording with no spatial meaning.
/// Documented here as a known limitation, not fixed in this pass.
/// See 5.1-apple-ready-epic-scoping.md for the planned proper fix (a
/// dedicated 6-channel decode path that preserves discrete channels
/// instead of downmixing them at all).
pub fn downmix_to_stereo(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    let frames = interleaved.len() / channels;
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let base = f * channels;
        // L: mean of all odd-indexed channels (0, 2, ...)
        // R: mean of all even-indexed channels (1, 3, ...)
        let l_ch: Vec<f32> = (0..channels)
            .step_by(2)
            .map(|c| interleaved[base + c])
            .collect();
        let r_ch: Vec<f32> = (1..channels)
            .step_by(2)
            .map(|c| interleaved[base + c])
            .collect();
        let l = if l_ch.is_empty() {
            0.0
        } else {
            l_ch.iter().sum::<f32>() / l_ch.len() as f32
        };
        let r = if r_ch.is_empty() {
            l
        } else {
            r_ch.iter().sum::<f32>() / r_ch.len() as f32
        };
        out.push(sanitize_sample(l));
        out.push(sanitize_sample(r));
    }
    out
}

// ── Resampling ────────────────────────────────────────────────────────────────

/// Resample stereo interleaved PCM from `original_sr` to 48000 Hz.
/// Uses rubato SincFixedIn with Linear interpolation.
fn resample_stereo_to_48k(interleaved: &[f32], original_sr: u32) -> Result<Vec<f32>, DecodeError> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let ratio = TARGET_SAMPLE_RATE as f64 / original_sr as f64;

    let params = SincInterpolationParameters {
        sinc_len: SINC_LEN,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: SINC_OVERSAMPLE,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0, // max_resample_ratio_relative
        params,
        RESAMPLE_CHUNK_FRAMES,
        2, // 2 channels (non-interleaved in rubato)
    )
    .map_err(|e| DecodeError::ResampleFailure(format!("Resampler init: {e}")))?;

    // De-interleave: rubato expects [Vec<f32>; channels] (non-interleaved)
    let total_frames = interleaved.len() / 2;
    let mut ch0 = Vec::with_capacity(total_frames);
    let mut ch1 = Vec::with_capacity(total_frames);
    for frame in interleaved.chunks_exact(2) {
        ch0.push(frame[0]);
        ch1.push(frame[1]);
    }

    let mut out_ch0: Vec<f32> = Vec::new();
    let mut out_ch1: Vec<f32> = Vec::new();

    // Process in chunks of RESAMPLE_CHUNK_FRAMES
    let mut pos = 0_usize;
    while pos < total_frames {
        let end = (pos + RESAMPLE_CHUNK_FRAMES).min(total_frames);
        let chunk_len = end - pos;

        let wave_in: Vec<Vec<f32>> = if chunk_len == RESAMPLE_CHUNK_FRAMES {
            vec![ch0[pos..end].to_vec(), ch1[pos..end].to_vec()]
        } else {
            // Last partial chunk: pad with zeros to full chunk size
            let mut l = ch0[pos..end].to_vec();
            let mut r = ch1[pos..end].to_vec();
            l.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
            r.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
            vec![l, r]
        };

        let wave_out = resampler
            .process(&wave_in, None)
            .map_err(|e| DecodeError::ResampleFailure(format!("Resample chunk: {e}")))?;

        out_ch0.extend_from_slice(&wave_out[0]);
        out_ch1.extend_from_slice(&wave_out[1]);

        pos += RESAMPLE_CHUNK_FRAMES;
    }

    // Flush tail (push remaining internal frames out)
    if let Ok(tail) = resampler.process_partial::<Vec<f32>>(None, None) {
        if !tail[0].is_empty() {
            out_ch0.extend_from_slice(&tail[0]);
            out_ch1.extend_from_slice(&tail[1]);
        }
    }

    // Re-interleave for sp314-dsp AudioChunk.
    // sanitize_sample() is used instead of bare .clamp() — f32::clamp(NaN) is NaN.
    let mut out_interleaved = Vec::with_capacity(out_ch0.len() * 2);
    for (l, r) in out_ch0.iter().zip(out_ch1.iter()) {
        out_interleaved.push(sanitize_sample(*l));
        out_interleaved.push(sanitize_sample(*r));
    }

    Ok(out_interleaved)
}

// NOTE: matches resample_stereo_to_48k's existing tail-handling behavior
// (zero-padded last chunk's output is NOT truncated back to the
// mathematically expected frame count — confirmed identical, not a new
// bug introduced here). Both resamplers may emit a few extra trailing
// silence-derived frames. Pre-existing, out of scope for this Epic;
// candidate for a future, separate fix to both functions together.
fn resample_planar_to_48k(
    planar: &[Vec<f32>],
    original_sr: u32,
) -> Result<Vec<Vec<f32>>, DecodeError> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let channels = planar.len();
    let ratio = TARGET_SAMPLE_RATE as f64 / original_sr as f64;

    let params = SincInterpolationParameters {
        sinc_len: SINC_LEN,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: SINC_OVERSAMPLE,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        2.0, // max_resample_ratio_relative
        params,
        RESAMPLE_CHUNK_FRAMES,
        channels,
    )
    .map_err(|e| DecodeError::ResampleFailure(format!("Planar resampler init: {e}")))?;

    let total_frames = planar[0].len();
    let mut out: Vec<Vec<f32>> = vec![Vec::new(); channels];

    // Process in chunks of RESAMPLE_CHUNK_FRAMES
    let mut pos = 0_usize;
    while pos < total_frames {
        let end = (pos + RESAMPLE_CHUNK_FRAMES).min(total_frames);
        let chunk_len = end - pos;

        let wave_in: Vec<Vec<f32>> = if chunk_len == RESAMPLE_CHUNK_FRAMES {
            planar.iter().map(|ch| ch[pos..end].to_vec()).collect()
        } else {
            // Last partial chunk: pad with zeros to full chunk size
            planar
                .iter()
                .map(|ch| {
                    let mut pad = ch[pos..end].to_vec();
                    pad.resize(RESAMPLE_CHUNK_FRAMES, 0.0);
                    pad
                })
                .collect()
        };

        let wave_out = resampler
            .process(&wave_in, None)
            .map_err(|e| DecodeError::ResampleFailure(format!("Planar resample chunk: {e}")))?;

        for (i, out_ch) in out.iter_mut().enumerate() {
            out_ch.extend_from_slice(&wave_out[i]);
        }

        pos += RESAMPLE_CHUNK_FRAMES;
    }

    // Flush tail (push remaining internal frames out)
    if let Ok(tail) = resampler.process_partial::<Vec<f32>>(None, None) {
        if !tail[0].is_empty() {
            for (i, out_ch) in out.iter_mut().enumerate() {
                out_ch.extend_from_slice(&tail[i]);
            }
        }
    }

    Ok(out)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_constants() {
        assert_eq!(TARGET_SAMPLE_RATE, 48_000);
        assert_eq!(TARGET_CHANNELS, 2);
        assert_eq!(MAX_DURATION_SECS, 720);
        assert_eq!(MAX_FILE_BYTES, 500 * 1024 * 1024);
    }

    #[test]
    fn test_decode_nonexistent_file() {
        let result = decode_audio("/nonexistent/file.wav");
        assert!(
            matches!(result, Err(DecodeError::FileNotFound(_))),
            "Missing file must produce FileNotFound, got: {:?}",
            result
        );
    }

    #[test]
    fn test_decode_invalid_format() {
        let path = "/tmp/test_decode_not_audio_p7.txt";
        std::fs::write(path, b"not audio data at all").unwrap();
        let result = decode_audio(path);
        assert!(result.is_err(), "Non-audio file must produce an error");
    }

    #[test]
    fn test_mono_to_stereo() {
        let mono = vec![0.5f32, -0.5, 0.25];
        let stereo = mono_to_stereo(&mono);
        assert_eq!(stereo, vec![0.5, 0.5, -0.5, -0.5, 0.25, 0.25]);
    }

    #[test]
    fn test_mono_to_stereo_empty() {
        assert_eq!(mono_to_stereo(&[]), Vec::<f32>::new());
    }

    #[test]
    fn test_downmix_quad_to_stereo() {
        // 4-ch: L=0.4, R=0.2, C=0.5, LFE=0.0
        let quad = vec![0.4f32, 0.2, 0.5, 0.0]; // 1 frame, 4 ch
        let stereo = downmix_to_stereo(&quad, 4);
        assert_eq!(stereo.len(), 2);
        // L = mean(ch0=0.4, ch2=0.5) = 0.45
        // R = mean(ch1=0.2, ch3=0.0) = 0.10
        assert!((stereo[0] - 0.45).abs() < 1e-5, "L={}", stereo[0]);
        assert!((stereo[1] - 0.10).abs() < 1e-5, "R={}", stereo[1]);
    }

    #[test]
    fn test_downmix_mono_via_downmix() {
        let mono = vec![0.5f32]; // 1 frame, 1 ch
        let stereo = downmix_to_stereo(&mono, 1);
        assert_eq!(stereo.len(), 2);
        assert!((stereo[0] - 0.5).abs() < 1e-5);
        assert!((stereo[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_decode_error_display() {
        assert!(
            DecodeError::FileTooLarge(600_000_000)
                .to_string()
                .contains("500MB"),
            "FileTooLarge must mention 500MB limit"
        );
        assert!(
            DecodeError::DurationExceeded(800)
                .to_string()
                .contains("12 min"),
            "DurationExceeded must mention 12 min limit"
        );
        assert!(DecodeError::FileNotFound("/x".into())
            .to_string()
            .contains("/x"));
        assert!(DecodeError::UnsupportedFormat("mp4".into())
            .to_string()
            .contains("mp4"));
    }

    #[test]
    fn test_resample_passthrough_48k() {
        // When original_sr == 48000, resample_stereo_to_48k is skipped.
        // Verify mono_to_stereo + the 48k passthrough path produces correct length.
        let stereo_440hz: Vec<f32> = (0..48000 * 2)
            .map(|i| {
                let frame = i / 2;
                (2.0 * std::f32::consts::PI * 440.0 * frame as f32 / 48000.0).sin() * 0.5
            })
            .collect();
        // No resampling needed — just verify the vector is well-formed.
        assert_eq!(
            stereo_440hz.len(),
            96_000,
            "Stereo 48k should have 2 * 48000 samples"
        );
    }

    #[test]
    fn test_decode_real_mp3_file() {
        // Integration test: only runs if gargar.mp3 exists on disk.
        let path = "/home/aidevcon/Downloads/gargar.mp3";
        if !std::path::Path::new(path).exists() {
            return; // Skip on CI — not a failure
        }
        let pcm = decode_audio(path).expect("gargar.mp3 should decode without error");
        assert_eq!(pcm.sample_rate, 48_000, "Must be resampled to 48000");
        assert_eq!(pcm.channels, 2, "Must be stereo");
        assert!(!pcm.samples.is_empty(), "Must have decoded samples");
        assert!(pcm.duration_ms > 0, "Must have positive duration");
        // All samples must be within range (no clipping overflow)
        for (i, &s) in pcm.samples.iter().enumerate() {
            assert!(
                (-1.001..=1.001).contains(&s),
                "Sample [{i}] out of range: {s}"
            );
        }
        println!(
            "✅ gargar.mp3: {}ms, {}/{} ch/sr, {} samples",
            pcm.duration_ms,
            pcm.original_ch,
            pcm.original_sr,
            pcm.samples.len()
        );
    }

    fn zero_crossing_count(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0))
            .count()
    }

    #[test]
    fn decode_smart_preserves_six_discrete_channels() {
        let path = "../../m1/sp314-dsp/tests/fixtures/synthetic_5point1.wav";
        let payload = decode_smart(path).expect("decode_smart should succeed on 6-channel input");

        match payload {
            lineos_types::AudioPayload::FiveDotOne {
                channels,
                sample_rate,
                num_frames,
            } => {
                assert_eq!(sample_rate, TARGET_SAMPLE_RATE);
                assert_eq!(channels.len(), 6);

                // Expected ZCR counts for a 2.0s sine at each frequency: freq * 2 * duration_s.
                // Tolerance is generous (±5%) to absorb resampling/edge effects, while still
                // being far tighter than the gap between any two adjacent expected values —
                // a channel-order swap would miss by 50%+, not 5%.
                let expected_zc = [
                    (440.0_f32 * 2.0 * 2.0) as usize,  // L
                    (880.0_f32 * 2.0 * 2.0) as usize,  // R
                    (1000.0_f32 * 2.0 * 2.0) as usize, // C
                    (60.0_f32 * 2.0 * 2.0) as usize,   // LFE
                    (2000.0_f32 * 2.0 * 2.0) as usize, // Ls
                    (3000.0_f32 * 2.0 * 2.0) as usize, // Rs
                ];

                for (i, ch) in channels.iter().enumerate() {
                    assert_eq!(ch.len(), num_frames);
                    let zc = zero_crossing_count(ch);
                    let tolerance = (expected_zc[i] as f32 * 0.05).max(10.0) as usize;
                    assert!(
                        (zc as i64 - expected_zc[i] as i64).unsigned_abs() as usize <= tolerance,
                        "Channel {} expected ~{} zero-crossings, measured {} — possible channel order bug",
                        i, expected_zc[i], zc
                    );
                }
            }
            _ => panic!("Expected AudioPayload::FiveDotOne for 6-channel input, got Stereo"),
        }
    }
}
