use std::fmt;

pub enum DecodeChunk<'a> {
    /// Bounded-size interleaved stereo samples (≤ some max chunk size, NOT
    /// necessarily aligned to DspGraph's block_size — that alignment happens
    /// downstream, this layer just avoids whole-file accumulation).
    Samples(&'a [f32]),
    EndOfStream,
}

// Ο ορισμός μετακόμισε στο lineos-types στις 20/09 ώστε το conformance να τον βλέπει χωρίς να εξαρτάται από τον πυρήνα.
pub use lineos_types::decode::DecodeError;

#[derive(Debug)]
pub enum LazyReaderError {
    NoSupportedTrack,
    Symphonia(String),
    MissingSampleRate,
    InvalidBufferLength { len: usize, channels: usize },
}

impl fmt::Display for LazyReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSupportedTrack => write!(f, "no supported audio track found"),
            Self::Symphonia(e) => write!(f, "symphonia error: {e}"),
            Self::MissingSampleRate => write!(f, "missing sample rate"),
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
