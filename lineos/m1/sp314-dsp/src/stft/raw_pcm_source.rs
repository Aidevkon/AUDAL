//! RawPcmFileSource — reads interleaved f32 directly from disk.
//! Matches the raw dump format written by decode_node.rs.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::stft::sliding_overlap_reader::ChunkSource;

pub struct RawPcmFileSource {
    reader: BufReader<File>,
    byte_buf: Vec<u8>,
    /// Interleaved channel count (2 for stereo dumps, 6 for 5.1).
    /// The raw dump is headerless, so the caller must specify it.
    n_channels: usize,
}

impl RawPcmFileSource {
    pub fn new(path: &Path, n_channels: usize) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        Ok(Self {
            reader: BufReader::new(file),
            byte_buf: Vec::new(),
            n_channels,
        })
    }

    pub fn read_window(&self, start_frame: usize, len_frames: usize) -> Vec<f32> {
        use std::io::{Read, Seek, SeekFrom};
        let mut mono = Vec::new();

        if let Ok(mut cloned_file) = self.reader.get_ref().try_clone() {
            let channels = self.n_channels;
            let byte_offset = (start_frame * channels * 4) as u64;

            if cloned_file.seek(SeekFrom::Start(byte_offset)).is_err() {
                return mono;
            }

            let bytes_to_read = len_frames * channels * 4;
            let mut byte_buf = vec![0u8; bytes_to_read];

            let mut total_read = 0;
            while total_read < bytes_to_read {
                match cloned_file.read(&mut byte_buf[total_read..]) {
                    Ok(0) => break,
                    Ok(n) => total_read += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }

            let actual_frames = total_read / (4 * channels);
            mono.reserve_exact(actual_frames);

            for frame in 0..actual_frames {
                let mut sum = 0.0;
                for c in 0..channels {
                    let idx = (frame * channels + c) * 4;
                    let val = f32::from_le_bytes([
                        byte_buf[idx],
                        byte_buf[idx + 1],
                        byte_buf[idx + 2],
                        byte_buf[idx + 3],
                    ]);
                    sum += val;
                }
                mono.push(sum / channels as f32);
            }
        }
        mono
    }
}

impl ChunkSource for RawPcmFileSource {
    fn channels(&self) -> usize {
        self.n_channels
    }

    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let channels = self.channels();
        if !buffer.len().is_multiple_of(channels) {
            return Err(format!(
                "buffer length {} is not a multiple of {} channels",
                buffer.len(),
                channels
            ));
        }

        if buffer.is_empty() {
            return Ok(0);
        }

        // Y3-iv-a: explicit LE decode — the dump is f32 LE by the write contract (core tap / TappedDecoder); the
        // old native-endian pointer cast was correct only on LE hosts (latent-BE register item since P0-b).
        let byte_len = buffer.len() * 4;
        self.byte_buf.resize(byte_len, 0);

        // Read up to byte_len bytes
        let mut total_read = 0;
        while total_read < byte_len {
            match self.reader.read(&mut self.byte_buf[total_read..]) {
                Ok(0) => break, // EOF
                Ok(n) => total_read += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("Read error: {e}")),
            }
        }

        let frames_read = total_read / 4 / channels;
        let valid_bytes = frames_read * channels * 4;

        for (i, chunk) in self.byte_buf[..valid_bytes].chunks_exact(4).enumerate() {
            buffer[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }

        // Return number of frames read
        Ok(frames_read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_test_pattern(samples: &[f32]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        let raw_bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 4) };
        file.write_all(raw_bytes).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn raw_pcm_oracle_test() {
        // Known distinct values
        let original_samples: Vec<f32> = (0..1000).map(|i| i as f32 * 0.1).collect();
        let file = write_test_pattern(&original_samples);

        // Test with various chunk sizes (some aligned, some not).
        // This test uses n_ch=2; buffer lengths must be multiples of 2.
        let n_ch = 2usize;
        let chunk_sizes = vec![2, 4, 10, 100, 1000, 2000, 3];

        for &chunk_size in &chunk_sizes {
            let mut source = RawPcmFileSource::new(file.path(), n_ch).unwrap();
            let mut all_read = Vec::new();

            // Adjust chunk size if it's not a multiple of channels
            let buf_size = if chunk_size % n_ch != 0 {
                chunk_size + 1
            } else {
                chunk_size
            };
            let mut buf = vec![0.0_f32; buf_size];

            loop {
                let frames = source.fill_buffer(&mut buf).unwrap();
                if frames == 0 {
                    break;
                }
                all_read.extend_from_slice(&buf[..frames * n_ch]);
            }

            assert_eq!(
                all_read.len(),
                original_samples.len(),
                "Chunk size {} returned wrong total length",
                chunk_size
            );

            // Bit-pattern equality
            for (i, (read_val, expected_val)) in
                all_read.iter().zip(original_samples.iter()).enumerate()
            {
                assert_eq!(
                    read_val.to_bits(),
                    expected_val.to_bits(),
                    "Mismatch at index {} with chunk size {}",
                    i,
                    chunk_size
                );
            }
        }
    }

    #[test]
    fn fill_buffer_eof_behavior() {
        let samples = vec![1.0, 2.0]; // 1 frame, 2ch
        let file = write_test_pattern(&samples);
        let mut source = RawPcmFileSource::new(file.path(), 2).unwrap();

        let mut buf = vec![0.0; 2];
        let frames = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames, 1);

        // At EOF, should return 0
        let frames_eof = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames_eof, 0);

        // Repeated calls after EOF should remain 0, no panic or error
        let frames_eof2 = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames_eof2, 0);
    }

    #[test]
    fn fill_buffer_invalid_size_returns_error() {
        let samples = vec![1.0, 2.0]; // 1 frame, 2ch
        let file = write_test_pattern(&samples);
        let mut source = RawPcmFileSource::new(file.path(), 2).unwrap();

        let mut buf = vec![0.0; 3]; // Not a multiple of channels (2)
        let result = source.fill_buffer(&mut buf);

        assert!(result.is_err(), "Expected an error for invalid buffer size");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("is not a multiple of 2 channels"),
            "Unexpected error message: {}",
            err_msg
        );
    }

    #[test]
    fn test_streaming_bit_identity_oracle_real_file() {
        use crate::stft::sliding_overlap_reader::SlidingOverlapReader;
        use crate::stft::two_pass::TwoPassEngine;
        use std::io::Write;

        let n_total = 200_000;

        let left: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 440.0 + (500.0 * t);
                (2.0 * core::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let right: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 880.0 + (1000.0 * t);
                (2.0 * core::f32::consts::PI * freq * t).sin()
            })
            .collect();

        // 1. Write exact byte pattern that decode_node.rs uses:
        // interleaved f32.to_le_bytes(), L then R.
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        for i in 0..n_total {
            temp_file.write_all(&left[i].to_le_bytes()).unwrap();
            temp_file.write_all(&right[i].to_le_bytes()).unwrap();
        }
        let temp_path = temp_file.into_temp_path();

        let signal: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        // 2. OLD PATH (Slice-based)
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, 48000, None, None, false);

        let mut old_voice = Vec::new();
        let mut old_drums = Vec::new();
        let mut old_bass = Vec::new();
        let mut old_harmonics = Vec::new();
        let mut old_ambience = Vec::new();

        engine
            .process_slices_with_params(&signal, &left, &right, &scout, 1.0, false, |chunk| {
                old_voice.extend_from_slice(&chunk.voice);
                old_drums.extend_from_slice(&chunk.drums);
                old_bass.extend_from_slice(&chunk.bass);
                old_harmonics.extend_from_slice(&chunk.harmonics);
                old_ambience.extend_from_slice(&chunk.ambience);
            })
            .unwrap();

        // 3. NEW PATH (Streaming with real I/O)
        let mut e_new = TwoPassEngine::new();
        // Re-run scout to populate e_new.nmf exactly identical to e_old.
        // This takes ~150ms and avoids accessing private fields or risking shared state.
        let scout_new = e_new.scout(&signal, 48000, None, None, false);

        let mut new_voice: Vec<f32> = Vec::new();
        let mut new_drums: Vec<f32> = Vec::new();
        let mut new_bass: Vec<f32> = Vec::new();
        let mut new_harmonics: Vec<f32> = Vec::new();
        let mut new_ambience: Vec<f32> = Vec::new();

        let source = super::RawPcmFileSource::new(&temp_path, 2).unwrap();
        let reader = SlidingOverlapReader::new(source, 10240);

        e_new
            .process_stream_with_params(
                reader,
                &scout_new,
                1.0,
                false,
                false,
                &[],
                1.0,
                None,
                None,
                false,
                |chunk| {
                    new_voice.extend_from_slice(&chunk.voice);
                    new_drums.extend_from_slice(&chunk.drums);
                    new_bass.extend_from_slice(&chunk.bass);
                    new_harmonics.extend_from_slice(&chunk.harmonics);
                    new_ambience.extend_from_slice(&chunk.ambience);
                },
            )
            .unwrap();

        assert_eq!(old_voice.len(), new_voice.len(), "Length mismatch");
        for i in 0..old_voice.len() {
            assert_eq!(
                old_voice[i].to_bits(),
                new_voice[i].to_bits(),
                "Bit mismatch in voice at {}",
                i
            );
            assert_eq!(
                old_drums[i].to_bits(),
                new_drums[i].to_bits(),
                "Bit mismatch in drums at {}",
                i
            );
            assert_eq!(
                old_bass[i].to_bits(),
                new_bass[i].to_bits(),
                "Bit mismatch in bass at {}",
                i
            );
            assert_eq!(
                old_harmonics[i].to_bits(),
                new_harmonics[i].to_bits(),
                "Bit mismatch in harmonics at {}",
                i
            );
            assert_eq!(
                old_ambience[i].to_bits(),
                new_ambience[i].to_bits(),
                "Bit mismatch in ambience at {}",
                i
            );
        }
    }

    #[test]
    fn six_channel_reads_correctly() {
        // 6ch oracle: proves channels()==6, is_multiple_of(6) alignment,
        // frames_read = total_read / 4 / 6, and the LE decode loop —
        // all correct for N≠2. Uses write_test_pattern (raw f32 LE, no header).
        //
        // Layout: 3 frames × 6 channels = 18 interleaved samples.
        // Frame 0: [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]
        // Frame 1: [6.0, 7.0, 8.0, 9.0, 10.0, 11.0]
        // Frame 2: [12.0, 13.0, 14.0, 15.0, 16.0, 17.0]
        let samples: Vec<f32> = (0..18).map(|i| i as f32).collect();
        let file = write_test_pattern(&samples);

        let mut source = RawPcmFileSource::new(file.path(), 6).unwrap();
        assert_eq!(source.channels(), 6); // channels() returns the injected value

        // Fill 2 frames (12 slots = 2 × 6ch)
        let mut buf = vec![0.0f32; 12];
        let frames = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames, 2, "expected 2 frames from a 12-slot buffer");
        for (i, (&got, &expected)) in buf[..12].iter().zip(samples[..12].iter()).enumerate() {
            assert_eq!(
                got.to_bits(),
                expected.to_bits(),
                "bit mismatch at sample {i} in first fill"
            );
        }

        // Fill remaining 1 frame (buf still 12 slots, only 6 samples left on disk)
        let frames2 = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames2, 1, "expected 1 frame for the 3rd frame");
        for (i, (&got, &expected)) in buf[..6].iter().zip(samples[12..18].iter()).enumerate() {
            assert_eq!(
                got.to_bits(),
                expected.to_bits(),
                "bit mismatch at sample {i} in second fill"
            );
        }

        // EOF
        let frames3 = source.fill_buffer(&mut buf).unwrap();
        assert_eq!(frames3, 0, "expected 0 frames at EOF");

        // Misaligned buffer for a 6ch source must error with the parametric message.
        // Note: source is at EOF, but fill_buffer checks alignment BEFORE attempting
        // reads, so the error is returned regardless of stream position.
        let mut bad_buf = vec![0.0f32; 5]; // 5 is not a multiple of 6
        let err = source.fill_buffer(&mut bad_buf).unwrap_err();
        assert!(
            err.contains("is not a multiple of 6 channels"),
            "unexpected error: {err}"
        );
    }
}
