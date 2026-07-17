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

use crate::dsp::audio_source::AudioSource;
use crate::dsp::standardized_stream::StandardizedAudioStream;

const CHUNK_FRAMES: usize = 4096;

pub struct InputMetrics {
    pub integrated_lufs: Option<f32>,
    pub true_peak_dbtp: f32,
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
    let mut buf = vec![0f32; CHUNK_FRAMES * 2];
    let mut left = vec![0f32; CHUNK_FRAMES];
    let mut right = vec![0f32; CHUNK_FRAMES];

    loop {
        let frames = stream.fill_buffer(&mut buf)?;
        if frames == 0 {
            break;
        }
        for i in 0..frames {
            left[i] = buf[i * 2];
            right[i] = buf[i * 2 + 1];
        }
        lufs_meter.process_chunk(&left[..frames], &right[..frames]);
        peak_meter.process_chunk(&left[..frames], &right[..frames]);
    }

    Ok(InputMetrics {
        integrated_lufs: lufs_meter.finish(),
        true_peak_dbtp: peak_meter.finish(),
    })
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
        let wav_path = "/tmp/test_input_lufs.wav";
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
        let _ = std::fs::remove_file(wav_path);
    }

    #[test]
    fn matches_lufsmeter_reference_on_same_material() {
        // Cross-check: measuring via the standardized-stream path
        // should agree with a direct LufsMeter pass over the same
        // samples (proves the de-interleave/chunking here introduces
        // no measurement drift).
        let wav_path = "/tmp/test_input_lufs_ref.wav";
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
        let _ = std::fs::remove_file(wav_path);
    }

    #[test]
    fn too_short_for_gating_returns_none_not_error() {
        // 100ms — below EBU R128's 400ms gating minimum.
        let wav_path = "/tmp/test_input_lufs_short.wav";
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
        let _ = std::fs::remove_file(wav_path);
    }

    #[test]
    fn wrapper_preserves_lufs_exactly() {
        let wav_path = "/tmp/test_input_lufs_wrapper.wav";
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
        let _ = std::fs::remove_file(wav_path);
    }
}
