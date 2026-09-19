use sp314_dsp::io::flac_encode::flac_encode;
use std::path::Path;

/// Encode interleaved f32 (any channel count) to 24-bit FLAC.
/// Quantization is deterministic round-half-even, NO dither — see
/// F-050 tier-2 design: persisted bytes must be reproducible for
/// hash-pinned verification; at 24 bits the quantization floor
/// (-144 dBFS) is beneath every measurement this engine makes.
/// Clamps to [-1.0, 1.0] before scaling (NaN -> 0, documented).
pub fn encode_f32_flac_24(
    interleaved: &[f32],
    sample_rate: u32,
    channels: u16,
    path: &Path,
) -> Result<(), String> {
    let bytes = flac_encode(interleaved, sample_rate, channels as u32)
        .map_err(|e| format!("FLAC encode error: {}", e))?;

    // W17: το flacenc 0.3.1 παράγει 123× bloat σε
    // συγκεκριμένα σήματα. Trigger στα ~80s και εξαρτάται
    // από ΣΥΣΣΩΡΕΥΜΕΝΟ STATE, όχι από το σήμα.
    // Το libsndfile δίνει 10.4MB για το ίδιο σήμα που
    // εδώ βγάζει 1.29GB.
    // ΠΡΟΣΟΧΗ: το FLAC έχει σταθερό overhead (STREAMINFO
    // + frame headers) που κυριαρχεί σε μικρά αρχεία —
    // 5 δείγματα δίνουν νόμιμα 1160 bytes. Ο έλεγχος
    // ισχύει μόνο πάνω από ένα κατώφλι μεγέθους.
    const FLAC_GUARD_MIN_SAMPLES: usize = 48_000; // ~0.5s stereo
    let raw_size = interleaved.len() * 3; // 24-bit
    if interleaved.len() >= FLAC_GUARD_MIN_SAMPLES
        && bytes.len() > raw_size
    {
        return Err(format!(
            "FLAC encoder produced {} bytes for {} samples \
             ({}x raw PCM) — refusing to write. flacenc bloat bug.",
            bytes.len(),
            interleaved.len(),
            bytes.len() / raw_size.max(1)
        ));
    }

    // W17 DIAGNOSTIC — TEMPORARY
    eprintln!(
        "[W17-FLAC] input_samples={} output_bytes={} ratio={:.4} path={}",
        interleaved.len(),
        bytes.len(),
        bytes.len() as f64 / (interleaved.len() as f64 * 3.0),  // vs 24-bit PCM
        path.display()
    );

    std::fs::write(path, bytes).map_err(|e| format!("Failed to write FLAC file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_raw_interleaved;
    use tempfile::TempDir;

    #[test]
    fn test_encode_decode_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test_roundtrip.flac");

        let sr = 48000;
        let ch = 2;
        let total_frames = sr * 2; // 2 seconds
        let mut original = Vec::with_capacity(total_frames * 2);
        let freq = 440.0;
        let amp = 10.0_f32.powf(-20.0 / 20.0);

        for i in 0..total_frames {
            let t = i as f32 / sr as f32;
            let v = (2.0 * std::f32::consts::PI * freq * t).sin() * amp;
            original.push(v);
            original.push(v);
        }

        // Test encode
        encode_f32_flac_24(&original, sr as u32, ch, &path).unwrap();

        // Test decode
        let (decoded, decoded_sr, decoded_ch) =
            decode_raw_interleaved(path.to_str().unwrap()).unwrap();
        assert_eq!(decoded_sr, sr as u32);
        assert_eq!(decoded_ch, ch);
        // flacenc pads to block_size, so decoded may be longer. Truncate to original.
        let decoded = &decoded[..original.len()];
        assert_eq!(decoded.len(), original.len());

        let mut max_abs_diff = 0.0_f32;
        for (o, d) in original.iter().zip(decoded.iter()) {
            let diff = (o - d).abs();
            if diff > max_abs_diff {
                max_abs_diff = diff;
            }
        }

        let allowed = 2.0_f32.powi(-22); // ~2.38e-7
        assert!(
            max_abs_diff < allowed,
            "Diff {} exceeds allowed {}",
            max_abs_diff,
            allowed
        );
    }

    #[test]
    fn test_nan_handling() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test_nan.flac");
        let data = vec![
            std::f32::NAN,
            0.5,
            std::f32::INFINITY,
            -1.5,
            std::f32::NEG_INFINITY,
        ];

        encode_f32_flac_24(&data, 48000, 1, &path).unwrap();

        let (decoded, _, _) = decode_raw_interleaved(path.to_str().unwrap()).unwrap();

        // flacenc pads to block_size
        let decoded = &decoded[..data.len()];
        assert_eq!(decoded.len(), data.len());
        // NAN -> 0.0
        assert_eq!(decoded[0], 0.0);
        // 0.5 -> 0.5
        assert!((decoded[1] - 0.5).abs() < 1e-6);
        // INF -> 1.0
        assert!((decoded[2] - 1.0).abs() < 1e-6);
        // -1.5 -> -1.0
        assert!((decoded[3] - (-1.0)).abs() < 1e-6);
        // -INF -> -1.0
        assert!((decoded[4] - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_encode_determinism() {
        let tmp = TempDir::new().unwrap();
        let path1 = tmp.path().join("test_det1.flac");
        let path2 = tmp.path().join("test_det2.flac");

        let sr = 48000;
        let mut data = Vec::with_capacity(sr * 2);
        for i in 0..sr {
            let v = (i as f32).sin();
            data.push(v);
            data.push(v);
        }

        encode_f32_flac_24(&data, sr as u32, 2, &path1).unwrap();
        encode_f32_flac_24(&data, sr as u32, 2, &path2).unwrap();

        let bytes1 = std::fs::read(&path1).unwrap();
        let bytes2 = std::fs::read(&path2).unwrap();

        assert_eq!(bytes1, bytes2, "Files should be byte-identical");
    }
}
