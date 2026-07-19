use crate::crossfader::Crossfader;
use crate::scheduler::SectionScheduler;
use crate::stem_engine::StemEngine;
use sp314_nodes::{graph::DspGraph, topology::DspTopology};
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
fn js_error(msg: &str) -> JsValue {
    JsValue::from_str(msg)
}

#[cfg(not(target_arch = "wasm32"))]
fn js_error(_msg: &str) -> JsValue {
    JsValue::UNDEFINED
}

#[wasm_bindgen]
pub struct LoomEngine {
    /// None only during an active crossfade (graph ownership
    /// transferred to the crossfader). All mutator methods on this
    /// engine silently no-op while None — matches the pre-existing
    /// behavior where parameter changes during crossfade were applied
    /// to a soon-discarded dummy clone and lost; this makes that same
    /// loss explicit instead of silent-by-accident.
    graph: Option<DspGraph>,
    block_size: usize,
    sample_rate: u32,
    left_buf: Vec<f32>,
    right_buf: Vec<f32>,
    scheduler: Option<SectionScheduler>,
    crossfader: Crossfader,

    // Stem mode
    stem_engines: Vec<StemEngine>,
    stem_mode: bool,
    playback_frame: usize,
    sum_left: Vec<f32>,
    sum_right: Vec<f32>,
    stem_left: Vec<f32>,
    stem_right: Vec<f32>,
}

#[wasm_bindgen]
impl LoomEngine {
    #[wasm_bindgen(constructor)]
    pub fn new(
        topology_json: &str,
        block_size: usize,
        sample_rate: u32,
    ) -> Result<LoomEngine, JsValue> {
        let topology = DspTopology::from_json(topology_json)
            .map_err(|e| js_error(&format!("Topology parse error: {}", e)))?;

        let graph = DspGraph::from_topology(&topology, block_size, sample_rate)
            .map_err(|e| js_error(&format!("Graph build error: {:?}", e)))?;

        Ok(LoomEngine {
            graph: Some(graph),
            block_size,
            sample_rate,
            left_buf: vec![0.0; block_size],
            right_buf: vec![0.0; block_size],
            scheduler: None,
            crossfader: Crossfader::new(block_size),

            stem_engines: Vec::new(),
            stem_mode: false,
            playback_frame: 0,
            sum_left: vec![0.0; block_size],
            sum_right: vec![0.0; block_size],
            stem_left: vec![0.0; block_size],
            stem_right: vec![0.0; block_size],
        })
    }

    pub fn load_time_aware_behaviour(&mut self, tab_json: &str) -> Result<(), JsValue> {
        let mut scheduler = SectionScheduler::from_time_aware_behaviour(
            tab_json,
            self.block_size,
            self.sample_rate,
        )
        .map_err(|e| js_error(&format!("Scheduler error: {:?}", e)))?;

        // Start playing the first section immediately
        if let Some(first_section) = scheduler.sections.first_mut() {
            if let Some(graph) = first_section.ready_graph.take() {
                self.graph = Some(graph);
            }
        }
        self.scheduler = Some(scheduler);
        Ok(())
    }

    pub fn process_with_sections(&mut self, input_output: &mut [f32]) {
        if input_output.len() != self.block_size * 2 {
            return;
        }

        // 1. Deinterleave
        for i in 0..self.block_size {
            self.left_buf[i] = input_output[i * 2];
            self.right_buf[i] = input_output[i * 2 + 1];
        }

        // 2. Check if crossfade is in progress
        if self.crossfader.is_active() {
            let complete = self
                .crossfader
                .process_block(&mut self.left_buf, &mut self.right_buf);
            if complete {
                if let Some(completed_graph) = self.crossfader.take_completed_graph() {
                    self.graph = Some(completed_graph);
                }
            }
        } else {
            // 3. Normal processing
            if let Some(graph) = self.graph.as_mut() {
                graph.process_block(&mut self.left_buf, &mut self.right_buf);
            }

            // 4. Advance scheduler and check for section boundary
            if let Some(scheduler) = &mut self.scheduler {
                if let Some(new_index) = scheduler.advance(self.block_size) {
                    if let Some(section) = scheduler.sections.get_mut(new_index) {
                        if let Some(new_graph) = section.ready_graph.take() {
                            let fade_samples = section.crossfade_samples;

                            // We pass the old graph directly to the crossfader, leaving self.graph
                            // as None for the duration of the crossfade!
                            let old_graph = self.graph.take().unwrap();
                            self.crossfader.begin(old_graph, new_graph, fade_samples);
                        }
                    }
                }
            }
        }

        // 5. Interleave back
        for i in 0..self.block_size {
            input_output[i * 2] = self.left_buf[i];
            input_output[i * 2 + 1] = self.right_buf[i];
        }
    }

    pub fn seek_to_ms(&mut self, position_ms: f64) {
        if let Some(scheduler) = &mut self.scheduler {
            // Replenish missing ready_graphs
            for section in &mut scheduler.sections {
                if section.ready_graph.is_none() {
                    section.ready_graph = Some(section.canonical_graph.clone());
                }
            }

            let sample = (position_ms * self.sample_rate as f64 / 1000.0) as u64;
            scheduler.playback_sample = sample;

            if let Some(idx) = scheduler.find_section_index(sample) {
                scheduler.current_index = idx;
                if let Some(mut graph) = scheduler.sections[idx].ready_graph.take() {
                    graph.reset();
                    self.graph = Some(graph);
                }
            }
            self.crossfader.clear();
        }
    }

    pub fn current_position_ms(&self) -> f64 {
        if let Some(scheduler) = &self.scheduler {
            (scheduler.playback_sample as f64 * 1000.0) / self.sample_rate as f64
        } else {
            0.0
        }
    }

    pub fn process(&mut self, input_output: &mut [f32]) {
        if input_output.len() != self.block_size * 2 {
            return;
        }
        for i in 0..self.block_size {
            self.left_buf[i] = input_output[i * 2];
            self.right_buf[i] = input_output[i * 2 + 1];
        }
        if let Some(graph) = self.graph.as_mut() {
            graph.process_block(&mut self.left_buf, &mut self.right_buf);
        }
        for i in 0..self.block_size {
            input_output[i * 2] = self.left_buf[i];
            input_output[i * 2 + 1] = self.right_buf[i];
        }
    }

    pub fn set_node_parameter(
        &mut self,
        node_id: &str,
        param: &str,
        value: f32,
    ) -> Result<(), JsValue> {
        if let Some(graph) = self.graph.as_mut() {
            graph
                .set_node_parameter(node_id, param, value)
                .map_err(|e| js_error(&format!("{:?}", e)))
        } else {
            Ok(())
        }
    }

    pub fn set_stem_node_parameter(
        &mut self,
        stem_id: &str,
        node_id: &str,
        param: &str,
        value: f32,
    ) -> Result<(), JsValue> {
        // Use a simple if-chain without allocating a hashmap, per architect note
        for engine in &mut self.stem_engines {
            if engine.id == stem_id {
                return engine
                    .set_node_parameter(node_id, param, value)
                    .map_err(|e| js_error(&format!("{:?}", e)));
            }
        }
        Ok(())
    }

    pub fn set_global_glide_ms(&mut self, glide_ms: f32) {
        if let Some(graph) = self.graph.as_mut() {
            graph.set_global_glide_ms(glide_ms);
        }
        for engine in &mut self.stem_engines {
            engine.set_global_glide_ms(glide_ms);
        }
    }

    pub fn reset(&mut self) {
        if let Some(graph) = self.graph.as_mut() {
            graph.reset();
        }
        if let Some(scheduler) = &mut self.scheduler {
            scheduler.reset();
        }
        self.crossfader.clear();
        for se in &mut self.stem_engines {
            se.reset();
        }
        self.playback_frame = 0;
    }

    pub fn load_stems(
        &mut self,
        vocals_flac: &[u8],
        drums_flac: &[u8],
        bass_flac: &[u8],
        other_flac: &[u8],
        tab_json: &str,
    ) -> Result<(), JsValue> {
        // Parse TAB to get stem_configs if available, else just minimal topology for each.
        // For simplicity and since we don't have TimeAwareBehaviour stem_configs parsing yet,
        // we parse standard configs, or just fallback to default graphs for each stem.
        // Actually the prompt says: "Builds per-stem DspGraphs from TimeAwareBehaviour stem_configs."
        // We'll decode the FLACs first.
        use crate::stem::StemBuffer;
        use crate::stem_engine::StemEngine;

        let v_buf = StemBuffer::from_flac_bytes("vocals", vocals_flac)
            .map_err(|e| js_error(&format!("Vocals decode: {:?}", e)))?;
        let d_buf = StemBuffer::from_flac_bytes("drums", drums_flac)
            .map_err(|e| js_error(&format!("Drums decode: {:?}", e)))?;
        let b_buf = StemBuffer::from_flac_bytes("bass", bass_flac)
            .map_err(|e| js_error(&format!("Bass decode: {:?}", e)))?;
        let o_buf = StemBuffer::from_flac_bytes("other", other_flac)
            .map_err(|e| js_error(&format!("Other decode: {:?}", e)))?;

        // Parse JSON for stem_configs
        #[derive(serde::Deserialize)]
        struct TabWithStems {
            stem_configs:
                Option<std::collections::HashMap<String, sp314_nodes::topology::DspTopology>>,
        }

        let tab: TabWithStems =
            serde_json::from_str(tab_json).map_err(|e| js_error(&format!("JSON error: {}", e)))?;
        let mut configs = tab.stem_configs.unwrap_or_default();

        // Helper to get or create minimal graph
        let mut make_graph = |id: &str| -> Result<DspGraph, JsValue> {
            let top = configs.remove(id).unwrap_or_else(|| {
                sp314_nodes::topology::DspTopology::from_json(r#"{"topology_id":"minimal","nodes":[{"node_id":"Input","node_type":"Input","parameters":{}},{"node_id":"Output","node_type":"Output","parameters":{}}],"edges":[{"source":"Input","target":"Output"}]}"#).unwrap()
            });
            DspGraph::from_topology(&top, self.block_size, self.sample_rate)
                .map_err(|e| js_error(&format!("Graph error: {:?}", e)))
        };

        let v_engine = StemEngine::new(v_buf, make_graph("vocals")?, self.block_size);
        let d_engine = StemEngine::new(d_buf, make_graph("drums")?, self.block_size);
        let b_engine = StemEngine::new(b_buf, make_graph("bass")?, self.block_size);
        let o_engine = StemEngine::new(o_buf, make_graph("other")?, self.block_size);

        self.stem_engines = vec![v_engine, d_engine, b_engine, o_engine];
        self.stem_mode = true;
        self.playback_frame = 0;

        Ok(())
    }

    pub fn process_stems(&mut self, output: &mut [f32]) {
        if output.len() != self.block_size * 2 {
            return;
        }

        self.sum_left.fill(0.0);
        self.sum_right.fill(0.0);

        for engine in &mut self.stem_engines {
            engine.process_block(
                self.playback_frame,
                &mut self.stem_left,
                &mut self.stem_right,
            );
            for i in 0..self.block_size {
                self.sum_left[i] += self.stem_left[i];
                self.sum_right[i] += self.stem_right[i];
            }
        }

        self.playback_frame += self.block_size;

        for i in 0..self.block_size {
            output[i * 2] = self.sum_left[i];
            output[i * 2 + 1] = self.sum_right[i];
        }
    }

    pub fn seek_stems_to_ms(&mut self, position_ms: f64) {
        self.playback_frame = (position_ms * self.sample_rate as f64 / 1000.0) as usize;
    }
}
