use std::fmt;

#[derive(Debug)]
pub enum DecodeError {
    FileNotFound(String),
    FileTooLarge(u64),
    DurationExceeded(u64),
    UnsupportedFormat(String),
    DecodeFailure(String),
    ResampleFailure(String),
    ConsumerError(String),
    MissingSampleRate,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::FileNotFound(p) => write!(f, "File not found: {p}"),
            DecodeError::FileTooLarge(sz) => {
                write!(f, "File too large ({} MB > 500MB limit)", sz / 1024 / 1024)
            }
            DecodeError::DurationExceeded(s) => write!(f, "Audio too long ({s}s > 12 min limit)"),
            DecodeError::UnsupportedFormat(e) => write!(f, "Unsupported format: {e}"),
            DecodeError::DecodeFailure(e) => write!(f, "Decode failure: {e}"),
            DecodeError::ResampleFailure(e) => write!(f, "Resample failure: {e}"),
            DecodeError::ConsumerError(e) => write!(f, "Consumer error: {e}"),
            DecodeError::MissingSampleRate => write!(f, "Missing sample rate"),
        }
    }
}
