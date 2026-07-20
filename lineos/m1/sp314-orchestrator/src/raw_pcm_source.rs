//! RawPcmFileSource — reads interleaved f32 directly from disk.
//! Matches the raw dump format written by decode_node.rs.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sp314_dsp::stft::sliding_overlap_reader::ChunkSource;

pub struct RawPcmFileSource {
    reader: BufReader<File>,
}

impl RawPcmFileSource {
    pub fn new(path: &Path) -> Result<Self, String> {
        let file =
            File::open(path).map_err(|e| format!("Failed to open {}: {}", path.display(), e))?;
        Ok(Self {
            reader: BufReader::new(file),
        })
    }
}

impl ChunkSource for RawPcmFileSource {
    fn channels(&self) -> usize {
        2 // Strict constraint for Music/Stereo path
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

        // We read directly into the caller's f32 buffer by temporarily viewing
        // it as a mutable byte slice. This exactly mirrors the unsafe cast used
        // during writing in decode_node.rs and avoids an intermediate Vec<u8> allocation.
        let byte_len = buffer.len() * 4;
        let byte_buf: &mut [u8] =
            unsafe { std::slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, byte_len) };

        // Read up to byte_len bytes
        let mut total_read = 0;
        while total_read < byte_len {
            match self.reader.read(&mut byte_buf[total_read..]) {
                Ok(0) => break, // EOF
                Ok(n) => total_read += n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("Read error: {e}")),
            }
        }

        // Return number of frames read (which is total bytes read / 4 / channels)
        // If we read a partial float, we drop the fractional part to maintain alignment,
        // though our write pattern guarantees aligned 4-byte boundaries.
        Ok(total_read / 4 / channels)
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

        // Test with various chunk sizes (some aligned, some not)
        // Channels is 2, so buffer length must be multiple of 2.
        let chunk_sizes = vec![2, 4, 10, 100, 1000, 2000, 3];

        for &chunk_size in &chunk_sizes {
            let mut source = RawPcmFileSource::new(file.path()).unwrap();
            let mut all_read = Vec::new();

            // Adjust chunk size if it's odd, since trait contract requires multiple of channels
            let buf_size = if chunk_size % 2 != 0 {
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
                all_read.extend_from_slice(&buf[..frames * 2]);
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
        let samples = vec![1.0, 2.0]; // 1 frame
        let file = write_test_pattern(&samples);
        let mut source = RawPcmFileSource::new(file.path()).unwrap();

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
        let samples = vec![1.0, 2.0]; // 1 frame
        let file = write_test_pattern(&samples);
        let mut source = RawPcmFileSource::new(file.path()).unwrap();

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
        use sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader;
        use sp314_dsp::stft::two_pass::TwoPassEngine;
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
        let scout = engine.scout(&signal, 48000);

        let mut old_voice = Vec::new();
        let mut old_drums = Vec::new();
        let mut old_bass = Vec::new();
        let mut old_harmonics = Vec::new();
        let mut old_ambience = Vec::new();

        engine
            .process_slices_with_params(&signal, &left, &right, &scout, 1.0, |chunk| {
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
        let scout_new = e_new.scout(&signal, 48000);

        let mut new_voice = Vec::new();
        let mut new_drums = Vec::new();
        let mut new_bass = Vec::new();
        let mut new_harmonics = Vec::new();
        let mut new_ambience = Vec::new();

        let source = super::RawPcmFileSource::new(&temp_path).unwrap();
        let reader = SlidingOverlapReader::new(source, 10240);

        e_new
            .process_stream_with_params(reader, &scout_new, 1.0, |chunk| {
                new_voice.extend_from_slice(&chunk.voice);
                new_drums.extend_from_slice(&chunk.drums);
                new_bass.extend_from_slice(&chunk.bass);
                new_harmonics.extend_from_slice(&chunk.harmonics);
                new_ambience.extend_from_slice(&chunk.ambience);
            })
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
}
