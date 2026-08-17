use std::io::Cursor;
use flac_codec::encode::{FlacSampleWriter, Options};

pub const FLAC_BITS_PER_SAMPLE: u32 = 24;

#[derive(Debug)]
pub enum FlacEncodeError {
    EmptyInput,
    CodecError(String),
}

impl std::fmt::Display for FlacEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlacEncodeError::EmptyInput => write!(f, "Cannot encode empty FLAC"),
            FlacEncodeError::CodecError(e) => write!(f, "FLAC codec error: {}", e),
        }
    }
}

impl std::error::Error for FlacEncodeError {}

/// Encodes interleaved f32 PCM to a FLAC bytestream.
pub fn flac_encode(pcm: &[f32], sample_rate: u32, channels: u32) -> Result<Vec<u8>, FlacEncodeError> {
    if pcm.is_empty() {
        return Err(FlacEncodeError::EmptyInput);
    }

    // Explicit quantization to 24-bit with round_ties_even
    let mut quantized = Vec::with_capacity(pcm.len());
    let max_val = 8388607.0_f32; // 24-bit max (2^23 - 1)

    for &s in pcm {
        let s_clamped = if s.is_nan() { 0.0 } else { s.clamp(-1.0, 1.0) };
        let q = (s_clamped * max_val).round_ties_even() as i32;
        quantized.push(q);
    }

    let mut cursor = Cursor::new(Vec::new());

    let options = Options::default().block_size(4096).unwrap();

    let mut encoder = FlacSampleWriter::new(&mut cursor, options, sample_rate, FLAC_BITS_PER_SAMPLE, channels as u8, None)
        .map_err(|e: flac_codec::Error| FlacEncodeError::CodecError(e.to_string()))?;
        
    encoder.write(&quantized)
        .map_err(|e: flac_codec::Error| FlacEncodeError::CodecError(e.to_string()))?;

    encoder.finalize()
        .map_err(|e: flac_codec::Error| FlacEncodeError::CodecError(e.to_string()))?;

    Ok(cursor.into_inner())
}
