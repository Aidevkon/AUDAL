//! Lookahead Ring Primitive
//! Pillar 2 of Scout Vision: Macro-scale lookahead delay line.

extern crate alloc;

/// CONTRACT: call consume_into() BEFORE feed() in each cycle to
/// get the exact `capacity_frames`-length delay. Calling feed()
/// first shifts the observed delay to `capacity_frames -
/// block_size` instead, because feed() advances write_pos before
/// consume_into() reads from it. See
/// true_lookahead_detects_impulse_before_output test for a
/// worked-out proof of this off-by-one-block effect.
pub struct LookaheadRing {
    buffer: alloc::vec::Vec<f32>,
    capacity_frames: usize,
    block_size: usize,
    write_pos: usize,
    filled: usize,
}

impl LookaheadRing {
    pub fn new(capacity_frames: usize, block_size: usize) -> Self {
        assert!(
            capacity_frames.is_multiple_of(block_size),
            "capacity_frames must be a multiple of block_size"
        );
        Self {
            buffer: alloc::vec![0.0_f32; capacity_frames],
            capacity_frames,
            block_size,
            write_pos: 0,
            filled: 0,
        }
    }

    pub fn feed(&mut self, chunk: &[f32]) {
        assert_eq!(
            chunk.len(),
            self.block_size,
            "feed() chunk size must match block_size exactly"
        );

        let end_pos = self.write_pos + self.block_size;
        if end_pos <= self.capacity_frames {
            self.buffer[self.write_pos..end_pos].copy_from_slice(chunk);
        } else {
            let first_part = self.capacity_frames - self.write_pos;
            let second_part = self.block_size - first_part;
            self.buffer[self.write_pos..self.capacity_frames].copy_from_slice(&chunk[..first_part]);
            self.buffer[..second_part].copy_from_slice(&chunk[first_part..]);
        }

        self.write_pos = (self.write_pos + self.block_size) % self.capacity_frames;

        if self.filled < self.capacity_frames {
            self.filled = (self.filled + self.block_size).min(self.capacity_frames);
        }
    }

    pub fn consume_into(&mut self, dest: &mut [f32]) -> bool {
        assert_eq!(
            dest.len(),
            self.block_size,
            "consume_into() dest size must match block_size exactly"
        );

        if self.filled < self.capacity_frames {
            return false;
        }

        // In a steady-state ring (filled == capacity), the oldest block_size frames
        // are exactly at the current write_pos.
        let read_pos = self.write_pos;
        let end_pos = read_pos + self.block_size;

        if end_pos <= self.capacity_frames {
            dest.copy_from_slice(&self.buffer[read_pos..end_pos]);
        } else {
            let first_part = self.capacity_frames - read_pos;
            let second_part = self.block_size - first_part;
            dest[..first_part].copy_from_slice(&self.buffer[read_pos..self.capacity_frames]);
            dest[first_part..].copy_from_slice(&self.buffer[..second_part]);
        }

        true
    }

    /// peek_into:
    /// dest.len() can be ANY length <= self.capacity_frames.
    /// Decision rationale: Transient detection or RMS calculation might need to look
    /// at specific window sizes (e.g. 5ms = 240 samples) which don't necessarily
    /// match the DSP block_size (e.g. 512). Allowing arbitrary peek sizes makes
    /// the primitive vastly more useful for analysis nodes.
    pub fn peek_into(&self, offset_frames: usize, dest: &mut [f32]) -> bool {
        let peek_len = dest.len();
        assert!(
            peek_len > 0 && peek_len <= self.capacity_frames,
            "peek_into() dest size must be between 1 and capacity_frames"
        );

        if offset_frames + peek_len > self.filled {
            return false;
        }

        let oldest_pos =
            (self.capacity_frames + self.write_pos - self.filled) % self.capacity_frames;
        let read_pos = (oldest_pos + offset_frames) % self.capacity_frames;

        let end_pos = read_pos + peek_len;

        if end_pos <= self.capacity_frames {
            dest.copy_from_slice(&self.buffer[read_pos..end_pos]);
        } else {
            let first_part = self.capacity_frames - read_pos;
            let second_part = peek_len - first_part;
            dest[..first_part].copy_from_slice(&self.buffer[read_pos..self.capacity_frames]);
            dest[first_part..].copy_from_slice(&self.buffer[..second_part]);
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_then_peek_returns_correct_window() {
        let mut ring = LookaheadRing::new(100, 10);

        // Fill the ring completely
        for i in 0..10 {
            let chunk = vec![i as f32; 10];
            ring.feed(&chunk);
        }

        // Peek at offset 90 (the last fed chunk, which was vec![9.0; 10])
        let mut dest = vec![0.0; 10];
        let success = ring.peek_into(90, &mut dest);

        assert!(success);
        assert_eq!(dest, vec![9.0; 10]);

        // Peek at offset 0 (the oldest chunk, which was vec![0.0; 10])
        let mut dest_oldest = vec![0.0; 10];
        let success_oldest = ring.peek_into(0, &mut dest_oldest);

        assert!(success_oldest);
        assert_eq!(dest_oldest, vec![0.0; 10]);
    }

    #[test]
    fn peek_beyond_filled_returns_false() {
        let mut ring = LookaheadRing::new(100, 10);
        ring.feed(&vec![1.0; 10]); // filled = 10

        let mut dest = vec![0.0; 10];
        let success = ring.peek_into(20, &mut dest);

        assert!(!success);
    }

    #[test]
    fn true_lookahead_detects_impulse_before_output() {
        let capacity = 100;
        let block_size = 10;
        let mut ring = LookaheadRing::new(capacity, block_size);

        let zeros = vec![0.0; block_size];
        let mut out = vec![0.0; block_size];

        // Cycle 1-4: Feed zeros (consume-then-feed pattern)
        for _ in 1..=4 {
            let has_output = ring.consume_into(&mut out);
            assert!(!has_output, "cold-start: should return false");
            ring.feed(&zeros);
        }

        // Cycle 5: Feed impulse
        let mut impulse = vec![0.0; block_size];
        impulse[5] = 1.0;

        let has_output = ring.consume_into(&mut out);
        assert!(!has_output, "cold-start still active");
        ring.feed(&impulse);

        // --- LOOKAHEAD PROOF ---
        let mut peek_dest = vec![0.0; block_size];
        let peek_success = ring.peek_into(40, &mut peek_dest);
        assert!(peek_success);
        assert_eq!(
            peek_dest, impulse,
            "Lookahead: Impulse seen immediately at cycle 5"
        );

        // Cycle 6-10: Feed zeros, filling the buffer
        for _ in 6..=10 {
            let _ = ring.consume_into(&mut out);
            ring.feed(&zeros);
        }

        // Now filled == 100. Cycle 11-14: Output should be the zeros from Cycle 1-4
        for cycle in 11..=14 {
            let has_output = ring.consume_into(&mut out);
            assert!(has_output);
            assert_eq!(out, zeros, "Outputting initial zeros at cycle {}", cycle);
            ring.feed(&zeros);
        }

        // --- DELAY PROOF ---
        // Cycle 15: Output should now be the impulse from Cycle 5
        let has_output = ring.consume_into(&mut out);
        assert!(has_output);
        assert_eq!(
            out, impulse,
            "Delay: Impulse outputted exactly 10 blocks later at cycle 15"
        );
        ring.feed(&zeros);
    }

    #[test]
    #[should_panic(expected = "capacity_frames must be a multiple of block_size")]
    fn new_rejects_non_multiple_capacity() {
        let _ring = LookaheadRing::new(105, 10);
    }
}
