//! Streaming DSP pipeline — chunked
//! processing via DspGraph::process_block.
//!
//! STATUS: Not yet wired to any HTTP
//! endpoint. Built for the planned
//! "2.5 Neon Canvas" instrument: live A/B
//! preview + delta visualization for the
//! Single Track Session onboarding flow
//! (vs. the Batch/Album/Episode flow, which
//! uses the offline path in dsp/mod.rs).
//!
//! Decode parity with the batch path is
//! already proven by
//! decode_streaming_matches_batch_decode_
//! exactly (handlers/decode_actor.rs).
//! DSP-graph output parity (this module vs.
//! mod.rs, same input) has NOT yet been
//! verified end-to-end — do that before
//! wiring this to a real endpoint.
//!
//! Currently only exercised by
//! bin/benchmark_streaming.rs.

use crate::decode_provider::DecodeProvider;
use lineos_corpus::scout::{SegmentBoundary, SegmentType, TimelineRouter};
use sp314_dsp::io::decode_types::DecodeChunk;
use sp314_dsp::io::wav_writer::StreamingWavWriter;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;
use std::error::Error;

// allow: 12 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
#[allow(clippy::too_many_arguments)]
pub fn run_streaming_pipeline_with_timeline(
    decoder: impl DecodeProvider,
    output_path: &str,
    topology: &DspTopology,
    block_size: usize,
    sample_rate: u32,
    boundaries: Vec<SegmentBoundary>,
    ducking_node_id: &str,
    speech_gain: f32,
    music_gain: f32,
    pre_analysis: Option<&lineos_types::pre_analysis::PreAnalysisData>,
    rx_res: std::sync::mpsc::Receiver<sp314_dsp::stft::stem_renderer::NmfResult>,
    flagged_indices: Vec<usize>,
) -> Result<(), Box<dyn Error>> {
    let mut graph = DspGraph::from_topology(topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut writer = StreamingWavWriter::new(output_path, sample_rate)?;
    let router = TimelineRouter::new(boundaries.clone());

    let mut vb = sp314_nodes::topology::DspTopologyBuilder::new("vocal_graph_topology");
    let v_in = vb.add_node("in", "Input", serde_json::json!({}));
    let v_deesser = vb.add_node(
        "deesser",
        "DeEsser",
        serde_json::json!({ "threshold_db": 0.0, "frequency_hz": 6000.0 }),
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

        for i in 0..8 {
            let node_id = format!("ltass_band_{}", i);
            vocal_graph
                .set_node_parameter_no_glide(&node_id, "gain_db", ref_gains[i])
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
                        for i in 0..available_stem_frames {
                            let idx = local_start_frame + i;
                            m_bl[i] = stems.drums[idx] + stems.bass[idx] + stems.harmonics[idx] + stems.ambience[idx];
                        }
                        m_bl.resize(block_size, 0.0);
                        let mut m_br = m_bl.clone();



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
            writer.write_chunk(&bl[..valid_frames], &br[..valid_frames])?;
            total_frames_processed += valid_frames;
        }
        Ok(())
    })
    .map_err(|e| format!("{:?}", e))?;

    writer.finalize()?;
    Ok(())
}
