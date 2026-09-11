//! Streaming DSP pipeline — chunked
//! processing via DspGraph::process_block.
//!
//! STATUS: LIVE. Wired to POST /master/streaming endpoint.
//! ⚠ ΔΙΟΡΘΩΣΗ 2026-08-25: prior comment said "Not yet wired to any HTTP endpoint".
//! ΨΕΥΔΕΣ — active via POST /master/streaming (and batch). Misled three separate
//! recons on 2026-08-24 before measured by running code. Callers: executor.rs,
//! and streaming mode from CLI entry point.
//!
//! Decode parity with the batch path is
//! already proven by
//! decode_streaming_matches_batch_decode_
//! exactly (handlers/decode_actor.rs).
//! DSP-graph output parity (this module vs.
//! mod.rs, same input) has NOT yet been
//! verified end-to-end — do that before
//! the implementation ships.
//!
//! NOTE: bin/benchmark_streaming.rs reference is stale;
//! that file no longer exists in the tree.

use crate::decode_provider::DecodeProvider;
use lineos_corpus::scout::{SegmentBoundary, SegmentType, TimelineRouter};
use sp314_dsp::io::decode_types::DecodeChunk;
use sp314_dsp::io::wav_writer::StreamingWavWriter;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;
use std::error::Error;

/// DSP graph + gain configuration for a streaming run.
pub struct StreamingConfig<'a> {
    pub topology: &'a DspTopology,
    pub block_size: usize,
    pub sample_rate: u32,
    pub ducking_node_id: &'a str,
    pub speech_gain: f32,
    pub music_gain: f32,
    /// Pre-render gain (linear, not dB) computed from a
    /// full-file input LUFS measurement — the missing v3
    /// counterpart to v2's autotune pre-gain. 1.0 = unity.
    /// APPLIED AT TWO SITES (see apply_pre_gain call sites
    /// below) because the fallback and dual-graph paths draw
    /// from structurally separate buffers — the dual-graph
    /// stems come from an independent NMF worker reading the
    /// file directly, not through this function's decoder.
    /// If you add a new DSP path here, it MUST also call
    /// apply_pre_gain on its input before any graph processing.
    pub pre_gain_linear: f32,
    /// Exact expected output frame count (from the decoder's
    /// StandardizedAudioStream, if resampling occurred) — anything
    /// written beyond this many frames is resampler padding/tail
    /// artifact, not real audio. None = no cap (trust whatever the
    /// stream produces, today's pre-fix behavior — passthrough
    /// files with no resampling have no artifact to trim anyway).
    pub expected_output_frames: Option<u64>,
    /// Full-file noise floor from the trunk (Y1). None when
    /// unmeasured. Y4-b's adaptive gate reads it; the -45 dBFS
    /// default policy lives here in the orchestrator, never in
    /// the DSP core.
    pub noise_floor_dbfs: Option<f32>,
    /// Bypass flag for A/B testing (Y4-c TODO: plumb to UI)
    pub restoration_enabled: bool,
}

/// Multiply left/right buffers by a linear gain, in place.
/// The single source of truth for pre-gain application — both
/// the fallback and dual-graph paths call this so the logic
/// never diverges even if one path's surrounding code changes.
fn apply_pre_gain(left: &mut [f32], right: &mut [f32], gain: f32) {
    if (gain - 1.0).abs() < f32::EPSILON {
        return; // unity gain, skip the pass entirely
    }
    for s in left.iter_mut() {
        *s *= gain;
    }
    for s in right.iter_mut() {
        *s *= gain;
    }
}

/// Pre-analysis products that drive segment routing.
pub struct TimelinePlan<'a> {
    pub boundaries: Vec<SegmentBoundary>,
    pub flagged_indices: Vec<usize>,
    pub pre_analysis: Option<&'a lineos_types::pre_analysis::PreAnalysisData>,
}

/// Returns the total number of per-channel frames written.
pub fn run_streaming_pipeline_with_timeline(
    decoder: impl DecodeProvider,
    output_path: &str,
    config: &StreamingConfig<'_>,
    plan: TimelinePlan<'_>,
    rx_res: std::sync::mpsc::Receiver<sp314_dsp::stft::stem_renderer::NmfResult>,
) -> Result<usize, Box<dyn Error>> {
    let topology = config.topology;
    let block_size = config.block_size;
    let sample_rate = config.sample_rate;
    let ducking_node_id = config.ducking_node_id;
    let speech_gain = config.speech_gain;
    let music_gain = config.music_gain;
    let boundaries = plan.boundaries;
    let flagged_indices = plan.flagged_indices;
    let pre_analysis = plan.pre_analysis;
    let mut graph = DspGraph::from_topology(topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut writer = StreamingWavWriter::new(output_path, sample_rate)?;
    let router = TimelineRouter::new(boundaries.clone());

    let mut vb = sp314_nodes::topology::DspTopologyBuilder::new("vocal_graph_topology");
    let v_in = vb.add_node("in", "Input", serde_json::json!({}));
    // HEARD 2026-08-24 — Α/Β/Α, αγγλικά + γερμανικά.
    // −24 dB· στα −18/−12 μεγεθύνεται η αντήχηση της
    // ηχογράφησης: τα sibilants διεγείρουν την ουρά
    // του χώρου, ο κόμβος δεν την προσθέτει, την
    // αποκαλύπτει.
    // ΗΤΑΝ 0.0, που σκότωνε το default του κόμβου
    // (−24) — η μείωση ενεργοποιείται μόνο πάνω από
    // το κατώφλι, άρα ποτέ.
    // ΔΗΛΩΜΕΝΟ ΟΡΙΟ: ίδιο threshold, άλλη
    // επιθετικότητα ανά στάθμη αρχείου.
    let v_deesser = vb.add_node(
        "deesser",
        "DeEsser",
        serde_json::json!({ "threshold_db": -24.0, "frequency_hz": 6000.0 }),
    );
    let v_eq0 = vb.add_node(
        "ltass_band_0",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 50.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq1 = vb.add_node(
        "ltass_band_1",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 150.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq2 = vb.add_node(
        "ltass_band_2",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 350.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq3 = vb.add_node(
        "ltass_band_3",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 750.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq4 = vb.add_node(
        "ltass_band_4",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 1500.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq5 = vb.add_node(
        "ltass_band_5",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 3000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq6 = vb.add_node(
        "ltass_band_6",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 6000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_eq7 = vb.add_node(
        "ltass_band_7",
        "BiquadFilter",
        serde_json::json!({ "freq_hz": 12000.0, "q": 0.707, "filter_type": 3.0, "gain_db": 0.0 }),
    );
    let v_gain = vb.add_node(
        "vca_gain",
        "Gain",
        serde_json::json!({ "gain": 1.0, "glide_ms": 10.0 }),
    );
    let v_out = vb.add_node("out", "Output", serde_json::json!({}));

    vb.connect(&v_in, &v_deesser);
    vb.connect(&v_deesser, &v_eq0);
    vb.connect(&v_eq0, &v_eq1);
    vb.connect(&v_eq1, &v_eq2);
    vb.connect(&v_eq2, &v_eq3);
    vb.connect(&v_eq3, &v_eq4);
    vb.connect(&v_eq4, &v_eq5);
    vb.connect(&v_eq5, &v_eq6);
    vb.connect(&v_eq6, &v_eq7);
    vb.connect(&v_eq7, &v_gain);
    vb.connect(&v_gain, &v_out);
    let vocal_topology = vb.build();

    let mut mb = sp314_nodes::topology::DspTopologyBuilder::new("vca_bus_topology");
    let m_in = mb.add_node("in", "Input", serde_json::json!({}));
    let m_gain = mb.add_node(
        "vca_gain",
        "Gain",
        serde_json::json!({ "gain": 1.0, "glide_ms": 10.0 }),
    );
    let m_widener = mb.add_node(
        "widener",
        "Width",
        serde_json::json!({ "decorrelation": 0.0, "side_gain_db": 0.0, "mono_comp_shelf_db": 0.0 }),
    );
    let m_out = mb.add_node("out", "Output", serde_json::json!({}));

    mb.connect(&m_in, &m_gain);
    mb.connect(&m_gain, &m_widener);
    mb.connect(&m_widener, &m_out);
    let music_topology = mb.build();
    let mut vocal_graph = DspGraph::from_topology(&vocal_topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut music_graph = DspGraph::from_topology(&music_topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;

    // The -45 default policy lives HERE, in the orchestrator, never in the DSP core.
    let gate_threshold_db = config.noise_floor_dbfs.unwrap_or(-45.0);
    let mut rest_chain = sp314_dsp::restoration::RestorationChain::new(
        sample_rate as f32,
        sp314_dsp::restoration::RestorationConfig::voice(),
        0.0,
        gate_threshold_db,
    ); // 0.0 pad because streaming does not pre-pad

    // Precompute LTASS gains for the vocal graph ONCE for the whole file
    if let Some(pre) = pre_analysis {
        let profile = aether_bridge::reference_resolver::ReferenceProfile::load(
            aether_bridge::reference_resolver::ProfileId::PodcastV1,
        );
        let n = profile.normalization_band_count;
        let raw_profile = &pre.spectral_profile_db;
        let speech_mean: f32 = raw_profile[..n].iter().sum::<f32>() / n as f32;
        let normalized_profile: [f32; 8] = std::array::from_fn(|k| raw_profile[k] - speech_mean);

        let ref_gains = aether_bridge::reference_resolver::ReferenceResolver::resolve(
            &normalized_profile,
            &profile,
        );

        for (i, &gain) in ref_gains.iter().enumerate() {
            let node_id = format!("ltass_band_{}", i);
            vocal_graph
                .set_node_parameter_no_glide(&node_id, "gain_db", gain)
                .map_err(|e| format!("Failed to set LTASS gain: {:?}", e))?;
        }
    }

    let flagged_hybrid_indices: std::collections::HashSet<usize> =
        flagged_indices.into_iter().collect();

    let mut jit_cache: std::collections::HashMap<usize, sp314_dsp::stft::stem_renderer::FiveStems> =
        std::collections::HashMap::new();
    let mut failed_segments: std::collections::HashSet<usize> = std::collections::HashSet::new();

    let mut acc_left: Vec<f32> = Vec::with_capacity(block_size * 2);
    let mut acc_right: Vec<f32> = Vec::with_capacity(block_size * 2);

    let mut total_frames_processed: usize = 0;
    let mut last_type: Option<SegmentType> = None;
    let mut last_idx: Option<usize> = None;

    let (_, _) = decoder.stream_to(|chunk| -> Result<(), Box<dyn Error>> {
        let is_eof = matches!(chunk, DecodeChunk::EndOfStream);
        if let DecodeChunk::Samples(interleaved) = chunk {
            for frame in interleaved.chunks_exact(2) {
                acc_left.push(frame[0]);
                acc_right.push(frame[1]);
            }
        }
        loop {
            let (mut bl, mut br, valid_frames) = if acc_left.len() >= block_size {
                let bl: Vec<f32> = acc_left.drain(..block_size).collect();
                let br: Vec<f32> = acc_right.drain(..block_size).collect();
                (bl, br, block_size)
            } else if is_eof && !acc_left.is_empty() {
                let valid = acc_left.len();
                let mut bl = acc_left.clone();
                let mut br = acc_right.clone();
                bl.resize(block_size, 0.0);
                br.resize(block_size, 0.0);
                acc_left.clear();
                acc_right.clear();
                (bl, br, valid)
            } else {
                break;
            };

            apply_pre_gain(&mut bl, &mut br, config.pre_gain_linear);

            let mut dual_graph_processed = false;
            let block_time_sec = total_frames_processed as f32 / sample_rate as f32;
            if let Some((seg_idx, seg_type)) = router.get_segment_at(block_time_sec) {
                if let Some(old_idx) = last_idx {
                    if old_idx != seg_idx {
                        jit_cache.remove(&old_idx);
                    }
                }

                if Some(seg_type) != last_type {
                    match seg_type {
                        SegmentType::Speech => {
                            graph
                                .set_node_parameter(ducking_node_id, "gain", speech_gain)
                                .map_err(|e| {
                                    Box::<dyn Error>::from(format!(
                                        "ducking_node_id '{}' invalid: {:?}",
                                        ducking_node_id, e
                                    ))
                                })?;
                            // on Music->Speech entry, reset to avoid stale-state click
                            rest_chain.reset();
                        }
                        SegmentType::Music => {
                            graph
                                .set_node_parameter(ducking_node_id, "gain", music_gain)
                                .map_err(|e| {
                                    Box::<dyn Error>::from(format!(
                                        "ducking_node_id '{}' invalid: {:?}",
                                        ducking_node_id, e
                                    ))
                                })?;
                        }
                    }
                    last_type = Some(seg_type);
                }

                if flagged_hybrid_indices.contains(&seg_idx) {
                    if !jit_cache.contains_key(&seg_idx) && !failed_segments.contains(&seg_idx) {
                        loop {
                            match rx_res.recv_timeout(std::time::Duration::from_secs(10)) {
                                Ok(result) => {
                                    let id = result.segment_id;
                                    jit_cache.insert(id, result.stems);
                                    if id == seg_idx {
                                        break;
                                    }
                                }
                                Err(_e) => {
                                    eprintln!("segment {} stems unavailable (worker error or timeout) — proceeding WITHOUT stem separation for this segment", seg_idx);
                                    failed_segments.insert(seg_idx);
                                    break;
                                }
                            }
                        }
                    }

                    if let Some(stems) = jit_cache.get(&seg_idx) {
                        let segment_start_sec = boundaries[seg_idx].start_sec;
                        let local_offset_sec = (block_time_sec - segment_start_sec).max(0.0);
                        let local_start_frame = (local_offset_sec * sample_rate as f32) as usize;
                        let local_end_frame = (local_start_frame + block_size).min(stems.voice.len());

                        let available_stem_frames = local_end_frame.saturating_sub(local_start_frame);

                        if Some(seg_idx) != last_idx {
                            println!("using local frames {}..{} of segment {} (voice.len={}, drums.len={})",
                                local_start_frame, local_end_frame, seg_idx, stems.voice.len(), stems.drums.len());
                        }

                        let mut v_bl = stems.voice[local_start_frame..local_end_frame].to_vec();
                        v_bl.resize(block_size, 0.0);
                        let mut v_br = v_bl.clone();

                        let mut m_bl = vec![0.0f32; available_stem_frames];
                        for (i, m) in m_bl.iter_mut().enumerate() {
                            let idx = local_start_frame + i;
                            *m = stems.drums[idx] + stems.bass[idx] + stems.harmonics[idx] + stems.ambience[idx];
                        }
                        m_bl.resize(block_size, 0.0);
                        let mut m_br = m_bl.clone();

                        apply_pre_gain(&mut v_bl, &mut v_br, config.pre_gain_linear);
                        apply_pre_gain(&mut m_bl, &mut m_br, config.pre_gain_linear);

                        if seg_type == SegmentType::Speech && config.restoration_enabled {
                            rest_chain.process(&mut v_bl, &mut v_br);
                        }

                        vocal_graph.process_block(&mut v_bl, &mut v_br);
                        music_graph.process_block(&mut m_bl, &mut m_br);



                        for i in 0..available_stem_frames {
                            bl[i] = v_bl[i] + m_bl[i];
                            br[i] = v_br[i] + m_br[i];
                        }
                        for i in available_stem_frames..block_size {
                            // TODO(Wave 3): Revisit this boundary when adding real EQ/Reverb nodes.
                            // Currently, v_bl[i] and m_bl[i] are exactly 0.0 here because the input was padded
                            // with zeroes and the GainNode has no memory/tail.
                            // Thus, `+= 0.0` just leaves `bl[i]` as RAW, UNPROCESSED mix audio.
                            // The single ducking `graph` is skipped for this entire block, meaning this tiny tail
                            // (at most 21ms) goes through completely un-ducked and un-processed.
                            // When decay-producing nodes are added to the dual graphs, they WILL produce non-zero
                            // tails here. Mixing them with raw audio needs a deliberate design decision at that time.
                            bl[i] += v_bl[i] + m_bl[i];
                            br[i] += v_br[i] + m_br[i];
                        }
                        dual_graph_processed = true;
                    }
                }

                last_idx = Some(seg_idx);
            }

            if !dual_graph_processed {
                graph.process_block(&mut bl, &mut br);
            }

            let frames_to_write = if let Some(cap) = config.expected_output_frames {
                let remaining = (cap as usize).saturating_sub(total_frames_processed);
                valid_frames.min(remaining)
            } else {
                valid_frames
            };
            if frames_to_write > 0 {
                writer.write_chunk(&bl[..frames_to_write], &br[..frames_to_write])?;
                total_frames_processed += frames_to_write;
            }
        }
        Ok(())
    })
    .map_err(|e| format!("{:?}", e))?;

    writer.finalize()?;
    Ok(total_frames_processed)
}
