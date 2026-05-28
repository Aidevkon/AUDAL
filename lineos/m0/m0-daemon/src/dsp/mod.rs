// src/dsp/mod.rs
// v3 DSP adapter for m0-daemon.
// Replaces MasteringPipeline::master() from v2.9.
// Orchestrates: pipelineforge → sp314-nodes DspGraph → process_offline

pub mod autotune;

use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;
use lineos_types::{
    StereoBuffer, LufsReport, MasteringIntent,
};

pub struct DspAdapter;

impl DspAdapter {
    /// Master a stereo buffer using v3 engine.
    /// Replaces MasteringPipeline::master().
    pub fn master(
        intent: &MasteringIntent,
        audio: &mut StereoBuffer,
        aether_config: Option<&integration::config::DspConfig>,
    ) -> Result<MasteringResult, DspError> {

        // 1. Build EngineerConditions from intent
        let conditions = Self::intent_to_conditions(intent, audio);

        // 2. Forge the DSP topology
        let topology_json = Pipelineforge::forge(&conditions)
            .map_err(|e| DspError::ForgeError(format!("{:?}", e)))?;

        // 3. Build DspGraph
        let mut topology = DspTopology::from_json(&topology_json)
            .map_err(|e| DspError::TopologyError(format!("{:?}", e)))?;

        if let Some(config) = aether_config {
            Self::apply_topology_overrides(&mut topology, config);
        }

        let block_size = 512;
        let mut graph = DspGraph::from_topology(
            &topology,
            block_size,
            audio.sample_rate,
        ).map_err(|e| DspError::GraphError(format!("{:?}", e)))?;

        // 4. Process offline in blocks
        let num_frames = audio.num_frames;
        let mut frame = 0;
        while frame < num_frames {
            let end = (frame + block_size).min(num_frames);
            let block_len = end - frame;
            if block_len < block_size {
                let mut pad_l = vec![0.0_f32; block_size];
                let mut pad_r = vec![0.0_f32; block_size];
                pad_l[..block_len].copy_from_slice(&audio.left[frame..end]);
                pad_r[..block_len].copy_from_slice(&audio.right[frame..end]);
                
                graph.process_block(&mut pad_l, &mut pad_r);
                
                audio.left[frame..end].copy_from_slice(&pad_l[..block_len]);
                audio.right[frame..end].copy_from_slice(&pad_r[..block_len]);
            } else {
                graph.process_block(
                    &mut audio.left[frame..end],
                    &mut audio.right[frame..end],
                );
            }
            frame += block_len;
        }

        // Post-process LUFS correction — mathematically exact
        // Measure actual output LUFS and correct to target
        use sp314_dsp::metering::measure_integrated_lufs;
        let output_lufs = measure_integrated_lufs(
            &audio.left, &audio.right);
        let target_lufs = intent.target.target_lufs;

        if output_lufs > -69.0 {
            let correction_db = target_lufs - output_lufs;
            // Clamp correction to ±18dB to avoid wild swings
            let correction_db = correction_db
                .max(-18.0_f32)
                .min(18.0_f32);
            let correction_linear = 10.0_f32
                .powf(correction_db / 20.0_f32);
            for s in audio.left.iter_mut() {
                *s *= correction_linear;
            }
            for s in audio.right.iter_mut() {
                *s *= correction_linear;
            }
        }

        // 5. Measure output LUFS
        // Use sp314-dsp metering if available, or compute simple RMS
        let output_lufs = Self::measure_lufs(&audio.left, &audio.right, audio.sample_rate);

        Ok(MasteringResult {
            lufs: output_lufs,
            preset_name: intent.preset_name.clone(),
        })
    }

    /// Apply deterministic Aether overrides to the DSP graph topology.
    fn apply_topology_overrides(topology: &mut DspTopology, config: &integration::config::DspConfig) {
        for node in &mut topology.nodes {
            if node.node_type == "Compressor" {
                if let Some(obj) = node.parameters.as_object_mut() {
                    obj.insert("threshold_db".into(), serde_json::json!(config.dynamics.comp_threshold_db));
                    obj.insert("ratio".into(), serde_json::json!(config.dynamics.comp_ratio));
                }
            }
            // TODO v2: Map eq, sat, stereo to exact node IDs.
        }
    }

    /// Translate MasteringIntent → EngineerConditions for Pipelineforge.
    fn intent_to_conditions(
        intent: &MasteringIntent,
        audio: &StereoBuffer,
    ) -> ConditionSet {
        let mut conditions = vec![];

        // Basic loudness conditions
        let approx_lufs = Self::estimate_lufs(&audio.left, &audio.right);
        if approx_lufs > intent.target.target_lufs + 2.0 {
            conditions.push(EngineerCondition::LufsTooLoud);
        } else if approx_lufs < intent.target.target_lufs - 6.0 {
            conditions.push(EngineerCondition::LufsTooQuiet);
        }

        ConditionSet {
            conditions,
            sample_rate: audio.sample_rate,
            target_lufs: intent.target.target_lufs,
        }
    }

    /// Simple RMS-based LUFS estimate (not BS.1770 — for routing only).
    fn estimate_lufs(left: &[f32], right: &[f32]) -> f32 {
        let sum_sq: f32 = left.iter().chain(right.iter())
            .map(|s| s * s)
            .sum::<f32>() / (left.len() + right.len()) as f32;
        if sum_sq < 1e-10 { return -144.0; }
        10.0 * sum_sq.log10() - 0.691
    }

    /// Measure output LUFS — uses sp314-dsp metering.
    fn measure_lufs(left: &[f32], right: &[f32], _sample_rate: u32) -> LufsReport {
        // Use sp314-dsp::metering::measure_integrated_lufs
        // Import here to avoid top-level sp314-dsp dependency in dsp/mod.rs
        use sp314_dsp::metering::measure_integrated_lufs;
        let lufs = measure_integrated_lufs(left, right);
        LufsReport {
            integrated_lufs:   lufs,
            true_peak_dbfs:    Self::true_peak(left, right),
            loudness_range_lu: 0.0, // LRA calculation is future scope
            short_term_lufs:   None,
        }
    }

    fn true_peak(left: &[f32], right: &[f32]) -> f32 {
        let peak = left.iter().chain(right.iter())
            .map(|s| s.abs())
            .fold(0.0_f32, f32::max);
        if peak < 1e-10 { return -144.0; }
        20.0 * peak.log10()
    }
}

pub struct MasteringResult {
    pub lufs:        LufsReport,
    pub preset_name: String,
}

#[derive(Debug)]
pub enum DspError {
    ForgeError(String),
    TopologyError(String),
    GraphError(String),
    ProcessError(String),
}
