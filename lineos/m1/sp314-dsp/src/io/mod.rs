// src/io/mod.rs

pub mod flac_encode;
pub mod flac_writer;
pub mod wav_reader;
pub mod wav_writer;
pub use flac_writer::FlacWriter;
pub use wav_reader::{DecodedAudio, WavReader};
pub use wav_writer::WavWriter;
pub mod decode_types;
pub mod stream;
