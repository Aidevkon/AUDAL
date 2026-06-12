//! Streaming WAV I/O — Memory-Aware DSP Foundation
//! Authority: Creator OS Architecture v1.0
//! Purpose: Read/write WAV in chunks — never load full file into RAM
//! Status: Foundation only — Two-Pass pipeline wiring in v3.0
//!
//! Memory profile:
//!   2h podcast @48kHz stereo f32 = 2.7GB full load
//!   With chunked I/O: ~2MB constant regardless of file size

use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use std::fs::File;
use std::io::{BufReader, BufWriter, Seek};

/// Reads a WAV file in fixed-size chunks.
/// Never loads the full file into RAM.
pub struct WavChunkReader {
    reader: WavReader<BufReader<File>>,
    sample_rate: u32,
    channels: u16,
}

impl WavChunkReader {
    pub fn open(path: &str) -> Result<Self, hound::Error> {
        let reader = WavReader::open(path)?;
        let spec = reader.spec();
        Ok(Self {
            sample_rate: spec.sample_rate,
            channels: spec.channels,
            reader,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Read next `chunk_frames` stereo frames → interleaved f32.
    /// Returns None at EOF.
    pub fn next_chunk(&mut self, chunk_frames: usize) -> Option<Vec<f32>> {
        let n_samples = chunk_frames * self.channels as usize;
        let mut buf = Vec::with_capacity(n_samples);

        let spec = self.reader.spec();
        match spec.sample_format {
            SampleFormat::Float => {
                for sample in self.reader.samples::<f32>().take(n_samples) {
                    match sample {
                        Ok(s) => buf.push(s),
                        Err(_) => break,
                    }
                }
            }
            SampleFormat::Int => {
                let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
                for sample in self.reader.samples::<i32>().take(n_samples) {
                    match sample {
                        Ok(s) => buf.push(s as f32 / max_val),
                        Err(_) => break,
                    }
                }
            }
        }

        if buf.is_empty() {
            None
        } else {
            Some(buf)
        }
    }

    /// Total sample count (all channels).
    pub fn total_samples(&self) -> u32 {
        self.reader.len()
    }

    /// Duration in seconds.
    pub fn duration_secs(&self) -> f32 {
        self.reader.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
    }

    /// Seek to position in milliseconds.
    /// O(1) disk seek — zero RAM allocation.
    /// INV-ST-3: peak RAM ≤ 5MB regardless of seek position.
    pub fn seek_ms(&mut self, position_ms: u64) -> Result<(), String> {
        let sample_offset = (position_ms as f64
            * self.sample_rate as f64
            * self.channels as f64
            / 1000.0) as u32;
        self.seek_samples(sample_offset)
    }

    /// Seek to absolute sample position (all channels).
    pub fn seek_samples(&mut self, sample_pos: u32) -> Result<(), String> {
        self.reader.seek(sample_pos)
            .map_err(|e| format!("WavChunkReader seek failed: {e}"))
    }
}

/// Writes mastered audio in chunks directly to disk.
/// Memory usage: constant ~2MB regardless of file length.
pub struct WavChunkWriter {
    writer: WavWriter<BufWriter<File>>,
}

impl WavChunkWriter {
    pub fn create(path: &str, sample_rate: u32, channels: u16) -> Result<Self, hound::Error> {
        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        Ok(Self {
            writer: WavWriter::create(path, spec)?,
        })
    }

    /// Write interleaved f32 chunk directly to disk.
    pub fn write_chunk(&mut self, data: &[f32]) -> Result<(), hound::Error> {
        for &s in data {
            self.writer.write_sample(s)?;
        }
        Ok(())
    }

    /// Flush and finalize the WAV file.
    pub fn finalize(self) -> Result<(), hound::Error> {
        self.writer.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_seek_ms_positions_correctly() {
        let path = "/tmp/test_stream_seek.wav";
        // 2 seconds stereo @ 48kHz = 192000 samples
        let samples: Vec<f32> = (0..192000)
            .map(|i| i as f32 / 192000.0)
            .collect();
        let mut writer = WavChunkWriter::create(path, 48000, 2).unwrap();
        writer.write_chunk(&samples).unwrap();
        writer.finalize().unwrap();

        let mut reader = WavChunkReader::open(path).unwrap();
        // Seek to 1000ms = 96000 samples (halfway)
        reader.seek_ms(1000).unwrap();
        let chunk = reader.next_chunk(1).unwrap();
        // At 1s position, ramp value ~0.5
        let expected = 96000.0f32 / 192000.0;
        assert!(
            (chunk[0] - expected).abs() < 0.01,
            "seek_ms(1000): expected ~{:.3}, got {:.3}", expected, chunk[0]
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_stream_from_seek_mid() {
        let path = "/tmp/test_stream_seek_mid.wav";
        let samples: Vec<f32> = vec![0.1f32; 96000];
        let mut writer = WavChunkWriter::create(path, 48000, 2).unwrap();
        writer.write_chunk(&samples).unwrap();
        writer.finalize().unwrap();

        let mut reader = WavChunkReader::open(path).unwrap();
        let _ = reader.next_chunk(24000); // read halfway
        reader.seek_samples(0).unwrap();  // seek back to start
        let chunk = reader.next_chunk(1).unwrap();
        assert!((chunk[0] - 0.1).abs() < 0.001);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn write_then_read_chunk_roundtrip() {
        let path = "/tmp/test_stream_roundtrip.wav";
        let sample_rate = 48000u32;
        let channels = 2u16;
        let chunk: Vec<f32> = (0..96000).map(|i| (i as f32 * 0.001).sin()).collect();

        // Write
        let mut writer = WavChunkWriter::create(path, sample_rate, channels).expect("write create");
        writer.write_chunk(&chunk).expect("write chunk");
        writer.finalize().expect("finalize");

        // Read back
        let mut reader = WavChunkReader::open(path).expect("read open");
        assert_eq!(reader.sample_rate(), sample_rate);
        assert_eq!(reader.channels(), channels);

        let read_chunk = reader.next_chunk(48000).expect("read chunk");
        assert_eq!(read_chunk.len(), chunk.len());

        // Verify roundtrip fidelity
        let mse: f32 = chunk
            .iter()
            .zip(read_chunk.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            / chunk.len() as f32;
        assert!(mse < 1e-10, "Roundtrip MSE={:.2e}", mse);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn next_chunk_returns_none_at_eof() {
        let path = "/tmp/test_stream_eof.wav";
        let chunk: Vec<f32> = vec![0.1f32; 1024];
        let mut writer = WavChunkWriter::create(path, 48000, 2).unwrap();
        writer.write_chunk(&chunk).unwrap();
        writer.finalize().unwrap();

        let mut reader = WavChunkReader::open(path).unwrap();
        let _ = reader.next_chunk(512); // read all
        let eof = reader.next_chunk(512);
        assert!(eof.is_none(), "Expected None at EOF");

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn duration_secs_correct() {
        let path = "/tmp/test_stream_duration.wav";
        // 1 second of stereo audio = 48000 * 2 = 96000 samples
        let chunk: Vec<f32> = vec![0.0f32; 96000];
        let mut writer = WavChunkWriter::create(path, 48000, 2).unwrap();
        writer.write_chunk(&chunk).unwrap();
        writer.finalize().unwrap();

        let reader = WavChunkReader::open(path).unwrap();
        let dur = reader.duration_secs();
        assert!((dur - 1.0).abs() < 0.01, "Expected ~1s, got {}", dur);

        std::fs::remove_file(path).ok();
    }
}
