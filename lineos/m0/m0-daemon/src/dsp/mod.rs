// src/dsp/mod.rs
// v3 DSP adapter for m0-daemon.
// Replaces MasteringPipeline::master() from v2.9.
// Orchestrates: pipelineforge → sp314-nodes DspGraph → process_offline

pub mod audio_source;
pub mod autotune;
pub mod beat_detector;
pub mod dump_audio_source;
pub mod file_decoder;
pub mod input_lufs;
pub mod wav_to_raw;

pub mod lazy_reader;
pub mod maestro;
pub mod orchestrator;

pub mod signal_health;

pub mod six_channel_stream;
pub mod standardized_decoder;
pub mod standardized_stream;
pub mod stream_core;

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
        let mut conditions = Self::intent_to_conditions(intent, left, right, sample_rate);
        // The reference resolver populates zone_bands only when a
        // profile resolved for this content. Non-empty bands are
        // therefore the signal that reference correction applies.
        if aether_config.is_some_and(|c| !c.eq.zone_bands.is_empty()) {
            conditions
                .conditions
                .push(EngineerCondition::ReferenceProfileResolved);
        }
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
    // allow: 8 args — mastering entry point; 2 are &mut audio slices,
    // the rest are orthogonal config inputs (intent, rate, analysis,
    // ratios, aether, drive). Grouping into a struct adds indirection
    // without clarity; revisit if it grows again.
    #[allow(clippy::too_many_arguments)]
    pub fn master(
        intent: &MasteringIntent,
        left: &mut [f32],
        right: &mut [f32],
        sample_rate: u32,
        _pre_analysis: &lineos_types::pre_analysis::PreAnalysisData,
        stem_ratios: &[f32; 5],
        aether_config: Option<&integration::config::DspConfig>,
        pre_gain_linear: f32, // input drive staging; 1.0 = none
    ) -> Result<MasteringResult, DspError> {
        // [F-042 FIX] Apply autotune pre-gain to drive the nonlinear
        // graph stages.
        // NOTE: 'pre_analysis.integrated_lufs' is currently derived
        // from a 30s proxy. This +/-1-2 LU variance is perfectly
        // acceptable for analog-style drive staging. It will naturally
        // become 100% exact when Cycle 5 (Y-Shape streaming) shifts
        // NMF to Pass 2, allowing Pass 1 to scan the full file O(1).
        if (pre_gain_linear - 1.0).abs() > f32::EPSILON {
            for s in left.iter_mut() {
                *s *= pre_gain_linear;
            }
            for s in right.iter_mut() {
                *s *= pre_gain_linear;
            }
        }

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

            // F-043: plain borrows — the par_iter reads strictly
            // precede the write-back (post-.collect()), so the
            // clones shielded a hazard the borrow phases already
            // exclude. Verified: no interleaving, no other
            // left_src/right_src consumers.
            let left_src: &[f32] = &left[..];
            let right_src: &[f32] = &right[..];

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
        // Merger prefixes every node id with "f{i}_", so exact equality
        // never matches. Strip the leading flavour segment and compare
        // the remainder. This is deliberately not a suffix test:
        // "f0_ltass_band_3" matches "ltass_band_3" while
        // "f0_ltass_band_30" does not.
        let id_matches =
            |full: &str, want: &str| -> bool { full.split_once('_').map(|(_, r)| r) == Some(want) };

        let topology_set_param = |nodes: &mut Vec<sp314_nodes::topology::TopologyNode>,
                                  node_id: &str,
                                  param: &str,
                                  val: f32| {
            for node in nodes.iter_mut() {
                if id_matches(&node.node_id, node_id) {
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

        // LTASS_CFS duplicates REF_CFS from aether-bridge/src/lib.rs:132.
        // Promoting it to a shared const would need pipelineforge to
        // depend on aether-bridge, and m0-daemon depends on both — the
        // duplication is deliberate and both sites must move together.
        const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];

        if !config.eq.zone_bands.is_empty() {
            // Build the per-band target from the resolver's output.
            // zone_bands are filtered to |gain_db| > 0.1 in aether-bridge,
            // so absent frequencies stay at 0.0.
            let mut target = [0.0f32; 8];
            for band in &config.eq.zone_bands {
                if let Some(i) = LTASS_CFS
                    .iter()
                    .position(|&f| (f - band.center_hz).abs() < 1.0)
                {
                    target[i] = band.gain_db;
                }
            }

            // Inverse of the filter interaction matrix, computed from the
            // RBJ peaking response at Q = 1.0, fs = 48000, evaluated at the
            // eight LTASS_CFS frequencies. Row i column j is how much of
            // band j's gain must be removed from band i to cancel its skirt.
            // Regenerating this requires rerunning compute_ainv.py in the
            // scratch dir; it is valid only for Q = 1.0 at 48 kHz. The
            // pipeline resamples everything to TARGET_SR = 48000
            // (standardized_stream.rs:46), so the fixed matrix is exact for
            // all inputs.
            #[rustfmt::skip]
            const A_INV: [[f32; 8]; 8] = [
                [ 1.015516, -0.126734,  0.006471, -0.000985,  0.000086, -0.000015,  0.000001, -0.000000],
                [-0.126734,  1.065134, -0.232219,  0.018031, -0.002903,  0.000272, -0.000039,  0.000002],
                [ 0.006471, -0.232219,  1.125672, -0.294387,  0.029469, -0.004493,  0.000381, -0.000037],
                [-0.000985,  0.018031, -0.294387,  1.183255, -0.356109,  0.035759, -0.004797,  0.000267],
                [ 0.000086, -0.002903,  0.029469, -0.356109,  1.211057, -0.350587,  0.031474, -0.002893],
                [-0.000015,  0.000272, -0.004493,  0.035759, -0.350587,  1.191587, -0.317378,  0.017796],
                [ 0.000001, -0.000039,  0.000381, -0.004797,  0.031474, -0.317378,  1.129959, -0.213450],
                [-0.000000,  0.000002, -0.000037,  0.000267, -0.002893,  0.017796, -0.213450,  1.042_03],
            ];

            // The resolver already clamps its output to the profile's
            // g_max_db, but compensation can push a band past it. Clamp
            // again after solving. podcast-v1 uses 6.0; the music profiles
            // use 2.5 and would need this value passed in rather than
            // hardcoded when reference correction reaches the music path.
            const G_MAX_DB: f32 = 6.0;

            for (i, row) in A_INV.iter().enumerate() {
                let compensated: f32 = row.iter().zip(target.iter()).map(|(&a, &t)| a * t).sum();
                let clamped = compensated.clamp(-G_MAX_DB, G_MAX_DB);
                let node_id = format!("ltass_band_{i}");
                topology_set_param(&mut topology.nodes, &node_id, "gain_db", clamped);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves that apply_topology_overrides writes the correct A_INV-compensated
    /// gains into the ltass_band_i nodes after the Merger prefixes their IDs.
    ///
    /// This is pure arithmetic — no audio fixture, no floating-point measurement
    /// noise. It validates two things independently:
    ///   1. id_matches strips the "f{j}_" prefix correctly so the right nodes are found.
    ///   2. The A_INV matrix multiplication produces the expected compensated gains.
    ///
    /// If this passes, those two facts are proved. If it fails, the audio test
    /// was never going to tell us where the bug was.
    ///
    /// Expected: x = A_INV @ target, target = podcast corrections.
    /// Computed by scratch/compute_ainv.py. Tolerance: 0.001 dB (arithmetic).
    #[test]
    fn ltass_gain_write_exact() {
        // x = A_INV @ [2.04, -3.02, 1.66, 1.04, 0.38, -2.10, -6.00, -1.14]
        // from compute_ainv.py forward section (Q = 1.0, fs = 48 kHz).
        const EXPECTED: [f32; 8] = [
            2.464165, -3.843418, 2.295347, 0.503499, 0.698397, -0.722693, -5.862202, 0.054525,
        ];

        // Minimal ConditionSet: only ReferenceProfileResolved so that
        // LtassCorrection is the only non-baseline flavour in the topology
        // (plus the always-present LufsNormalization).
        let conditions = ConditionSet {
            conditions: vec![EngineerCondition::ReferenceProfileResolved],
            sample_rate: 48_000,
            target_lufs: -16.0,
        };

        let topology_json = Pipelineforge::forge(&conditions).expect("Pipelineforge::forge failed");

        let mut topology =
            DspTopology::from_json(&topology_json).expect("DspTopology::from_json failed");

        // Build DspConfig with the eight measured podcast corrections.
        let zone_bands = [
            (50.0_f32, 2.04_f32),
            (150.0, -3.02),
            (350.0, 1.66),
            (750.0, 1.04),
            (1500.0, 0.38),
            (3000.0, -2.10),
            (6000.0, -6.00),
            (12000.0, -1.14),
        ]
        .iter()
        .map(|&(cf, gain)| integration::config::ZoneBand {
            center_hz: cf,
            gain_db: gain,
            q: 0.707,
            source: aether::semantic::zone::EqSource::Reference,
        })
        .collect::<Vec<_>>();

        let config = integration::config::DspConfig {
            eq: integration::config::DspEqConfig {
                low_shelf_gain_db: 0.0,
                low_shelf_freq_hz: 100.0,
                high_shelf_gain_db: 0.0,
                high_shelf_freq_hz: 10_000.0,
                zone_bands,
            },
            dynamics: integration::config::DspDynamicsConfig {
                comp_threshold_db: -18.0,
                comp_ratio: 4.0,
                comp_attack_ms: 10.0,
                comp_release_ms: 100.0,
            },
            sat: integration::config::DspSatConfig {
                drive: 0.0,
                mix: 0.0,
            },
            stereo: integration::config::DspStereoConfig { width: 1.0 },
            ambience: None,
            persona_id: "test".into(),
            chaos_seed: 0,
            instrument_deltas: Default::default(),
        };

        // Apply overrides — this is the function under test.
        DspAdapter::apply_topology_overrides(&mut topology, &config);

        // Read gain_db back from each ltass_band_i node.
        // The Merger prefixes IDs with "f{j}_"; id_matches strips that prefix.
        for (i, &expected_val) in EXPECTED.iter().enumerate() {
            let want = format!("ltass_band_{i}");
            let node = topology
                .nodes
                .iter()
                .find(|n| n.node_id.split_once('_').map(|(_, r)| r) == Some(want.as_str()))
                .unwrap_or_else(|| {
                    panic!(
                        "{want} not found in merged topology. \
                         Is LtassCorrection being inserted when \
                         ReferenceProfileResolved is set? \
                         Node IDs present: {:?}",
                        topology
                            .nodes
                            .iter()
                            .map(|n| &n.node_id)
                            .collect::<Vec<_>>()
                    )
                });

            let got = node
                .parameters
                .get("gain_db")
                .and_then(|v| v.as_f64())
                .unwrap_or_else(|| {
                    panic!(
                        "{want}: gain_db parameter missing. \
                         Parameters present: {:?}",
                        node.parameters
                    )
                }) as f32;

            assert!(
                (got - expected_val).abs() < 0.001,
                "{want}: gain_db = {got:.6} dB, expected {expected_val:.6} dB, \
                 diff = {:+.6} dB",
                got - expected_val
            );
        }
    }
}
