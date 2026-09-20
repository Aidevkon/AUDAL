//! Full-file input LUFS measurement — a dedicated, standardized
//! pre-pass separate from both the 30s scout sample (too short for
//! true EBU R128 integrated LUFS) and the post-render measured
//! wav→raw pass (measures OUTPUT, too late to inform gain staging).
//!
//! Reuses StandardizedAudioStream directly (48k/stereo/sanitized —
//! same contract as the render path) rather than the
//! StandardizedDecoder wrapper, since no tap/DecodeProvider
//! machinery is needed here — just measurement.
//!
//! This is the missing half of autotune-equivalent gain staging for
//! v3: v2 measures the full file before rendering and applies a
//! pre-gain; v3 currently applies no gain staging at all (confirmed
//! by recon 2026-07-17). This function supplies the missing
//! "measure the whole input" step; the gain computation and
//! application are separate, later steps (Parts B-continued/C).

use crate::audio_source::AudioSource;
use crate::standardized_decoder::StandardizedDecoder;
use crate::standardized_stream::StandardizedAudioStream;
use sp314_orchestrator::decode_provider::TappedDecoder;

const CHUNK_FRAMES: usize = 4096;

pub struct InputMetrics {
    pub integrated_lufs: Option<f32>,
    pub true_peak_dbtp: f32,
    /// Full-file BPM via StreamingBeatDetector. Y2b promotes this
    /// to the production ducking-gain path (replacing the 30s
    /// scout proxy). The full-file value is more accurate and
    /// computed in the same P0 pass — no additional decode.
    pub bpm: f32,
    /// Beat timestamps in ms (full-file StreamingBeatDetector).
    pub beats_ms: Vec<u32>,
    /// Downbeat timestamps in ms (full-file StreamingBeatDetector).
    pub downbeats_ms: Vec<u32>,
    /// Onset timestamps in ms (full-file StreamingBeatDetector).
    pub transients_ms: Vec<u32>,
}

/// Full-file input measurement — LUFS AND true peak, in one
/// standardized-stream pass. Extended from the original
/// LUFS-only measure_input_lufs (Part B, 67bc87c) to also serve
/// RunAnalysis's migration off decode_smart, without a second
/// full-file read for the same data.
pub fn measure_input_metrics(path: &std::path::Path) -> Result<InputMetrics, String> {
    let mut stream = StandardizedAudioStream::open(path)?;
    let mut lufs_meter = sp314_dsp::metering::LufsMeter::new();
    let mut peak_meter = sp314_dsp::metering::true_peak_meter::TruePeakMeter::new();
    let mut beat_detector = crate::beat_detector::StreamingBeatDetector::new(48_000);
    let mut buf = vec![0f32; CHUNK_FRAMES * 2];
    let mut left = vec![0f32; CHUNK_FRAMES];
    let mut right = vec![0f32; CHUNK_FRAMES];
    let mut mono = vec![0f32; CHUNK_FRAMES];

    loop {
        let frames = stream.fill_buffer(&mut buf)?;
        if frames == 0 {
            break;
        }
        for i in 0..frames {
            left[i] = buf[i * 2];
            right[i] = buf[i * 2 + 1];
            mono[i] = (left[i] + right[i]) * 0.5;
        }
        lufs_meter.process_chunk(&left[..frames], &right[..frames]);
        peak_meter.process_chunk(&left[..frames], &right[..frames]);
        beat_detector.feed_chunk(&mono[..frames]);
    }

    let (bpm, beats_ms, downbeats_ms, transients_ms) = beat_detector.finish();
    Ok(InputMetrics {
        integrated_lufs: lufs_meter.finish(),
        true_peak_dbtp: peak_meter.finish(),
        bpm,
        beats_ms,
        downbeats_ms,
        transients_ms,
    })
}

/// P0: decode→dump + input metrics in ONE pass.
///
/// Builds StandardizedDecoder → TappedDecoder (by value), streams the
/// full file to the raw PCM dump at `raw_tap_path`, and simultaneously
/// feeds LufsMeter + TruePeakMeter + StreamingBeatDetector on every
/// chunk — eliminating the dedicated `measure_input_metrics` decode
/// pass that previously ran separately in the RunStreaming path.
///
/// Returns `InputMetrics` (identical fields to `measure_input_metrics`)
/// plus the owned `StandardizedDecoder` (needed downstream for
/// `input_hashes`, `expected_output_frames`, `into_dead_air`).
///
/// A tap write failure is a HARD error: the dump is the artery for all
/// downstream passes (Pass 1 trunk, Pass 2 render). This differs from
/// `TappedDecoder`'s original best-effort policy, which was appropriate
/// when the dump was an optional A/B monitoring artifact.
///
/// NOTE: TappedDecoder's native-endian cast (`from_raw_parts` on `&[f32]`
/// → `&[u8]`) writes platform-endian bytes. On LE (x86-64/aarch64) this
/// matches the f32 LE contract `RawPcmFileSource` expects. On BE it
/// would silently produce a wrong dump — same latent issue as
/// `write_interleaved_dump` (inherited, not introduced here).
pub fn pass0_decode_to_dump(
    audio_path: &std::path::Path,
    raw_tap_path: &str,
) -> Result<(InputMetrics, StandardizedDecoder), String> {
    let std_decoder = StandardizedDecoder::open(audio_path)
        .map_err(|e| format!("P0: standardized decode open failed: {e}"))?;

    // TappedDecoder takes ownership of the decoder; we recover it
    // via into_inner() after streaming, matching the executor's
    // proven pattern (executor.rs:392 — main_decoder.into_inner()).
    let tapped = TappedDecoder::new(std_decoder, raw_tap_path);

    let mut lufs_meter = sp314_dsp::metering::LufsMeter::new();
    let mut peak_meter = sp314_dsp::metering::true_peak_meter::TruePeakMeter::new();
    let mut beat_detector = crate::beat_detector::StreamingBeatDetector::new(48_000);

    // Scratch buffers — allocated once, reused per chunk.
    let mut left = vec![0f32; CHUNK_FRAMES];
    let mut right = vec![0f32; CHUNK_FRAMES];
    let mut mono = vec![0f32; CHUNK_FRAMES];

    use sp314_dsp::io::decode_types::DecodeChunk;
    use sp314_orchestrator::decode_provider::DecodeProvider;

    let (_sample_rate, _channels) = tapped
        .stream_to(|chunk| -> Result<(), String> {
            if let DecodeChunk::Samples(interleaved) = chunk {
                let frames = interleaved.len() / 2;
                // Grow scratch if a chunk is larger than CHUNK_FRAMES
                // (unlikely — StandardizedDecoder uses 4096, but defensive).
                if frames > left.len() {
                    left.resize(frames, 0.0);
                    right.resize(frames, 0.0);
                    mono.resize(frames, 0.0);
                }
                for i in 0..frames {
                    left[i] = interleaved[i * 2];
                    right[i] = interleaved[i * 2 + 1];
                    mono[i] = (left[i] + right[i]) * 0.5;
                }
                lufs_meter.process_chunk(&left[..frames], &right[..frames]);
                peak_meter.process_chunk(&left[..frames], &right[..frames]);
                beat_detector.feed_chunk(&mono[..frames]);
            }
            Ok(())
        })
        .map_err(|e| format!("P0: decode stream failed: {e}"))?;

    // Tap failure is a HARD error — the dump is the artery.
    if let Some(e) = tapped.take_tap_error() {
        return Err(format!("P0: raw dump write failed ({}): {e}", raw_tap_path));
    }

    // Recover the owned StandardizedDecoder for downstream use
    // (input_hashes, expected_output_frames, into_dead_air).
    let std_decoder = tapped.into_inner();

    let (bpm, beats_ms, downbeats_ms, transients_ms) = beat_detector.finish();
    let metrics = InputMetrics {
        integrated_lufs: lufs_meter.finish(),
        true_peak_dbtp: peak_meter.finish(),
        bpm,
        beats_ms,
        downbeats_ms,
        transients_ms,
    };
    Ok((metrics, std_decoder))
}

/// Backward-compatible wrapper — the original LUFS-only signature,
/// used by the v3 executor's gain-staging call site (Part C,
/// 916eef5). Kept so that call site needs no change.
pub fn measure_input_lufs(path: &std::path::Path) -> Result<Option<f32>, String> {
    Ok(measure_input_metrics(path)?.integrated_lufs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &str, sample_rate: u32, src: &[f32]) {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
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
    fn measures_full_file_not_just_a_slice() {
        // 2 seconds of a constant-level tone — long enough for EBU
        // R128 gating (>400ms) but short enough for a fast test.
        // The key property under test: the function reads to EOF,
        // not just an early slice — verified by asserting a finite,
        // sane result rather than None (which would indicate a
        // premature/empty read).
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_input_lufs.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let n = 48_000 * 2;
        let src: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = 0.3 * (i as f32 * 0.05).sin();
                [v, v]
            })
            .collect();
        write_test_wav(wav_path, 48_000, &src);

        let result = measure_input_metrics(std::path::Path::new(wav_path)).unwrap();
        assert!(
            result.integrated_lufs.is_some(),
            "2s of tone should yield a measurable LUFS"
        );
        let lufs = result.integrated_lufs.unwrap();
        assert!(
            lufs.is_finite() && lufs < 0.0,
            "expected a plausible negative LUFS, got {lufs}"
        );
        assert!(
            result.true_peak_dbtp.is_finite() && result.true_peak_dbtp < 0.0,
            "expected a plausible negative true peak, got {}",
            result.true_peak_dbtp
        );
        assert!(result.bpm >= 0.0, "BPM should be non-negative");
    }

    #[test]
    fn matches_lufsmeter_reference_on_same_material() {
        // Cross-check: measuring via the standardized-stream path
        // should agree with a direct LufsMeter pass over the same
        // samples (proves the de-interleave/chunking here introduces
        // no measurement drift).
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_input_lufs_ref.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let n = 48_000 * 2;
        let src: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = 0.4 * (i as f32 * 0.02).sin();
                [v, v * 0.9]
            })
            .collect();
        write_test_wav(wav_path, 48_000, &src);

        let via_helper = measure_input_metrics(std::path::Path::new(wav_path)).unwrap();

        let mut ref_meter = sp314_dsp::metering::LufsMeter::new();
        let mut peak_meter = sp314_dsp::metering::true_peak_meter::TruePeakMeter::new();
        let left: Vec<f32> = src.chunks_exact(2).map(|f| f[0]).collect();
        let right: Vec<f32> = src.chunks_exact(2).map(|f| f[1]).collect();
        ref_meter.process_chunk(&left, &right);
        peak_meter.process_chunk(&left, &right);
        let reference = ref_meter.finish();
        let ref_peak = peak_meter.finish();

        assert_eq!(
            via_helper.integrated_lufs.map(|v| (v * 1000.0).round()),
            reference.map(|v| (v * 1000.0).round()),
            "standardized-stream measurement must match direct LufsMeter to 3 decimals"
        );
        assert_eq!(
            (via_helper.true_peak_dbtp * 1000.0).round(),
            (ref_peak * 1000.0).round(),
            "standardized-stream measurement must match direct TruePeakMeter to 3 decimals"
        );
        assert!(via_helper.bpm >= 0.0, "BPM should be non-negative");
    }

    #[test]
    fn too_short_for_gating_returns_none_not_error() {
        // 100ms — below EBU R128's 400ms gating minimum.
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_input_lufs_short.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let n = 4_800 * 2;
        let src: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = 0.3 * (i as f32 * 0.05).sin();
                [v, v]
            })
            .collect();
        write_test_wav(wav_path, 48_000, &src);

        let result = measure_input_metrics(std::path::Path::new(wav_path)).unwrap();
        assert!(
            result.integrated_lufs.is_none(),
            "sub-gating-window audio should yield None, not a bogus value"
        );
        assert!(
            result.true_peak_dbtp.is_finite(),
            "true peak is always valid"
        );
        assert_eq!(result.bpm, 0.0, "BPM should be 0.0 for audio too short");
    }

    #[test]
    fn wrapper_preserves_lufs_exactly() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_input_lufs_wrapper.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let n = 48_000 * 2;
        let src: Vec<f32> = (0..n)
            .flat_map(|i| {
                let v = 0.3 * (i as f32 * 0.05).sin();
                [v, v]
            })
            .collect();
        write_test_wav(wav_path, 48_000, &src);

        let from_metrics = measure_input_metrics(std::path::Path::new(wav_path))
            .unwrap()
            .integrated_lufs;
        let from_wrapper = measure_input_lufs(std::path::Path::new(wav_path)).unwrap();

        assert_eq!(
            from_metrics, from_wrapper,
            "wrapper should be a transparent passthrough"
        );
    }

    #[test]
    fn measures_correct_bpm_on_known_tempo_fixture() {
        // Same click-track pattern proven in beat_detector.rs's oracle
        // test yesterday (7e32e32) — sharp, precisely-timed transients
        // at a known BPM, a much stronger signal than a continuous tone
        // for validating tempo detection specifically.
        let tmp = tempfile::TempDir::new().unwrap();
        let wav_path_buf = tmp.path().join("test_input_metrics_bpm.wav");
        let wav_path = wav_path_buf.to_str().unwrap();
        let sr = 48_000u32;
        let bpm_target = 120.0f32;
        let duration_secs = 8.0f32;
        let n = (sr as f32 * duration_secs) as usize;
        let interval_samples = (60.0 / bpm_target * sr as f32) as usize;

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(wav_path, spec).unwrap();
        let mut samples = vec![0.0f32; n];
        let mut pos = 0;
        while pos + 20 < n {
            for i in 0..20 {
                samples[pos + i] = 0.9 * (1.0 - i as f32 / 20.0);
            }
            pos += interval_samples;
        }
        for &s in &samples {
            writer.write_sample(s).unwrap();
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();

        let result = measure_input_metrics(std::path::Path::new(wav_path)).unwrap();

        assert!(
            (result.bpm - bpm_target).abs() < 1.0,
            "expected ~{}bpm, got {}",
            bpm_target,
            result.bpm
        );
    }
}
