//! render_node — TwoPassEngine rendering, spatial mixing, and streaming hashes.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P3b

use crate::blob_store::StemFingerprints;
use crate::handlers::master::MixLevels;
use sha2::{Digest, Sha256};
use sp314_dsp::spatial::five_dot_one::FiveDotOneStage;
use sp314_dsp::spatial::renderer::StereoRenderer;
use sp314_dsp::stft::two_pass::{ScoutResult, TwoPassEngine};

/// Streaming 6-channel spatial dump writer.
/// Replaces SpatialSlicesMut (A4-iii): the render
/// callback writes interleaved 6ch PCM directly to
/// disk instead of filling six Vec<f32> buffers.
pub struct SpatialDumpWriter {
    writer: std::io::BufWriter<std::fs::File>,
    scratch: Vec<u8>, // bounded: chunk_len * 24, reused
    pub frames_written: usize,
}

impl SpatialDumpWriter {
    pub fn create(path: &str) -> Result<Self, String> {
        let file = std::fs::File::create(path)
            .map_err(|e| format!("SpatialDumpWriter: create {path}: {e}"))?;
        Ok(Self {
            writer: std::io::BufWriter::new(file),
            scratch: Vec::new(),
            frames_written: 0,
        })
    }

    /// Interleave stage channels frame-by-frame (L R C LFE Ls Rs)
    /// and write as explicit little-endian f32 bytes. Layout is
    /// byte-identical to write_interleaved_dump on LE platforms;
    /// on BE this is correct where the old unsafe cast would not be.
    pub fn write_stage(&mut self, stage: &FiveDotOneStage) -> Result<(), String> {
        use std::io::Write;
        let n = stage.l.len();
        self.scratch.clear();
        self.scratch.reserve(n * 24);
        for i in 0..n {
            self.scratch.extend_from_slice(&stage.l[i].to_le_bytes());
            self.scratch.extend_from_slice(&stage.r[i].to_le_bytes());
            self.scratch.extend_from_slice(&stage.c[i].to_le_bytes());
            self.scratch.extend_from_slice(&stage.lfe[i].to_le_bytes());
            self.scratch.extend_from_slice(&stage.ls[i].to_le_bytes());
            self.scratch.extend_from_slice(&stage.rs[i].to_le_bytes());
        }
        self.writer
            .write_all(&self.scratch)
            .map_err(|e| format!("SpatialDumpWriter: write: {e}"))?;
        self.frames_written += n;
        Ok(())
    }

    /// Finalize the dump: if the engine emitted fewer frames than
    /// `expected_frames`, pad with explicit zero-frames to reproduce
    /// the implicit silent tail that the old Vec<f32> allocation
    /// provided (the STFT engine does NOT emit flush-tail frames —
    /// the trailing STFT_FLUSH_TAIL zeros came from vec![0.0; n]
    /// initialization, confirmed by recon 2026-07-23).
    ///
    /// Over-emission (frames_written > expected_frames) is a hard
    /// error — that indicates real engine/data corruption, not a
    /// benign tail gap.
    pub fn finish(mut self, expected_frames: usize) -> Result<usize, String> {
        use std::io::Write;
        if self.frames_written > expected_frames {
            return Err(format!(
                "SpatialDumpWriter: over-emission: wrote {} frames but expected {}",
                self.frames_written, expected_frames
            ));
        }
        let deficit = expected_frames - self.frames_written;
        if deficit > 0 {
            // Write zero-frames in scratch-sized chunks to avoid a
            // single huge allocation. 24 bytes per frame (6 × f32 LE).
            const BYTES_PER_FRAME: usize = 24;
            let chunk_frames = self.scratch.capacity().max(BYTES_PER_FRAME) / BYTES_PER_FRAME;
            let zero_chunk_bytes = chunk_frames * BYTES_PER_FRAME;
            self.scratch.clear();
            self.scratch.resize(zero_chunk_bytes, 0u8);
            let mut remaining = deficit;
            while remaining > 0 {
                let n = remaining.min(chunk_frames);
                self.writer
                    .write_all(&self.scratch[..n * BYTES_PER_FRAME])
                    .map_err(|e| format!("SpatialDumpWriter: tail write: {e}"))?;
                remaining -= n;
            }
            self.frames_written = expected_frames;
        }
        self.writer
            .flush()
            .map_err(|e| format!("SpatialDumpWriter: flush: {e}"))?;
        Ok(self.frames_written)
    }
}

/// Non-audio render configuration for a single chunk.
pub struct RenderSettings<'a> {
    pub ducking_gain: f32,
    pub mix_levels: Option<&'a MixLevels>,
    pub flavour_id: Option<&'a str>,
    pub sample_rate: u32,
    pub noise_floor_dbfs: Option<f32>,
    pub restoration_enabled: bool,
    pub macro_router_enabled: bool,
    pub boundaries: &'a [lineos_corpus::scout::SegmentBoundary],
    pub vad_observe_enabled: bool,
    pub blob_id: &'a str,
}

/// Immutable input audio for a single chunk.
pub struct RenderInputs {
    pub original_sum_sq: f32,
    pub total_frames: usize,
    pub stream_source: sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader<
        sp314_dsp::stft::raw_pcm_source::RawPcmFileSource,
    >,
}

pub fn run(
    two_pass: &mut TwoPassEngine,
    scout: &ScoutResult,
    settings: &RenderSettings<'_>,
    inputs: RenderInputs,
    left_slice: &mut [f32],
    right_slice: &mut [f32],
    mut spatial: Option<&mut SpatialDumpWriter>,
) -> Result<(StemFingerprints, sp314_dsp::stft::two_pass::RenderMetadata), String> {
    let ducking_gain = settings.ducking_gain;
    let mix_levels = settings.mix_levels;
    let flavour_id = settings.flavour_id;
    let sample_rate = settings.sample_rate;
    let mut h_voice = Sha256::new();
    let mut h_drums = Sha256::new();
    let mut h_bass = Sha256::new();
    let mut h_harmonics = Sha256::new();
    let mut h_ambience = Sha256::new();

    let is_broadcast = flavour_id == Some("broadcast");
    let mut voice_graph_opt = if is_broadcast {
        let topo = pipelineforge::flavor::Flavor::POXVoice.build(sample_rate);
        Some(
            sp314_nodes::graph::DspGraph::from_topology(&topo, 512, sample_rate)
                .map_err(|e| format!("POX graph error: {:?}", e))?,
        )
    } else {
        None
    };

    let mix = mix_levels
        .map(|m: &crate::handlers::master::MixLevels| m.clamped())
        .unwrap_or_default();
    let mut write_offset = 0;

    // W3: Routing Attribution (known by construction)
    const STEM_ID_VOICE: usize = 0;
    const STEM_ID_DRUMS: usize = 1;
    const STEM_ID_BASS: usize = 2;
    const STEM_ID_HARMONICS: usize = 3;

    let mut routing_table = sp314_dsp::analysis::routing::RoutingTable::default();
    routing_table.assign(STEM_ID_VOICE, sp314_dsp::analysis::routing::Bus::Voice);
    routing_table.assign(STEM_ID_DRUMS, sp314_dsp::analysis::routing::Bus::Drums);
    routing_table.assign(STEM_ID_BASS, sp314_dsp::analysis::routing::Bus::Music);
    routing_table.assign(STEM_ID_HARMONICS, sp314_dsp::analysis::routing::Bus::Music);
    // ambience = glue-feed: εκτός RoutingTable κατά v2
    // (Glue = processor, not stem). Routing του glue path: W4.

    struct BusGains { voice: f32, drums: f32, music: f32 }
    let bus_gains = BusGains { voice: 1.0, drums: 1.0, music: 1.0 };

    let get_bus_gain = |bus: Option<sp314_dsp::analysis::routing::Bus>| -> f32 {
        match bus {
            Some(sp314_dsp::analysis::routing::Bus::Voice) => bus_gains.voice,
            Some(sp314_dsp::analysis::routing::Bus::Drums) => bus_gains.drums,
            Some(sp314_dsp::analysis::routing::Bus::Music) => bus_gains.music,
            None => 1.0,
        }
    };

    let effective_voice_gain = mix.voice * get_bus_gain(routing_table.bus_of(STEM_ID_VOICE));
    let effective_drums_gain = mix.drums * get_bus_gain(routing_table.bus_of(STEM_ID_DRUMS));
    let effective_bass_gain = mix.bass * get_bus_gain(routing_table.bus_of(STEM_ID_BASS));
    let effective_harmonics_gain = mix.harmonics * get_bus_gain(routing_table.bus_of(STEM_ID_HARMONICS));
    // Το mix.ambience μένει ως έχει, αφού δεν μπαίνει στον πίνακα.

    let original_sum_sq = inputs.original_sum_sq;
    // Captured error from spatial I/O inside the infallible callback.
    let mut spatial_err: Option<String> = None;

    let mut vocal_gate = sp314_dsp::restoration::gate::NoiseGate::new(
        settings.sample_rate as f32,
        0.0,
        settings.noise_floor_dbfs.unwrap_or(-45.0),
    );

    const GLUE_PAD_LINEAR: f32 = 0.125_892_54; // 10^(-18/20)
    let glue_lpf_coeffs =
        sp314_dsp::masking_eq::biquad::rbj_lowpass(6000.0, 0.707, settings.sample_rate as f64);
    let mut glue_lpf_state = sp314_dsp::masking_eq::biquad::BiquadState::default();

    let callback = |stems_chunk: &sp314_dsp::stft::two_pass::FiveStemsChunk| {
        let chunk_len = stems_chunk.voice.len();

        let mv: Vec<f32> = stems_chunk
            .voice
            .iter()
            .map(|s| {
                let mut sample = *s;
                if settings.restoration_enabled {
                    sample = vocal_gate.process_mono(sample);
                }
                sample * effective_voice_gain
            })
            .collect();
        let md: Vec<f32> = stems_chunk.drums.iter().map(|s| s * effective_drums_gain).collect();
        let mb: Vec<f32> = stems_chunk.bass.iter().map(|s| s * effective_bass_gain).collect();
        let mh: Vec<f32> = stems_chunk
            .harmonics
            .iter()
            .map(|s| s * effective_harmonics_gain)
            .collect();
        let ma: Vec<f32> = stems_chunk
            .ambience
            .iter()
            .map(|s| {
                let mut sample = *s;
                if settings.restoration_enabled {
                    sample = sp314_dsp::masking_eq::biquad::process_tdf2(
                        sample,
                        &glue_lpf_coeffs,
                        &mut glue_lpf_state,
                    ) * GLUE_PAD_LINEAR;
                }
                sample * mix.ambience
            })
            .collect();

        h_voice
            .update(unsafe { std::slice::from_raw_parts(mv.as_ptr() as *const u8, mv.len() * 4) });
        h_drums
            .update(unsafe { std::slice::from_raw_parts(md.as_ptr() as *const u8, md.len() * 4) });
        h_bass
            .update(unsafe { std::slice::from_raw_parts(mb.as_ptr() as *const u8, mb.len() * 4) });
        h_harmonics
            .update(unsafe { std::slice::from_raw_parts(mh.as_ptr() as *const u8, mh.len() * 4) });
        h_ambience
            .update(unsafe { std::slice::from_raw_parts(ma.as_ptr() as *const u8, ma.len() * 4) });

        let mut clean_voice = mv.clone();
        if let Some(ref mut vg) = voice_graph_opt {
            let mut v_right = clean_voice.clone();
            let mut frame = 0;
            while frame < chunk_len {
                let end = (frame + 512).min(chunk_len);
                vg.process_block(&mut clean_voice[frame..end], &mut v_right[frame..end]);
                frame = end;
            }
            for i in 0..chunk_len {
                clean_voice[i] = (clean_voice[i] + v_right[i]) * 0.5;
            }
        }

        let stage =
            FiveDotOneStage::render_chunk(&clean_voice, &md, &mb, &mh, &ma, &scout.assignments);
        let mut stage = stage;
        stage.apply_scales(scout.rear_scale, scout.lfe_scale);

        if let Some(ref mut sp) = spatial {
            if spatial_err.is_none() {
                if let Err(e) = sp.write_stage(&stage) {
                    spatial_err = Some(e);
                }
            }
        }

        let (sp_l, sp_r) = StereoRenderer::render(&stage);

        let end_offset = write_offset + sp_l.len();
        left_slice[write_offset..end_offset].copy_from_slice(&sp_l);
        right_slice[write_offset..end_offset].copy_from_slice(&sp_r);
        write_offset = end_offset;
    };

    let trace_path = crate::spool::spool_dir().join(format!("vad-trace-{}.csv", settings.blob_id));
    // W1 glue-feed: ControlBus observer για το κύκλωμα read-back echo
    let control_bus = std::sync::Arc::new(
        sp314_dsp::dsp::control_bus::ControlBus::new(settings.sample_rate as f32),
    );
    let mut ducker = sp314_dsp::dsp::control_bus::Ducker::new(settings.sample_rate as f32);
    let echo_path = std::path::PathBuf::from("/tmp/w1_csv_B_bus.csv");
    let _ = std::fs::remove_file(&echo_path);
    let echo_file = std::fs::File::create(&echo_path).ok();
    let mut echo_w = echo_file.map(|f| {
        use std::io::Write;
        let mut bw = std::io::BufWriter::new(f);
        let _ = writeln!(bw, "frame_index,time_sec,p_speech,voice_gate,old_duck_gain,new_duck_gain");
        bw
    });

    let duck_track: std::rc::Rc<std::cell::RefCell<Vec<f32>>> = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let duck_track_clone = duck_track.clone();

    let mut observer_holder = if settings.vad_observe_enabled {
        match std::fs::File::create(&trace_path) {
            Ok(file) => {
                let mut w = std::io::BufWriter::new(file);
                use std::io::Write;
                let _ = writeln!(w, "frame_index,time_sec,posterior,is_speech,duck_gain,rms_db,flatness,ms_ratio,rms_delta,noise_floor_dbfs");
                let sr = sample_rate as f64;
                use sp314_dsp::analysis::vad_sensors::FRAME_SAMPLES;
                Some(move |obs: sp314_dsp::analysis::vad_model::VadObservation| {
                    let time_sec = obs.frame_index as f64 * FRAME_SAMPLES as f64 / sr;
                    let _ = writeln!(
                        w,
                        "{},{:.2},{:.4},{},{:.4},{:.2},{:.4},{:.4},{:.4},{:.2}",
                        obs.frame_index,
                        time_sec,
                        obs.posterior,
                        obs.is_speech,
                        obs.duck_gain,
                        obs.rms_db,
                        obs.spectral_flatness,
                        obs.mid_side_ratio,
                        obs.rms_delta_30ms,
                        obs.noise_floor_dbfs
                    );
                    // W1 wiring: update → publish → read-back echo
                    let new_duck = ducker.update(obs.posterior);
                    control_bus.publish(sp314_dsp::dsp::control_bus::ControlFrame {
                        p_speech: obs.posterior,
                        duck_gain: new_duck,
                        voice_gate: obs.is_speech,
                        snr_db: obs.rms_db - obs.noise_floor_dbfs,
                    });
                    let echo = control_bus.read();
                    if let Some(ref mut ew) = echo_w {
                        use std::io::Write;
                        let _ = writeln!(
                            ew,
                            "{},{:.2},{:.4},{},{:.4},{:.4}",
                            obs.frame_index,
                            time_sec,
                            echo.p_speech,
                            echo.voice_gate,
                            obs.duck_gain,
                            echo.duck_gain,
                        );
                    }

                    // W2.1: ControlTrack — time-indexed duck automation. frame i ↔ samples [i*480,(i+1)*480).
                    // Καταναλωτής: W2.2 (M bus, vector lookup + per-sample interpolation).
                    duck_track_clone.borrow_mut().push(new_duck);
                })
            }
            Err(e) => {
                eprintln!("vad trace unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    let vad_observer: Option<&mut dyn FnMut(sp314_dsp::analysis::vad_model::VadObservation)> =
        observer_holder.as_mut().map(|f| f as &mut dyn FnMut(_));

    let mut _metadata = two_pass
        .process_stream_with_params(
            inputs.stream_source,
            scout,
            ducking_gain,
            settings.macro_router_enabled,
            settings.boundaries,
            settings.sample_rate as f32,
            settings.noise_floor_dbfs,
            vad_observer,
            callback,
        )
        .map_err(|e| format!("TwoPassEngine error: {e}"))?;

    if let Some(e) = spatial_err {
        return Err(e);
    }

    let voice_hex = format!("{:x}", h_voice.finalize());
    let drums_hex = format!("{:x}", h_drums.finalize());
    let bass_hex = format!("{:x}", h_bass.finalize());
    let harm_hex = format!("{:x}", h_harmonics.finalize());
    let amb_hex = format!("{:x}", h_ambience.finalize());
    let pipeline_hex = {
        let mut hp = Sha256::new();
        hp.update(voice_hex.as_bytes());
        hp.update(bass_hex.as_bytes());
        format!("{:x}", hp.finalize())
    };

    let original_rms = libm::sqrtf(original_sum_sq / (inputs.total_frames.max(1) * 2) as f32);
    let mix_rms = libm::sqrtf(
        left_slice
            .iter()
            .zip(right_slice.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>()
            / (left_slice.len() * 2) as f32,
    );
    let gain = if mix_rms > 1e-10 {
        (original_rms / mix_rms).clamp(0.5, 2.0)
    } else {
        1.0
    };

    for i in 0..left_slice.len() {
        left_slice[i] *= gain;
        right_slice[i] *= gain;
    }

    Ok((
        StemFingerprints {
            voice: voice_hex,
            drums: drums_hex,
            bass: bass_hex,
            harmonics: harm_hex,
            ambience: amb_hex,
            pipeline: pipeline_hex,
        },
        _metadata,
    ))
}
