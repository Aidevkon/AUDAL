use lineos_types::certificate::StageRecord;
use std::time::Instant;

/// Profiler for the deterministic forensic timeline.
/// Captures exact durations and BLAKE3 hashes at stage boundaries.
pub struct TimelineProfiler {
    records: Vec<StageRecord>,
    last_mark: Instant,
}

impl Default for TimelineProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl TimelineProfiler {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            last_mark: Instant::now(),
        }
    }

    /// Mark a stage completion.
    /// Calculates duration since the last mark (or instantiation),
    /// computes a fast BLAKE3 hash of the provided PCM buffer,
    /// and pushes the record to the timeline.
    pub fn mark_stage(&mut self, stage_name: &str, buffer_for_hash: &[f32]) {
        let duration_ms = self.last_mark.elapsed().as_millis() as u64;
        let stage_hash = crate::dsp_pipeline_helpers::blake3_pcm(buffer_for_hash);

        self.records.push(StageRecord {
            stage: stage_name.to_string(),
            duration_ms,
            stage_hash,
        });

        // Reset the mark for the next stage
        self.last_mark = Instant::now();
    }

    /// Mark a stage completion using a precomputed hash.
    /// Used for stages where the hash is computed incrementally (e.g. Ingest).
    pub fn mark_stage_with_hash(&mut self, stage_name: &str, stage_hash: String) {
        let duration_ms = self.last_mark.elapsed().as_millis() as u64;

        self.records.push(StageRecord {
            stage: stage_name.to_string(),
            duration_ms,
            stage_hash,
        });

        // Reset the mark for the next stage
        self.last_mark = Instant::now();
    }

    /// Return the accumulated forensic timeline.
    pub fn finalize(self) -> Vec<StageRecord> {
        self.records
    }
}
