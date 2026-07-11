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

use crate::dsp::beat_detector::BeatDetector;
use crate::handlers::decode_actor::{decode_streaming, DecodeChunk};
use lineos_corpus::scout::{SegmentBoundary, SegmentType, TimelineRouter};
use sp314_dsp::io::wav_writer::StreamingWavWriter;
use sp314_nodes::graph::DspGraph;
use sp314_nodes::topology::DspTopology;
use std::error::Error;

pub fn run_streaming_pipeline(
    input_path: &str,
    output_path: &str,
    topology: &DspTopology,
    block_size: usize,
    sample_rate: u32,
) -> Result<(), Box<dyn Error>> {
    let mut graph = DspGraph::from_topology(topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut writer = StreamingWavWriter::new(output_path, sample_rate)?;

    // Accumulator buffers — collect incoming decoder chunks until we have
    // exactly block_size frames, then process+write, repeat. Symphonia packet
    // sizes don't align to block_size, so this buffering is necessary (the
    // existing DspAdapter batch loop sidesteps this by having the WHOLE file
    // available to slice arbitrarily — we don't have that luxury here).
    let mut acc_left: Vec<f32> = Vec::with_capacity(block_size * 2);
    let mut acc_right: Vec<f32> = Vec::with_capacity(block_size * 2);

    let (_, _) = decode_streaming(input_path, |chunk| -> Result<(), Box<dyn Error>> {
        match chunk {
            DecodeChunk::Samples(interleaved) => {
                // de-interleave into acc_left/acc_right
                for frame in interleaved.chunks_exact(2) {
                    acc_left.push(frame[0]);
                    acc_right.push(frame[1]);
                }
                // drain full block_size chunks as they accumulate
                while acc_left.len() >= block_size {
                    let mut bl: Vec<f32> = acc_left.drain(..block_size).collect();
                    let mut br: Vec<f32> = acc_right.drain(..block_size).collect();
                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl, &br)?;
                }
            }
            DecodeChunk::EndOfStream => {
                // flush remaining partial block, same zero-pad-then-trim pattern
                // as the existing DspAdapter batch loop
                if !acc_left.is_empty() {
                    let remaining = acc_left.len();
                    let mut bl = acc_left.clone();
                    let mut br = acc_right.clone();
                    bl.resize(block_size, 0.0);
                    br.resize(block_size, 0.0);
                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl[..remaining], &br[..remaining])?;

                    acc_left.clear();
                    acc_right.clear();
                }
            }
        }
        Ok(())
    })
    .map_err(|e| format!("{:?}", e))?;

    writer.finalize()?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BeatAnalysisResult {
    pub bpm: f32,
    pub beats_ms: Vec<u32>,
    pub downbeats_ms: Vec<u32>,
    pub transients_ms: Vec<u32>,
}

pub fn run_streaming_pipeline_with_scout(
    input_path: &str,
    output_path: &str,
    topology: &DspTopology,
    block_size: usize,
    sample_rate: u32,
) -> Result<BeatAnalysisResult, Box<dyn Error>> {
    let mut graph = DspGraph::from_topology(topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut writer = StreamingWavWriter::new(output_path, sample_rate)?;
    let beat_detector = BeatDetector::new(sample_rate);

    let mut acc_left: Vec<f32> = Vec::with_capacity(block_size * 2);
    let mut acc_right: Vec<f32> = Vec::with_capacity(block_size * 2);
    let mut mono_accumulator: Vec<f32> = Vec::new(); // ~57MB max for 5min track @ 48kHz

    let (_, _) = decode_streaming(input_path, |chunk| -> Result<(), Box<dyn Error>> {
        match chunk {
            DecodeChunk::Samples(interleaved) => {
                for frame in interleaved.chunks_exact(2) {
                    acc_left.push(frame[0]);
                    acc_right.push(frame[1]);
                    mono_accumulator.push((frame[0] + frame[1]) * 0.5); // mono downmix, streamed
                }
                while acc_left.len() >= block_size {
                    let mut bl: Vec<f32> = acc_left.drain(..block_size).collect();
                    let mut br: Vec<f32> = acc_right.drain(..block_size).collect();
                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl, &br)?;
                }
            }
            DecodeChunk::EndOfStream => {
                if !acc_left.is_empty() {
                    let remaining = acc_left.len();
                    let mut bl = acc_left.clone();
                    let mut br = acc_right.clone();
                    bl.resize(block_size, 0.0);
                    br.resize(block_size, 0.0);
                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl[..remaining], &br[..remaining])?;

                    acc_left.clear();
                    acc_right.clear();
                }
            }
        }
        Ok(())
    })
    .map_err(|e| format!("{:?}", e))?;

    writer.finalize()?;

    // Run BeatDetector on the full accumulated mono track
    let (bpm, beats_ms, downbeats_ms, transients_ms) = beat_detector.analyze(&mono_accumulator);

    Ok(BeatAnalysisResult {
        bpm,
        beats_ms,
        downbeats_ms,
        transients_ms,
    })
}

pub fn run_streaming_pipeline_with_timeline(
    input_path: &str,
    output_path: &str,
    topology: &DspTopology,
    block_size: usize,
    sample_rate: u32,
    boundaries: Vec<SegmentBoundary>,
    ducking_node_id: &str,
    speech_gain: f32,
    music_gain: f32,
) -> Result<(), Box<dyn Error>> {
    let mut graph = DspGraph::from_topology(topology, block_size, sample_rate)
        .map_err(|e| format!("{:?}", e))?;
    let mut writer = StreamingWavWriter::new(output_path, sample_rate)?;
    let router = TimelineRouter::new(boundaries);

    let mut acc_left: Vec<f32> = Vec::with_capacity(block_size * 2);
    let mut acc_right: Vec<f32> = Vec::with_capacity(block_size * 2);

    let mut total_frames_processed: usize = 0;
    let mut last_type: Option<SegmentType> = None;

    let (_, _) = decode_streaming(input_path, |chunk| -> Result<(), Box<dyn Error>> {
        match chunk {
            DecodeChunk::Samples(interleaved) => {
                for frame in interleaved.chunks_exact(2) {
                    acc_left.push(frame[0]);
                    acc_right.push(frame[1]);
                }
                while acc_left.len() >= block_size {
                    let mut bl: Vec<f32> = acc_left.drain(..block_size).collect();
                    let mut br: Vec<f32> = acc_right.drain(..block_size).collect();

                    let block_time_sec = total_frames_processed as f32 / sample_rate as f32;
                    if let Some(seg_type) = router.get_segment_type_at(block_time_sec) {
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
                    }

                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl, &br)?;
                    total_frames_processed += block_size;
                }
            }
            DecodeChunk::EndOfStream => {
                if !acc_left.is_empty() {
                    let remaining = acc_left.len();
                    let mut bl = acc_left.clone();
                    let mut br = acc_right.clone();
                    bl.resize(block_size, 0.0);
                    br.resize(block_size, 0.0);

                    let block_time_sec = total_frames_processed as f32 / sample_rate as f32;
                    if let Some(seg_type) = router.get_segment_type_at(block_time_sec) {
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
                    }

                    graph.process_block(&mut bl, &mut br);
                    writer.write_chunk(&bl[..remaining], &br[..remaining])?;
                    total_frames_processed += remaining;

                    acc_left.clear();
                    acc_right.clear();
                }
            }
        }
        Ok(())
    })
    .map_err(|e| format!("{:?}", e))?;

    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::decode::decode_raw_interleaved;
    use serde_json::json;
    use sp314_nodes::topology::DspTopology;

    #[test]
    fn streaming_pipeline_matches_batch_graph_processing() {
        let topology_json = json!({
            "topology_id": "streaming_validation_test",
            "nodes": [
                { "node_id": "in", "node_type": "Input", "parameters": {} },
                { "node_id": "eq", "node_type": "BiquadFilter", "parameters": { "filter_type": 0.0, "freq_hz": 1000.0 } },
                { "node_id": "out", "node_type": "Output", "parameters": {} }
            ],
            "edges": [
                { "source": "in", "target": "eq", "modulation_type": "audio" },
                { "source": "eq", "target": "out", "modulation_type": "audio" }
            ]
        }).to_string();
        let topology = DspTopology::from_json(&topology_json).unwrap();

        let input_path = "../../m1/sp314-dsp/tests/fixtures/sine_1khz_3s.wav";
        let block_size = 512;
        let sample_rate = 48000;

        let (batch_samples, _, _) = decode_raw_interleaved(input_path).unwrap();
        let mut batch_left: Vec<f32> = batch_samples.iter().step_by(2).copied().collect();
        let mut batch_right: Vec<f32> = batch_samples.iter().skip(1).step_by(2).copied().collect();

        let mut batch_graph = DspGraph::from_topology(&topology, block_size, sample_rate).unwrap();
        let num_frames = batch_left.len();
        let mut f = 0;
        while f < num_frames {
            let e = (f + block_size).min(num_frames);
            let b_len = e - f;
            if b_len < block_size {
                let mut pad_l = vec![0.0_f32; block_size];
                let mut pad_r = vec![0.0_f32; block_size];
                pad_l[..b_len].copy_from_slice(&batch_left[f..e]);
                pad_r[..b_len].copy_from_slice(&batch_right[f..e]);
                batch_graph.process_block(&mut pad_l, &mut pad_r);
                batch_left[f..e].copy_from_slice(&pad_l[..b_len]);
                batch_right[f..e].copy_from_slice(&pad_r[..b_len]);
            } else {
                batch_graph.process_block(&mut batch_left[f..e], &mut batch_right[f..e]);
            }
            f += b_len;
        }

        let streaming_output = "/tmp/test_streaming_validation_output.wav";
        run_streaming_pipeline(
            input_path,
            streaming_output,
            &topology,
            block_size,
            sample_rate,
        )
        .unwrap();

        let (stream_interleaved, _, _) = decode_raw_interleaved(streaming_output).unwrap();
        let stream_left: Vec<f32> = stream_interleaved.iter().step_by(2).copied().collect();
        let _stream_right: Vec<f32> = stream_interleaved
            .iter()
            .skip(1)
            .step_by(2)
            .copied()
            .collect();

        assert!(
            (batch_left.len() as isize - stream_left.len() as isize).abs() <= block_size as isize,
            "frame count must match (allow 1 block diff)"
        );

        std::fs::remove_file(streaming_output).ok();
    }

    #[test]
    fn streaming_scout_matches_batch_beat_detector() {
        let topology_json = json!({
            "topology_id": "scout_validation_test",
            "nodes": [
                { "node_id": "in", "node_type": "Input", "parameters": {} },
                { "node_id": "eq", "node_type": "BiquadFilter", "parameters": { "filter_type": 0.0, "freq_hz": 1000.0 } },
                { "node_id": "out", "node_type": "Output", "parameters": {} }
            ],
            "edges": [
                { "source": "in", "target": "eq", "modulation_type": "audio" },
                { "source": "eq", "target": "out", "modulation_type": "audio" }
            ]
        }).to_string();
        let topology = DspTopology::from_json(&topology_json).unwrap();

        let input_path = "../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav";

        // BATCH baseline: decode όλο το αρχείο, downmix σε mono, analyze απευθείας
        let (batch_interleaved, sr, _) = decode_raw_interleaved(input_path).unwrap();
        let batch_mono: Vec<f32> = batch_interleaved
            .chunks_exact(2)
            .map(|f| (f[0] + f[1]) * 0.5)
            .collect();
        let batch_detector = BeatDetector::new(sr);
        let (batch_bpm, batch_beats, _, _) = batch_detector.analyze(&batch_mono);

        // STREAMING
        let output_path = "/tmp/test_scout_output.wav";
        let result =
            run_streaming_pipeline_with_scout(input_path, output_path, &topology, 512, sr).unwrap();

        assert!(
            (result.bpm - batch_bpm).abs() < 0.01,
            "BPM must match: batch={} stream={}",
            batch_bpm,
            result.bpm
        );
        assert_eq!(
            result.beats_ms, batch_beats,
            "Beat timestamps must match exactly"
        );

        std::fs::remove_file(output_path).ok();
    }

    #[test]
    #[ignore]
    fn streaming_pipeline_ducking_e2e() {
        use crate::dsp::pass1_pipeline::build_timeline_map;
        let topology_json = json!({
            "topology_id": "ducking_test",
            "nodes": [
                { "node_id": "in", "node_type": "Input", "parameters": {} },
                { "node_id": "duck_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 300.0 } },
                { "node_id": "out", "node_type": "Output", "parameters": {} }
            ],
            "edges": [
                { "source": "in", "target": "duck_gain", "modulation_type": "audio" },
                { "source": "duck_gain", "target": "out", "modulation_type": "audio" }
            ]
        }).to_string();
        let topology = DspTopology::from_json(&topology_json).unwrap();

        let input_path = "../../../flight_clips_stereo/clip_transition_st.wav";
        let boundaries = build_timeline_map(input_path).unwrap();

        let output_path = "/tmp/test_streaming_ducking_output.wav";

        let bad_run = run_streaming_pipeline_with_timeline(
            input_path,
            output_path,
            &topology,
            512,
            48000,
            boundaries.clone(),
            "invalid_node_id",
            1.0,
            0.501,
        );
        assert!(
            bad_run.is_err(),
            "Expected error when passing an invalid node ID"
        );
        assert!(
            bad_run.unwrap_err().to_string().contains("invalid_node_id"),
            "Error string should mention the invalid node ID"
        );

        run_streaming_pipeline_with_timeline(
            input_path,
            output_path,
            &topology,
            512,
            48000,
            boundaries,
            "duck_gain",
            1.0,
            0.501,
        )
        .unwrap();

        // Decode the output and compute RMS!
        let (out_samples, sr, channels) = decode_raw_interleaved(output_path).unwrap();
        assert_eq!(channels, 2);
        let out_left: Vec<f32> = out_samples.iter().step_by(2).copied().collect();

        let (in_samples, _, _) = decode_raw_interleaved(input_path).unwrap();
        let in_left: Vec<f32> = in_samples.iter().step_by(2).copied().collect();

        let compute_rms = |samples: &[f32], start_sec: f32, end_sec: f32| -> f32 {
            let start_idx = (start_sec * sr as f32) as usize;
            let end_idx = (end_sec * sr as f32) as usize;
            let slice = &samples[start_idx..end_idx];
            let sq_sum: f32 = slice.iter().map(|&x| x * x).sum();
            (sq_sum / slice.len() as f32).sqrt()
        };

        // Use safe boundaries, clip is ~29s
        let in_speech_db = 20.0 * compute_rms(&in_left, 1.0, 14.0).log10();
        let out_speech_db = 20.0 * compute_rms(&out_left, 1.0, 14.0).log10();

        let in_music_db = 20.0 * compute_rms(&in_left, 18.0, 28.0).log10();
        let out_music_db = 20.0 * compute_rms(&out_left, 18.0, 28.0).log10();

        println!("=== E2E Pass 2 Ducking Proof ===");
        println!(
            "Speech RMS In: {:.2} dB, Out: {:.2} dB, Delta: {:.2} dB",
            in_speech_db,
            out_speech_db,
            out_speech_db - in_speech_db
        );
        println!(
            "Music RMS In:  {:.2} dB, Out: {:.2} dB, Delta: {:.2} dB",
            in_music_db,
            out_music_db,
            out_music_db - in_music_db
        );

        let speech_delta = out_speech_db - in_speech_db;
        let music_delta = out_music_db - in_music_db;

        assert!(
            speech_delta.abs() < 0.5,
            "Speech should not be ducked (got {:.2} dB delta)",
            speech_delta
        );
        assert!(
            (music_delta - (-6.0)).abs() < 1.0,
            "Music should be ducked by ~6dB (got {:.2} dB delta)",
            music_delta
        );

        std::fs::remove_file(output_path).ok();
    }
}
