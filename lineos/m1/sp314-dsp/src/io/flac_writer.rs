// src/io/flac_writer.rs
// f32 samples → FLAC encode.
// Uses flacenc crate — Sony's pure Rust FLAC encoder (Apache-2.0).
// No DSP logic here — pure I/O.

use flacenc::component::BitRepr;

/// FLAC writer using `flacenc`.
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
            let l_i32 = (left[i] * 8388607.0_f32).clamp(-8388608.0, 8388607.0) as i32;
            let r_i32 = (right[i] * 8388607.0_f32).clamp(-8388608.0, 8388607.0) as i32;
            interleaved.push(l_i32);
            interleaved.push(r_i32);
        }

        let channels = 2;
        let bits_per_sample = 24;

        let source = flacenc::source::MemSource::from_samples(
            &interleaved,
            channels,
            bits_per_sample,
            sample_rate as usize,
        );

        let flac_stream = flacenc::encode_with_fixed_block_size(
            &flacenc::config::Encoder::default(),
            source,
            flacenc::config::Encoder::default().block_sizes[0],
        )
        .map_err(|e| format!("FLAC encoding error: {:?}", e))?;

        let mut file = std::fs::File::create(path)?;

        let mut sink = flacenc::bitsink::ByteSink::new();
        flac_stream
            .write(&mut sink)
            .map_err(|e| format!("FLAC write error: {:?}", e))?;

        use std::io::Write;
        file.write_all(sink.as_slice())?;

        Ok(())
    }
}
