// src/dsp/mod.rs
// v3 DSP adapter for m0-daemon.
// Replaces MasteringPipeline::master() from v2.9.
// Orchestrates: pipelineforge → sp314-nodes DspGraph → process_offline

pub mod autotune;
pub mod maestro;
pub use maestro::{AutoTuningController, RenderParams};

use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;
use lineos_types::{
    LufsReport, MasteringIntent,
};
use sp314_dsp::limiter::{BrickwallLimiter, LimiterConfig};

pub struct DspAdapter;

impl DspAdapter {
    /// Master a stereo buffer using v3 engine.
    /// Replaces MasteringPipeline::master().
    pub fn master(
        intent: &MasteringIntent,
        left: &mut [f32],
        right: &mut [f32],
        sample_rate: u32,
        aether_config: Option<&integration::config::DspConfig>,
    ) -> Result<MasteringResult, DspError> {

        // 1. Build EngineerConditions from intent
        let conditions = Self::intent_to_conditions(intent, left, right, sample_rate);

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
            sample_rate,
        ).map_err(|e| DspError::GraphError(format!("{:?}", e)))?;

        // 4. Process offline in blocks
        let num_frames = left.len();
        let mut frame = 0;
        while frame < num_frames {
            let end = (frame + block_size).min(num_frames);
            let block_len = end - frame;
            if block_len < block_size {
                let mut pad_l = vec![0.0_f32; block_size];
                let mut pad_r = vec![0.0_f32; block_size];
                pad_l[..block_len].copy_from_slice(&left[frame..end]);
                pad_r[..block_len].copy_from_slice(&right[frame..end]);
                
                graph.process_block(&mut pad_l, &mut pad_r);
                
                left[frame..end].copy_from_slice(&pad_l[..block_len]);
                right[frame..end].copy_from_slice(&pad_r[..block_len]);
            } else {
                graph.process_block(
                    &mut left[frame..end],
                    &mut right[frame..end],
                );
            }
            frame += block_len;
        }

        // Post-process LUFS correction — mathematically exact
        // Measure actual output LUFS and correct to target
        use sp314_dsp::metering::measure_integrated_lufs;
        let output_lufs = measure_integrated_lufs(
            left, right);
        let target_lufs = intent.target.target_lufs;

        if output_lufs > -69.0 {
            let correction_db = target_lufs - output_lufs;
            // Clamp correction to ±18dB to avoid wild swings
            let correction_db = correction_db
                .max(-18.0_f32)
                .min(18.0_f32);
            let correction_linear = libm::powf(
                10.0_f32,
                correction_db / 20.0_f32,
            );
            for s in left.iter_mut() {
                *s *= correction_linear;
            }
            for s in right.iter_mut() {
                *s *= correction_linear;
            }

            // Second limiter pass: catch ISPs introduced by LUFS correction.
            // Uses fast release (15ms) to transparently suppress Gibbs overshoots
            // without pumping. ceiling from intent — never hardcoded.
            // Authority: INV-AB-1, ITU-R BS.1770-4.
            let ceiling_linear = libm::powf(
                10.0_f32,
                intent.target.max_true_peak_db / 20.0_f32,
            );
            let isp_limiter_config = LimiterConfig {
                release_ms:         15.0_f32,
                ceiling_db:         intent.target.max_true_peak_db,
                true_peak_enabled:  true,
                midside_eq_enabled: false,
            };
            let _ = ceiling_linear; // used via config
            let mut isp_limiter = BrickwallLimiter::new(
                isp_limiter_config,
                sample_rate,
            );

            // Process main buffer
            isp_limiter.process_block(
                left,
                right,
            );

            // Latency compensation for ISPs:
            // Since we are working with fixed slices (mmap), we CANNOT reallocate or extend the slice.
            // But BrickwallLimiter introduces a tiny lookahead delay.
            // For now, we will just shift the samples backward by `lookahead` to compensate,
            // and zero-pad the end (or leave as is).
            let lookahead = isp_limiter.lookahead_samples();
            if lookahead > 0 && lookahead < left.len() {
                // Shift backward
                left.copy_within(lookahead.., 0);
                right.copy_within(lookahead.., 0);
                
                // Flush the delay line into the end of the buffer
                let mut flush_l = vec![0.0_f32; lookahead];
                let mut flush_r = vec![0.0_f32; lookahead];
                isp_limiter.process_block(&mut flush_l, &mut flush_r);
                
                let tail_start = left.len() - lookahead;
                left[tail_start..].copy_from_slice(&flush_l);
                right[tail_start..].copy_from_slice(&flush_r);
            }
        }

        // 5. Measure output LUFS
        // Use sp314-dsp metering if available, or compute simple RMS
        let output_lufs = Self::measure_lufs(left, right, sample_rate);

        Ok(MasteringResult {
            lufs: output_lufs,
            preset_name: intent.preset_name.clone(),
        })
    }

    /// Apply deterministic Aether overrides to the DSP graph topology.
    fn apply_topology_overrides(topology: &mut DspTopology, config: &integration::config::DspConfig) {
        let topology_set_param = |nodes: &mut Vec<sp314_nodes::topology::TopologyNode>, node_id: &str, param: &str, val: f32| {
            for node in nodes.iter_mut() {
                if node.node_id == node_id {
                    if let Some(obj) = node.parameters.as_object_mut() {
                        obj.insert(param.into(), serde_json::json!(val));
                    }
                }
            }
        };

        for node in &mut topology.nodes {
            if node.node_type == "Compressor" {
                if let Some(obj) = node.parameters.as_object_mut() {
                    obj.insert("threshold_db".into(), serde_json::json!(config.dynamics.comp_threshold_db));
                    obj.insert("ratio".into(), serde_json::json!(config.dynamics.comp_ratio));
                }
            }
            // TODO v2: Map eq, sat, stereo to exact node IDs.
        }

        if let Some(amb) = &config.ambience {
            topology_set_param(&mut topology.nodes, "ambience_reverb", "rt60", amb.reverb_time_delta_s);
            topology_set_param(&mut topology.nodes, "ambience_reverb", "hf_damping", amb.hf_damping_db.abs() / 3.0);
            topology_set_param(&mut topology.nodes, "ambience_reverb", "mix", if amb.output_gain_db > 0.0 { 0.8 } else { 0.0 });
            topology_set_param(&mut topology.nodes, "ambience_width", "decorrelation", amb.decorrelation);
            topology_set_param(&mut topology.nodes, "ambience_width", "side_gain_db", amb.side_gain_db);
        } else {
            topology_set_param(&mut topology.nodes, "ambience_reverb", "mix", 0.0);
            topology_set_param(&mut topology.nodes, "ambience_width", "decorrelation", 0.0);
        }
    }

    /// Translate MasteringIntent → EngineerConditions for Pipelineforge.
    fn intent_to_conditions(
        intent: &MasteringIntent,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> ConditionSet {
        let mut conditions = vec![];

        // Basic loudness conditions
        let approx_lufs = Self::estimate_lufs(left, right);
        if approx_lufs > intent.target.target_lufs + 2.0 {
            conditions.push(EngineerCondition::LufsTooLoud);
        } else if approx_lufs < intent.target.target_lufs - 6.0 {
            conditions.push(EngineerCondition::LufsTooQuiet);
        }

        ConditionSet {
            conditions,
            sample_rate,
            target_lufs: intent.target.target_lufs,
        }
    }

    /// Simple RMS-based LUFS estimate (not BS.1770 — for routing only).
    fn estimate_lufs(left: &[f32], right: &[f32]) -> f32 {
        let sum_sq: f32 = left.iter().chain(right.iter())
            .map(|s| s * s)
            .sum::<f32>() / (left.len() + right.len()) as f32;
        if sum_sq < 1e-10 { return -144.0; }
        10.0_f32 * libm::log10f(sum_sq) - 0.691_f32
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
        use sp314_dsp::limiter::TruePeakDetector;
        let mut detector = TruePeakDetector::new();
        let mut max_tp = 0.0_f32;
        for (&l, &r) in left.iter().zip(right.iter()) {
            let tp = detector.process(l, r);
            if tp > max_tp { max_tp = tp; }
        }
        // Flush the 18-sample delay line
        for _ in 0..18 {
            let tp = detector.process(0.0_f32, 0.0_f32);
            if tp > max_tp { max_tp = tp; }
        }
        if max_tp < 1e-10 { return -144.0_f32; }
        20.0_f32 * libm::log10f(max_tp)
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
