use serde::Deserialize;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

#[derive(Deserialize)]
pub struct TimeAwareBehaviourJson {
    pub sections: Vec<SectionJson>,
}

#[derive(Deserialize)]
pub struct SectionJson {
    pub start_ms: f64,
    pub end_ms: f64,
    pub crossfade_ms: f64,
    pub topology: DspTopology,
}

pub struct ScheduledSection {
    pub start_sample: u64,
    pub end_sample: u64,
    pub canonical_graph: DspGraph,
    pub ready_graph: Option<DspGraph>,
    pub crossfade_samples: usize,
}

pub struct SectionScheduler {
    pub sections: Vec<ScheduledSection>,
    pub current_index: usize,
    pub playback_sample: u64,
}

#[derive(Debug)]
pub enum SchedulerError {
    InvalidJson(String),
    NoSections,
    GraphError(String),
}

impl SectionScheduler {
    pub fn from_time_aware_behaviour(
        json: &str,
        block_size: usize,
        sample_rate: u32,
    ) -> Result<Self, SchedulerError> {
        let tab: TimeAwareBehaviourJson =
            serde_json::from_str(json).map_err(|e| SchedulerError::InvalidJson(e.to_string()))?;

        if tab.sections.is_empty() {
            return Err(SchedulerError::NoSections);
        }

        let mut sections = Vec::new();
        for sec in tab.sections {
            let start_sample = (sec.start_ms * sample_rate as f64 / 1000.0) as u64;
            let end_sample = (sec.end_ms * sample_rate as f64 / 1000.0) as u64;
            let crossfade_samples = (sec.crossfade_ms * sample_rate as f64 / 1000.0) as usize;

            let canonical_graph = DspGraph::from_topology(&sec.topology, block_size, sample_rate)
                .map_err(|e| SchedulerError::GraphError(format!("{:?}", e)))?;

            let ready_graph = Some(canonical_graph.clone());

            sections.push(ScheduledSection {
                start_sample,
                end_sample,
                canonical_graph,
                ready_graph,
                crossfade_samples,
            });
        }

        // Sort by start_sample to be safe
        sections.sort_by_key(|s| s.start_sample);

        Ok(Self {
            sections,
            current_index: 0,
            playback_sample: 0,
        })
    }

    pub fn advance(&mut self, block_size: usize) -> Option<usize> {
        let current_section = &self.sections[self.current_index];

        // Edge case: if we are in the last section and reach the end, do not wrap around
        // Just freeze playback_sample at end_sample and return None.
        if self.playback_sample + block_size as u64 >= current_section.end_sample {
            if self.current_index + 1 < self.sections.len() {
                self.playback_sample += block_size as u64;
                self.current_index += 1;
                return Some(self.current_index);
            } else {
                // Last section, do not wrap, stop advancing
                self.playback_sample = current_section.end_sample;
                return None;
            }
        }

        self.playback_sample += block_size as u64;
        None
    }

    pub fn find_section_index(&self, sample: u64) -> Option<usize> {
        for (i, sec) in self.sections.iter().enumerate() {
            if sample >= sec.start_sample && sample < sec.end_sample {
                return Some(i);
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.playback_sample = 0;
        self.current_index = 0;
    }
}
