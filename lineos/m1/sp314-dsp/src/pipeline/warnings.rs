//! WarningAggregator — sp314-dsp v2.9 §Warning Aggregator.

extern crate alloc;

use alloc::vec::Vec;

pub use crate::types::warnings::{LimiterOverworkSource, PipelineWarning, WarningRecord};

/// Aggregates pipeline warnings by discriminant (max 9 variant slots).
pub struct WarningAggregator {
    records: Vec<WarningRecord>,
}

impl WarningAggregator {
    pub fn new() -> Self {
        Self {
            records: Vec::with_capacity(9),
        }
    }

    /// Record a warning at `block_index`; merge with existing discriminant if present.
    pub fn push(&mut self, warning: PipelineWarning, block_index: u64) {
        for record in self.records.iter_mut() {
            if warning.same_discriminant(&record.warning) {
                record.count += 1;
                record.last_block = block_index;
                return;
            }
        }
        self.records.push(WarningRecord {
            warning,
            count:       1,
            first_block: block_index,
            last_block:  block_index,
        });
    }

    pub fn records(&self) -> &[WarningRecord] {
        &self.records
    }
}
