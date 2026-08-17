// src/io/flac_writer.rs
// f32 samples → FLAC encode.
// Uses unified flac_encode home.
// No DSP logic here — pure I/O.

use crate::io::flac_encode::flac_encode;
use std::io::Write;

/// FLAC writer.
/// Converts f32 audio to 24-bit lossless FLAC.
pub struct FlacWriter;

impl FlacWriter {
    /// Write stereo f32 sample buffers to a 24-bit FLAC file.
    /// 24-bit depth — lossless, full precision, smaller than 32-bit float WAV.
    /// Interleaves left and right into [L, R, L, R, ...].
    /// Returns Err on I/O failure.
    pub fn write(
        path: &str,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if left.len() != right.len() {
            return Err("Left and right channels must have the same length".into());
        }

        let mut interleaved = Vec::with_capacity(left.len() * 2);
        for i in 0..left.len() {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }

        let flac_bytes = flac_encode(&interleaved, sample_rate, 2)?;

        let mut file = std::fs::File::create(path)?;
        file.write_all(&flac_bytes)?;

        Ok(())
    }
}
