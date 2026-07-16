// src/dsp/mod.rs
// v3 DSP adapter for m0-daemon.
// Replaces MasteringPipeline::master() from v2.9.
// Orchestrates: pipelineforge → sp314-nodes DspGraph → process_offline

pub mod audio_source;
pub mod autotune;
pub mod beat_detector;
pub mod file_decoder;

pub mod lazy_reader;
pub mod maestro;
pub mod orchestrator;

pub mod signal_health;

pub mod standardized_stream;

pub use audio_source::AudioSource;
pub use maestro::{AutoTuningController, RenderParams};

use lineos_types::{LufsReport, MasteringIntent};
use pipelineforge::conditions::{ConditionSet, EngineerCondition};
use pipelineforge::forge::Pipelineforge;
use sp314_dsp::limiter::{BrickwallLimiter, LimiterConfig};
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;

pub struct DspAdapter;

impl DspAdapter {
    /// Build a configured DspGraph without
    /// running the audio loop. Used by the
    /// Episode streaming render path, which
    /// drives process_block() chunk-by-chunk
    /// itself.
    ///
    /// `left`/`right` should be the scout
    /// sample (e.g. 30s) — used only to
    /// derive EngineerConditions (crest,
    /// dynamics). The returned graph is
    /// stateful and ready for streaming.
    pub fn build_graph_only(
        intent: &MasteringIntent,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        stem_ratios: &[f32; 5],
        aether_config: Option<&integration::config::DspConfig>,
        block_size: usize,
    ) -> Result<sp314_nodes::graph::DspGraph, DspError> {
        let conditions = Self::intent_to_conditions(intent, left, right, sample_rate);
        let topology_json = Pipelineforge::forge(&conditions)
            .map_err(|e| DspError::ForgeError(format!("{:?}", e)))?;
        let mut topology = DspTopology::from_json(&topology_json)
            .map_err(|e| DspError::TopologyError(format!("{:?}", e)))?;
        if let Some(config) = aether_config {
            Self::apply_topology_overrides(&mut topology, config);
        }
        let mut graph = DspGraph::from_topology(&topology, block_size, sample_rate)
            .map_err(|e| DspError::TopologyError(format!("graph build: {:?}", e)))?;
        graph.update_features(stem_ratios);
        Ok(graph)
    }

    /// Master a stereo buffer using v3 engine.
    /// Replaces MasteringPipeline::master().
    pub fn master(
        intent: &MasteringIntent,
        left: &mut [f32],
        right: &mut [f32],
        sample_rate: u32,
        _pre_analysis: &lineos_types::pre_analysis::PreAnalysisData,
        stem_ratios: &[f32; 5],
        aether_config: Option<&integration::config::DspConfig>,
    ) -> Result<MasteringResult, DspError> {
        // Build the configured graph.
        // Shared with the Episode streaming
        // path via build_graph_only() — one
        // source of truth for construction.
        let block_size = 512;
        let num_frames = left.len();
        use rayon::prelude::*;

        // Phase 1: Serial Graph Processing.
        // The DspGraph contains heavily
        // stateful nodes (Compressor, Reverb)
        // that cannot be cleanly parallelized
        // without massive margins.
        let mut graph = Self::build_graph_only(
            intent,
            left,
            right,
            sample_rate,
            stem_ratios,
            aether_config,
            block_size,
        )?;
        let mut f = 0;
        while f < num_frames {
            let e = (f + block_size).min(num_frames);
            let b_len = e - f;
            if b_len < block_size {
                let mut pad_l = vec![0.0_f32; block_size];
                let mut pad_r = vec![0.0_f32; block_size];
                pad_l[..b_len].copy_from_slice(&left[f..e]);
                pad_r[..b_len].copy_from_slice(&right[f..e]);
                graph.process_block(&mut pad_l, &mut pad_r);
                left[f..e].copy_from_slice(&pad_l[..b_len]);
                right[f..e].copy_from_slice(&pad_r[..b_len]);
            } else {
                graph.process_block(&mut left[f..e], &mut right[f..e]);
            }
            f += b_len;
        }

        // --- BISECT TRAPS AFTER DSP GRAPH ---
        #[cfg(debug_assertions)]
        {
            let frames = graph.debug_frames.max(1) as f64;
            let mut idx = 1;
            for node_id in &graph.execution_order {
                if let (Some(&sq_l), Some(&sq_r)) =
                    (graph.debug_sq_l.get(node_id), graph.debug_sq_r.get(node_id))
                {
                    let rms_l = (sq_l / frames).sqrt();
                    let rms_r = (sq_r / frames).sqrt();
                    let ratio = rms_r / rms_l.max(1e-9);
                    eprintln!(
                        "[BISECT-5.{}-GRAPH-NODE] node_id={} L_rms={:.6} R_rms={:.6} ratio={:.4}",
                        idx, node_id, rms_l, rms_r, ratio
                    );
                    idx += 1;
                }
            }
        }

        // Post-process LUFS correction — mathematically exact
        // Measure actual output LUFS and correct to target
        use sp314_dsp::metering::measure_integrated_lufs;
        let output_lufs = measure_integrated_lufs(left, right);
        let target_lufs = intent.target.target_lufs;

        if output_lufs > -69.0 {
            let correction_db = target_lufs - output_lufs;
            let mut correction_db = correction_db.clamp(-18.0_f32, 18.0_f32);

            // Headroom-Aware LUFS Makeup.
            // If projected peak would force the ISP
            // Limiter to do GR > max_limiter_gr_db,
            // cap correction_db to preserve transients.
            // Trades LUFS accuracy for punch quality.
            // max_limiter_gr_db set by Control Plane —
            // DSP reads blindly (Separation of Concerns).
            let peak_raw_db = {
                let max_l = left.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
                let max_r = right.iter().map(|s| s.abs()).fold(0.0_f32, f32::max);
                let p = max_l.max(max_r);
                if p > 1e-9 {
                    20.0 * libm::log10f(p)
                } else {
                    -144.0
                }
            };
            let projected_peak = peak_raw_db + correction_db;
            let ceiling_db = intent.target.max_true_peak_db;
            if projected_peak > ceiling_db + intent.max_limiter_gr_db {
                correction_db = ceiling_db + intent.max_limiter_gr_db - peak_raw_db;
            }

            let correction_linear = libm::powf(10.0_f32, correction_db / 20.0_f32);

            let ceiling_linear = libm::powf(10.0_f32, intent.target.max_true_peak_db / 20.0_f32);

            let isp_limiter_config = LimiterConfig {
                release_ms: 15.0_f32,
                // DSP reads pre-computed value.
                // No math here — Separation of Concerns.
                blend_release_ms: intent.limiter_blend_release_ms,
                ceiling_db: intent.target.max_true_peak_db,
                true_peak_enabled: true,
                midside_eq_enabled: false,
            };
            let _ = ceiling_linear; // used via config

            // Phase 3: Parallel Gain & ISP Limiting
            // Limiter has 15ms fast release, but 100ms slow blend release.
            // To reach float-exact (1e-7) convergence on a 100ms tau,
            // we need ~16 time constants. 16 * 100ms = 1.6s.
            let isp_margin_sec = 1.6;
            let isp_margin = (sample_rate as f32 * isp_margin_sec).round() as usize;
            let isp_chunk_sec = 10.0;
            let isp_chunk_size = (sample_rate as f32 * isp_chunk_sec).round() as usize;
            let lookahead = (sample_rate as f32 * 0.005).round() as usize;

            let mut isp_chunks = Vec::new();
            let mut start = 0;
            while start < num_frames {
                let end = (start + isp_chunk_size).min(num_frames);
                let pad_start = start.saturating_sub(isp_margin);
                let pad_end = (end + lookahead).min(num_frames);
                let pad_len = start - pad_start;
                isp_chunks.push((start, end, pad_start, pad_end, pad_len));
                start = end;
            }

            let left_src = left.to_vec();
            let right_src = right.to_vec();

            let processed_isp: Vec<(Vec<f32>, Vec<f32>)> = isp_chunks
                .par_iter()
                .map(|&(_start, end, pad_start, pad_end, pad_len)| {
                    let mut isp_limiter_clone =
                        BrickwallLimiter::new(isp_limiter_config, sample_rate);

                    let is_last_chunk = pad_end == num_frames;
                    let flush_len = if is_last_chunk { lookahead } else { 0 };
                    let total_len = pad_end - pad_start + flush_len;

                    let mut work_l = vec![0.0_f32; total_len];
                    let mut work_r = vec![0.0_f32; total_len];

                    // Apply global gain correction during the copy
                    for (i, &s) in left_src[pad_start..pad_end].iter().enumerate() {
                        work_l[i] = s * correction_linear;
                    }
                    for (i, &s) in right_src[pad_start..pad_end].iter().enumerate() {
                        work_r[i] = s * correction_linear;
                    }

                    // Process block by block (512) for the limiter
                    let mut f = 0;
                    while f < total_len {
                        let e = (f + block_size).min(total_len);
                        let b_len = e - f;
                        if b_len < block_size {
                            let mut pad_l = vec![0.0_f32; block_size];
                            let mut pad_r = vec![0.0_f32; block_size];
                            pad_l[..b_len].copy_from_slice(&work_l[f..e]);
                            pad_r[..b_len].copy_from_slice(&work_r[f..e]);
                            isp_limiter_clone.process_block(&mut pad_l, &mut pad_r);
                            work_l[f..e].copy_from_slice(&pad_l[..b_len]);
                            work_r[f..e].copy_from_slice(&pad_r[..b_len]);
                        } else {
                            isp_limiter_clone.process_block(&mut work_l[f..e], &mut work_r[f..e]);
                        }
                        f += b_len;
                    }

                    // The limiter delays audio by exactly `lookahead` samples.
                    // Output for input `i` is at `i + lookahead`.
                    // The true start of our chunk in `work` is `pad_len`.
                    // So the true start of output is `pad_len + lookahead`.
                    let out_start = pad_len + lookahead;
                    let target_len = end - _start;
                    let out_end = (out_start + target_len).min(total_len);

                    let mut final_l = vec![0.0_f32; target_len];
                    let mut final_r = vec![0.0_f32; target_len];

                    let available = out_end - out_start;
                    final_l[..available].copy_from_slice(&work_l[out_start..out_end]);
                    final_r[..available].copy_from_slice(&work_r[out_start..out_end]);

                    (final_l, final_r)
                })
                .collect();

            let mut idx = 0;
            for (out_l, out_r) in processed_isp {
                let len = out_l.len();
                left[idx..idx + len].copy_from_slice(&out_l);
                right[idx..idx + len].copy_from_slice(&out_r);
                idx += len;
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
    fn apply_topology_overrides(
        topology: &mut DspTopology,
        config: &integration::config::DspConfig,
    ) {
        let topology_set_param = |nodes: &mut Vec<sp314_nodes::topology::TopologyNode>,
                                  node_id: &str,
                                  param: &str,
                                  val: f32| {
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
                    obj.insert(
                        "threshold_db".into(),
                        serde_json::json!(config.dynamics.comp_threshold_db),
                    );
                    obj.insert(
                        "ratio".into(),
                        serde_json::json!(config.dynamics.comp_ratio),
                    );
                }
            }
            // TODO v2: Map eq, sat, stereo to exact node IDs.
        }

        if let Some(amb) = &config.ambience {
            topology_set_param(
                &mut topology.nodes,
                "ambience_reverb",
                "rt60",
                amb.reverb_time_delta_s,
            );
            topology_set_param(
                &mut topology.nodes,
                "ambience_reverb",
                "hf_damping",
                amb.hf_damping_db.abs() / 3.0,
            );
            topology_set_param(
                &mut topology.nodes,
                "ambience_reverb",
                "mix",
                if amb.output_gain_db > 0.0 { 0.8 } else { 0.0 },
            );
            topology_set_param(
                &mut topology.nodes,
                "ambience_width",
                "decorrelation",
                amb.decorrelation,
            );
            topology_set_param(
                &mut topology.nodes,
                "ambience_width",
                "side_gain_db",
                amb.side_gain_db,
            );
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
        let sum_sq: f32 = left.iter().chain(right.iter()).map(|s| s * s).sum::<f32>()
            / (left.len() + right.len()) as f32;
        if sum_sq < 1e-10 {
            return -144.0;
        }
        10.0_f32 * libm::log10f(sum_sq) - 0.691_f32
    }

    /// Measure output LUFS — uses sp314-dsp metering.
    fn measure_lufs(left: &[f32], right: &[f32], _sample_rate: u32) -> LufsReport {
        // Use sp314-dsp::metering::measure_integrated_lufs
        // Import here to avoid top-level sp314-dsp dependency in dsp/mod.rs
        use sp314_dsp::metering::measure_integrated_lufs;
        let lufs = measure_integrated_lufs(left, right);
        LufsReport {
            integrated_lufs: lufs,
            true_peak_dbfs: Self::true_peak(left, right),
            loudness_range_lu: 0.0, // LRA calculation is future scope
            short_term_lufs: None,
        }
    }

    fn true_peak(left: &[f32], right: &[f32]) -> f32 {
        use sp314_dsp::limiter::TruePeakDetector;
        let mut detector = TruePeakDetector::new();
        let mut max_tp = 0.0_f32;
        for (&l, &r) in left.iter().zip(right.iter()) {
            let tp = detector.process(l, r);
            if tp > max_tp {
                max_tp = tp;
            }
        }
        // Flush the 18-sample delay line
        for _ in 0..18 {
            let tp = detector.process(0.0_f32, 0.0_f32);
            if tp > max_tp {
                max_tp = tp;
            }
        }
        if max_tp < 1e-10 {
            return -144.0_f32;
        }
        20.0_f32 * libm::log10f(max_tp)
    }
}

pub struct MasteringResult {
    pub lufs: LufsReport,
    pub preset_name: String,
}

#[derive(Debug)]
pub enum DspError {
    ForgeError(String),
    TopologyError(String),
    GraphError(String),
    ProcessError(String),
}
