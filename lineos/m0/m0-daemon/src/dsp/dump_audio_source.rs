//! DumpAudioSource — AudioSource over a raw PCM dump.
//!
//! Re-shaped DumpDecodeProvider pattern for the AudioSource
//! trait. The dump was written by StandardizedStreamCore
//! through set_tap: interleaved f32 LE, 48 kHz, stereo, by
//! construction. No header, no metadata — pure samples.

use crate::dsp::audio_source::AudioSource;
use std::io::{BufReader, Read};
use std::path::Path;

/// Streaming AudioSource backed by a raw PCM dump file.
pub struct DumpAudioSource {
    reader: BufReader<std::fs::File>,
    total_frames: u64,
    /// Reusable byte buffer — avoids per-call allocation.
    byte_buf: Vec<u8>,
}

impl DumpAudioSource {
    /// Open a raw dump for streaming playback.
    /// Hard error on `len % 8 != 0` (truncated dump).
    pub fn open(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("DumpAudioSource open: {}", e))?;
        let len = file
            .metadata()
            .map_err(|e| format!("DumpAudioSource metadata: {}", e))?
            .len();
        if len % 8 != 0 {
            return Err(format!(
                "DumpAudioSource: raw dump has partial frame \
                 — truncated/corrupt dump (len={})",
                len
            ));
        }
        Ok(Self {
            reader: BufReader::new(file),
            total_frames: len / 8, // 2 channels × 4 bytes
            byte_buf: Vec::new(),
        })
    }
}

impl AudioSource for DumpAudioSource {
    /// Always 48 kHz — written by StandardizedStreamCore's
    /// set_tap, which outputs sanitized 48k stereo by
    /// construction (the core tap contract, Y3-iii-a).
    fn sample_rate(&self) -> u32 {
        48_000
    }

    /// Always stereo — same contract as above.
    fn channels(&self) -> usize {
        2
    }

    fn total_frames_hint(&self) -> Option<u64> {
        Some(self.total_frames)
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let bytes_needed = buffer.len() * 4;
        if self.byte_buf.len() < bytes_needed {
            self.byte_buf.resize(bytes_needed, 0);
        }

        let mut total_read = 0;
        while total_read < bytes_needed {
            let n = self
                .reader
                .read(&mut self.byte_buf[total_read..bytes_needed])
                .map_err(|e| format!("DumpAudioSource read: {}", e))?;
            if n == 0 {
                break;
            }
            total_read += n;
        }

        // Truncate to frame boundary (defensive).
        let valid_bytes = total_read - (total_read % 8);
        let frames = valid_bytes / 8;

        // Decode f32 LE — explicit from_le_bytes, no pointer
        // casts (matches read_scout_from_raw_dump's pattern).
        for (i, chunk) in self.byte_buf[..valid_bytes].chunks_exact(4).enumerate() {
            buffer[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        Ok(frames)
    }
}
