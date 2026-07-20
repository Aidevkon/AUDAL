//! SlidingOverlapReader — bounded-lookback wrapper over any AudioSource.
//! Provides contiguous overlapping slices (history + new chunk) for DSP.

use super::audio_source::AudioSource;

pub struct OverlapChunk<'a> {
    pub signal: &'a [f32], // Mono downmix
    pub left: &'a [f32],
    pub right: &'a [f32],
    pub start: usize,  // Global index of the start of the history
    pub offset: usize, // Global index of the start of the new data
    pub end: usize,    // Global index of the end of the new data
}

pub struct SlidingOverlapReader<S: AudioSource> {
    source: S,
    global_offset: usize,
    history_l: Vec<f32>,
    history_r: Vec<f32>,
    history_m: Vec<f32>,
    read_buf: Vec<f32>,
    is_eof: bool,
    max_history: usize,
}

impl<S: AudioSource> SlidingOverlapReader<S> {
    pub fn new(source: S, max_history: usize) -> Self {
        Self {
            source,
            global_offset: 0,
            history_l: Vec::with_capacity(max_history + 4096),
            history_r: Vec::with_capacity(max_history + 4096),
            history_m: Vec::with_capacity(max_history + 4096),
            read_buf: Vec::new(),
            is_eof: false,
            max_history,
        }
    }

    pub fn next_chunk(&mut self, chunk_frames: usize) -> Result<Option<OverlapChunk<'_>>, String> {
        if self.is_eof {
            return Ok(None);
        }

        let channels = self.source.channels();
        assert_eq!(channels, 2, "SlidingOverlapReader requires stereo input");

        // Shift history to the front
        let current_len = self.history_l.len();
        let keep_len = current_len.min(self.max_history);
        if keep_len > 0 {
            self.history_l
                .copy_within(current_len - keep_len..current_len, 0);
            self.history_r
                .copy_within(current_len - keep_len..current_len, 0);
            self.history_m
                .copy_within(current_len - keep_len..current_len, 0);
        }
        self.history_l.truncate(keep_len);
        self.history_r.truncate(keep_len);
        self.history_m.truncate(keep_len);

        self.read_buf.resize(chunk_frames * 2, 0.0);
        let mut frames_read = 0;

        // Loop to handle partial reads up to chunk_frames or EOF
        while frames_read < chunk_frames {
            let remainder = &mut self.read_buf[frames_read * 2..chunk_frames * 2];
            if remainder.is_empty() {
                break;
            }
            match self.source.fill_buffer(remainder) {
                Ok(0) => {
                    self.is_eof = true;
                    break;
                }
                Ok(n) => {
                    frames_read += n;
                }
                Err(e) => {
                    return Err(format!("AudioSource read error: {e}"));
                }
            }
        }

        if frames_read == 0 {
            return Ok(None);
        }

        self.history_l.reserve(frames_read);
        self.history_r.reserve(frames_read);
        self.history_m.reserve(frames_read);

        // Deinterleave and append new data
        for i in 0..frames_read {
            let l = self.read_buf[i * 2];
            let r = self.read_buf[i * 2 + 1];
            let m = (l + r) * 0.5;

            self.history_l.push(l);
            self.history_r.push(r);
            self.history_m.push(m);
        }

        let start = self.global_offset.saturating_sub(self.max_history);
        let offset = self.global_offset;
        let end = self.global_offset + frames_read;

        self.global_offset = end;

        Ok(Some(OverlapChunk {
            signal: &self.history_m,
            left: &self.history_l,
            right: &self.history_r,
            start,
            offset,
            end,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSource {
        data: Vec<f32>,
        read_pos: usize,
        chunk_sizes: Vec<usize>,
        call_idx: usize,
    }

    impl AudioSource for MockSource {
        fn sample_rate(&self) -> u32 {
            48000
        }
        fn channels(&self) -> usize {
            2
        }
        fn total_frames_hint(&self) -> Option<u64> {
            Some((self.data.len() / 2) as u64)
        }
        fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
            if self.read_pos >= self.data.len() {
                return Ok(0);
            }
            let requested_frames = buffer.len() / 2;
            let mut yield_frames = if self.call_idx < self.chunk_sizes.len() {
                self.chunk_sizes[self.call_idx].min(requested_frames)
            } else {
                requested_frames
            };
            self.call_idx += 1;

            let frames_left = (self.data.len() - self.read_pos) / 2;
            yield_frames = yield_frames.min(frames_left);

            if yield_frames == 0 {
                return Ok(0);
            }

            let bytes_to_copy = yield_frames * 2;
            buffer[..bytes_to_copy]
                .copy_from_slice(&self.data[self.read_pos..self.read_pos + bytes_to_copy]);
            self.read_pos += bytes_to_copy;
            Ok(yield_frames)
        }
    }

    // GAP 1: Memory Bound test
    #[test]
    fn capacity_is_strictly_bounded() {
        let total_frames = 51200; // 100x CHUNK_FRAMES (assuming 512)
        let data = vec![0.0_f32; total_frames * 2];
        let source = MockSource {
            data,
            read_pos: 0,
            chunk_sizes: vec![],
            call_idx: 0,
        };

        let max_history = 10240;
        let chunk_size = 512;
        let mut reader = SlidingOverlapReader::new(source, max_history);

        let mut chunk_idx = 0;

        #[allow(clippy::while_let_loop)]
        loop {
            let chunk_len = match reader.next_chunk(chunk_size).unwrap() {
                Some(chunk) => chunk.left.len(),
                None => break,
            };
            chunk_idx += 1;

            // The strict upper bound for length is max_history + chunk_size
            let expected_max_len = max_history + chunk_size;

            // To ensure O(1) memory, the capacity must never exceed what's needed for the window.
            // Vector reallocation may give capacity a little above expected_max_len due to power-of-two rounding,
            // but we assert it doesn't grow wildly (e.g. bounded by 2 * expected_max_len).
            let max_allowed_capacity = expected_max_len * 2;

            let cap_l = reader.history_l.capacity();
            let cap_r = reader.history_r.capacity();
            let cap_m = reader.history_m.capacity();

            assert!(
                cap_l <= max_allowed_capacity,
                "history_l capacity grew unbounded! Chunk {}, cap {}, allowed {}",
                chunk_idx,
                cap_l,
                max_allowed_capacity
            );
            assert!(
                cap_r <= max_allowed_capacity,
                "history_r capacity grew unbounded! Chunk {}, cap {}, allowed {}",
                chunk_idx,
                cap_r,
                max_allowed_capacity
            );
            assert!(
                cap_m <= max_allowed_capacity,
                "history_m capacity grew unbounded! Chunk {}, cap {}, allowed {}",
                chunk_idx,
                cap_m,
                max_allowed_capacity
            );

            // Print out for report visibility
            if chunk_idx == 1 || chunk_idx == 10 || chunk_idx == 50 || chunk_idx == 100 {
                eprintln!(
                    "Chunk {}: length = {}, cap_l = {}, cap_r = {}, cap_m = {}",
                    chunk_idx, chunk_len, cap_l, cap_r, cap_m
                );
            }
        }

        assert_eq!(chunk_idx, 100, "Did not process all 100 chunks");
    }

    // Category 1: Oracle Reconstruction with distinct-content signal spanning boundaries
    #[test]
    fn category1_oracle_history_tail_matches_exactly() {
        let total_frames = 1500;
        let mut data = Vec::with_capacity(total_frames * 2);
        for i in 0..total_frames {
            data.push(i as f32 * 0.1);
            data.push(i as f32 * 0.2);
        }
        let source = MockSource {
            data,
            read_pos: 0,
            chunk_sizes: vec![],
            call_idx: 0,
        };
        let mut reader = SlidingOverlapReader::new(source, 1000);
        let mut last_tail_l = Vec::new();
        let mut chunk_idx = 0;

        while let Some(chunk) = reader.next_chunk(512).unwrap() {
            if chunk_idx > 0 {
                let history_len = chunk.offset - chunk.start;
                let history_portion = &chunk.left[..history_len];
                assert_eq!(
                    history_portion, last_tail_l,
                    "Chunk {} history does not match previous tail",
                    chunk_idx
                );
            }
            let tail_len = chunk.left.len().min(1000);
            last_tail_l = chunk.left[chunk.left.len() - tail_len..].to_vec();
            chunk_idx += 1;
        }
    }

    // Category 2: First chunk start == offset == 0
    #[test]
    fn category2_first_chunk_start_offset_zero() {
        let source = MockSource {
            data: vec![0.0; 4096 * 2],
            read_pos: 0,
            chunk_sizes: vec![],
            call_idx: 0,
        };
        let mut reader = SlidingOverlapReader::new(source, 10240);
        let chunk = reader.next_chunk(512).unwrap().unwrap();
        assert_eq!(chunk.start, 0);
        assert_eq!(chunk.offset, 0);
        assert_eq!(chunk.end, 512);
    }

    // Category 3: Concatenation exactly reconstructs original signal
    #[test]
    fn category3_concatenation_reconstructs_exactly() {
        let total_frames = 1500;
        let mut data = Vec::with_capacity(total_frames * 2);
        let mut original_l = Vec::with_capacity(total_frames);
        for i in 0..total_frames {
            let l = i as f32 * 0.1;
            data.push(l);
            data.push(i as f32 * 0.2);
            original_l.push(l);
        }
        let source = MockSource {
            data,
            read_pos: 0,
            chunk_sizes: vec![],
            call_idx: 0,
        };
        let mut reader = SlidingOverlapReader::new(source, 1000);

        let mut reconstructed_l = Vec::new();
        while let Some(chunk) = reader.next_chunk(512).unwrap() {
            let new_data_l = &chunk.left[chunk.offset - chunk.start..];
            reconstructed_l.extend_from_slice(new_data_l);
        }
        assert_eq!(reconstructed_l.len(), original_l.len());
        assert_eq!(reconstructed_l, original_l);
    }

    // Category 4: Irregular/partial fill_buffer sizes
    #[test]
    fn category4_irregular_fill_buffer_sizes() {
        let total_frames = 512;
        let data = vec![1.0_f32; total_frames * 2];
        let source = MockSource {
            data,
            read_pos: 0,
            chunk_sizes: vec![100, 50, 200, 162], // Irregular chunks summing to 512
            call_idx: 0,
        };
        let mut reader = SlidingOverlapReader::new(source, 1000);

        // Even though the source yielded piecemeal, next_chunk should abstract it
        // and return one clean 512-frame chunk.
        let chunk = reader.next_chunk(512).unwrap().unwrap();
        assert_eq!(
            chunk.end - chunk.offset,
            512,
            "Did not accumulate full 512 frames"
        );

        // Next should be EOF
        let next = reader.next_chunk(512).unwrap();
        assert!(next.is_none());
    }
}
