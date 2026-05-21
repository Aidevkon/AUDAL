//! StagePool — pre-allocated scratch for stage buffers (v2.9 §Stage Pool).
//! One heap allocation at pipeline init; zero allocations in the hot path.

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;

/// Maximum supported track length at 48 kHz (30 minutes), in frames per channel.
pub const MAX_POOL_FRAMES: usize = 48_000 * 60 * 30;

/// Scratch budget: eight concurrent stage buffers of `MAX_POOL_FRAMES` samples each.
pub const POOL_SCRATCH_BUDGET: usize = MAX_POOL_FRAMES * 8;

/// Pre-allocated contiguous scratch buffer for stage processing.
pub struct StagePool {
    scratch: Box<[f32]>,
    cursor:  usize,
}

impl StagePool {
    /// Initialise at pipeline startup. One allocation, never repeated.
    pub fn new() -> Self {
        let mut scratch = Vec::with_capacity(POOL_SCRATCH_BUDGET);
        // SAFETY: stages write every rented slice before read; reset() does not require zeroed memory.
        unsafe {
            scratch.set_len(POOL_SCRATCH_BUDGET);
        }
        Self {
            scratch: scratch.into_boxed_slice(),
            cursor:  0,
        }
    }

    /// Rent a mutable slice for one stage.
    /// Returns `None` if exhausted (DSPInvariant — input should have passed `validate_input`).
    pub fn rent(&mut self, size: usize) -> Option<&mut [f32]> {
        if self.cursor.saturating_add(size) > POOL_SCRATCH_BUDGET {
            return None;
        }
        let end = self.cursor + size;
        let slice = &mut self.scratch[self.cursor..end];
        self.cursor = end;
        Some(slice)
    }

    /// Reset cursor at the start of every pipeline run. Does not zero memory.
    #[inline(always)]
    pub fn reset(&mut self) {
        self.cursor = 0;
    }
}
