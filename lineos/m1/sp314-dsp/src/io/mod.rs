// src/io/mod.rs

pub mod wav_reader;
pub mod wav_writer;
pub mod flac_writer;
pub use wav_reader::{WavReader, DecodedAudio};
pub use wav_writer::WavWriter;
pub use flac_writer::FlacWriter;
pub mod stream;
