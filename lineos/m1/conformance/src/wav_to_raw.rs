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
    // ── F-085: μετρήσεις ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ, όχι της εισόδου ──
    // Ζουν ΕΔΩ, δίπλα στα output_lufs/output_lra/true_peak_dbtp,
    // επειδή αυτό το πέρασμα διαβάζει το ΜΑΣΤΕΡΑΡΙΣΜΕΝΟ wav. Το
    // trunk pass μετράει το raw_tap ΠΡΙΝ τον render (executor.rs:282
    // vs :355) — οι τιμές του περιγράφουν την ΕΙΣΟΔΟ και δεν
    // επιτρέπεται να μπουν σε μπλοκ που περιγράφει την ΕΞΟΔΟ.
    /// Πλήρους αρχείου συσχέτιση L/R του παραδοτέου.
    pub output_stereo_correlation: f32,
    /// p95−p5 των per-block RMS του παραδοτέου (DynamicsAnalyzer).
    pub output_dynamic_range_db: f32,
    /// Μη-σταθμισμένο RMS του mono downmix του παραδοτέου.
    pub output_rms_db: f32,
    /// 1 − |correlation| του παραδοτέου. ΠΑΡΑΓΩΓΟ του
    /// `output_stereo_correlation` (stereo.rs:32) — ταξιδεύει επειδή το
    /// σχήμα έχει το πεδίο, όχι επειδή προσθέτει πληροφορία.
    pub output_stereo_width: f32,
    /// Φασματικό κέντρο βάρους (Hz) του mono downmix του παραδοτέου,
    /// μέσος όρος ανά STFT frame.
    pub output_spectral_centroid: f32,
    /// Φασματική επιπεδότητα (geom/arith) του mono downmix του
    /// παραδοτέου, μέσος όρος ανά STFT frame. 0 = τόνος, 1 = θόρυβος.
    pub output_spectral_flatness: f32,
    /// Γεγονότα clipping ΤΟΥ ΠΑΡΑΔΟΤΕΟΥ (≥3 διαδοχικά δείγματα
    /// |x|≥0.999, ανά κανάλι, αθροισμένα). Ορισμός και όρια:
    /// `sp314_dsp::analysis::clipping`.
    pub output_clips_detected: u32,
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

    // F-085 wiring: ο ΥΠΑΡΧΩΝ streaming accumulator, ταϊσμένος με το
    // mono downmix του ΠΑΡΑΔΟΤΕΟΥ — ίδιο μοτίβο με το trunk pass
    // (trunk_pass.rs:512 `dynamics.feed_chunk(m)`), άλλο σήμα.
    // Δίνει rms_db ΚΑΙ dyn_range_db (p95−p5 των per-block RMS) σε ένα
    // πέρασμα: κρατάει ένα f32 ανά block, όχι ανά δείγμα.
    let mut dynamics = sp314_dsp::analysis::dynamics::StreamingDynamicsAnalyzer::new(spec.sample_rate);
    // Συσχέτιση: η ΜΑΘΗΜΑΤΙΚΗ του trunk_pass.rs:524-526/772-777,
    // αντιγραμμένη επειδή η `stereo::stereo_correlation` θέλει
    // ΟΛΟΚΛΗΡΑ τα κανάλια στη μνήμη — αυτό το πέρασμα είναι O(1) RAM
    // κατά σχεδίαση και δεν τα έχει.
    let mut corr_cross = 0f64;
    let mut corr_sum_l = 0f64;
    let mut corr_sum_r = 0f64;
    // F-085 wiring (φασματικά): ο ΥΠΑΡΧΩΝ streaming analyzer
    // (spectral.rs:220), ταϊσμένος με το ΙΔΙΟ mono downmix του
    // παραδοτέου. Τα per-frame μαθηματικά του είναι ταυτόσημα με τις
    // batch `spectral_centroid_hz`/`spectral_flatness` — η ισοδυναμία
    // είναι καρφωμένη με `assert_eq!` στα streaming_tests του ίδιου
    // αρχείου, ΟΧΙ με ανοχή. Κρατάει έναν STFT encoder, όχι το σήμα.
    let mut spectral =
        sp314_dsp::analysis::spectral::StreamingSpectralAnalyzer::new(spec.sample_rate);
    // F-085 wiring: γεγονότα clipping του ΠΑΡΑΔΟΤΕΟΥ. Το struct
    // κρατάει το μήκος ριπής ανά κανάλι, ώστε ριπή που διασχίζει όριο
    // chunk να μετράει ΜΙΑ φορά (test στο ίδιο module).
    let mut clips = sp314_dsp::analysis::clipping::ClipEventCounter::new();

    let mut interleaved: Vec<f32> = Vec::with_capacity(CHUNK_FRAMES * channels);
    let mut left_buf = vec![0f32; CHUNK_FRAMES];
    let mut right_buf = vec![0f32; CHUNK_FRAMES];
    let mut mono_buf = vec![0f32; CHUNK_FRAMES];
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
        // F-085: ίδιο πέρασμα, ίδια δείγματα — mono downmix για τα
        // dynamics (όπως trunk_pass.rs:504), σταυρωτά αθροίσματα για
        // τη συσχέτιση. f64 συσσώρευση: σε 8ωρο audiobook τα Σ(L²)
        // ξεπερνούν το χρήσιμο εύρος του f32.
        for i in 0..frames {
            mono_buf[i] = (left_buf[i] + right_buf[i]) * 0.5;
            corr_cross += (left_buf[i] as f64) * (right_buf[i] as f64);
            corr_sum_l += (left_buf[i] as f64) * (left_buf[i] as f64);
            corr_sum_r += (right_buf[i] as f64) * (right_buf[i] as f64);
        }
        dynamics.feed_chunk(&mono_buf[..frames]);
        spectral.feed_chunk(&mono_buf[..frames]);
        clips.feed_chunk(&left_buf[..frames], &right_buf[..frames]);
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

    // F-085: ίδια μαθηματική με trunk_pass.rs:772-777 — denom-guard
    // πρώτα, μετά clamp. Σιωπή/mono ⇒ 1.0, όπως εκεί.
    let denom = (corr_sum_l * corr_sum_r).sqrt();
    let output_stereo_correlation = if denom < 1e-10 {
        1.0
    } else {
        (corr_cross / denom).clamp(-1.0, 1.0) as f32
    };
    let dyn_result = dynamics.finish();
    // F-085: το width είναι ΠΑΡΑΓΩΓΟ — ο ίδιος τύπος με τη
    // `stereo::stereo_width` (`1.0 - |corr|`), πάνω στη συσχέτιση που
    // μόλις μετρήθηκε. ΔΕΝ ξαναδιαβάζει το σήμα.
    let output_stereo_width = 1.0 - output_stereo_correlation.abs();
    let (output_spectral_centroid, output_spectral_flatness, _crest) = spectral.finish();

    Ok(MeasuredOutput {
        frames_written,
        sample_rate: spec.sample_rate,
        output_lufs,
        output_lra,
        true_peak_dbtp,
        pcm_blake3: blake3.finalize().to_hex().to_string(),
        output_sha256: format!("{:x}", sha256.finalize()),
        output_stereo_correlation,
        output_dynamic_range_db: dyn_result.dyn_range_db,
        output_rms_db: dyn_result.rms_db,
        output_stereo_width,
        output_spectral_centroid,
        output_spectral_flatness,
        output_clips_detected: clips.finish(),
    })
}

/// F-085: τα ΕΞΙ μετρημένα πεδία του quality block, από ένα ΗΔΗ
/// γραμμένο raw master (interleaved f32 native-endian, 2ch).
/// Επιστρέφει (correlation, dyn_range_db, rms_db, width,
/// spectral_centroid, spectral_flatness, clips_detected).
///
/// Για τον Episode/offline caller (`dsp_pipeline`), που δεν περνάει
/// από το `wav_to_raw_measured` και του οποίου το `EpisodeRenderResult`
/// δεν κουβαλάει correlation/DR/RMS. Διαβάζει το ΙΔΙΟ αρχείο που
/// περιγράφει το certificate (`render_res.pcm_path`) — το ίδιο που
/// mmap-άρει και ο FLAC encoder λίγο παρακάτω. Chunked: O(1) RAM.
pub fn measure_raw_master(
    raw_path: &std::path::Path,
    sample_rate: u32,
) -> Result<(f32, f32, f32, f32, f32, f32, u32), String> {
    use std::io::Read;
    let file = std::fs::File::open(raw_path).map_err(|e| format!("open raw master: {e}"))?;
    let mut reader = std::io::BufReader::new(file);

    let mut dynamics = sp314_dsp::analysis::dynamics::StreamingDynamicsAnalyzer::new(sample_rate);
    let mut spectral =
        sp314_dsp::analysis::spectral::StreamingSpectralAnalyzer::new(sample_rate);
    let mut clips = sp314_dsp::analysis::clipping::ClipEventCounter::new();
    let mut corr_cross = 0f64;
    let mut corr_sum_l = 0f64;
    let mut corr_sum_r = 0f64;

    // 2ch × f32 = 8 bytes/frame
    let mut byte_buf = vec![0u8; CHUNK_FRAMES * 8];
    let mut mono_buf = vec![0f32; CHUNK_FRAMES];
    // F-085: το clipping μετριέται ΑΝΑ ΚΑΝΑΛΙ, όχι στο mono downmix —
    // δύο κομμένα κανάλια σε αντίθεση θα αλληλοακυρώνονταν στο mono.
    let mut left_buf = vec![0f32; CHUNK_FRAMES];
    let mut right_buf = vec![0f32; CHUNK_FRAMES];
    loop {
        let mut filled = 0usize;
        while filled < byte_buf.len() {
            let n = reader
                .read(&mut byte_buf[filled..])
                .map_err(|e| format!("read raw master: {e}"))?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        if filled < 8 {
            break;
        }
        let frames = filled / 8;
        for i in 0..frames {
            let o = i * 8;
            let l = f32::from_ne_bytes([
                byte_buf[o],
                byte_buf[o + 1],
                byte_buf[o + 2],
                byte_buf[o + 3],
            ]);
            let r = f32::from_ne_bytes([
                byte_buf[o + 4],
                byte_buf[o + 5],
                byte_buf[o + 6],
                byte_buf[o + 7],
            ]);
            mono_buf[i] = (l + r) * 0.5;
            left_buf[i] = l;
            right_buf[i] = r;
            corr_cross += (l as f64) * (r as f64);
            corr_sum_l += (l as f64) * (l as f64);
            corr_sum_r += (r as f64) * (r as f64);
        }
        dynamics.feed_chunk(&mono_buf[..frames]);
        spectral.feed_chunk(&mono_buf[..frames]);
        clips.feed_chunk(&left_buf[..frames], &right_buf[..frames]);
        if filled < byte_buf.len() {
            break;
        }
    }

    let denom = (corr_sum_l * corr_sum_r).sqrt();
    let correlation = if denom < 1e-10 {
        1.0
    } else {
        (corr_cross / denom).clamp(-1.0, 1.0) as f32
    };
    let d = dynamics.finish();
    // ΠΑΡΑΓΩΓΟ, ίδιος τύπος με stereo::stereo_width — δεν ξαναδιαβάζει.
    let width = 1.0 - correlation.abs();
    let (centroid, flatness, _crest) = spectral.finish();
    Ok((
        correlation,
        d.dyn_range_db,
        d.rms_db,
        width,
        centroid,
        flatness,
        clips.finish(),
    ))
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
