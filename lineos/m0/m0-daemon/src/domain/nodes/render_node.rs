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
    pub normalizer_ceiling_db: Option<f32>,
    pub flavour_id: Option<&'a str>,
    pub sample_rate: u32,
    pub noise_floor_dbfs: Option<f32>,
    pub restoration_enabled: bool,
    pub macro_router_enabled: bool,
    pub boundaries: &'a [lineos_corpus::scout::SegmentBoundary],
    pub vad_observe_enabled: bool,
    pub use_nmfd: bool,
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

    eprintln!("[W17-MIX] v={:.3} d={:.3} b={:.3} h={:.3} a={:.3} was_some={}",
        mix.voice, mix.drums, mix.bass,
        mix.harmonics, mix.ambience,
        mix_levels.is_some());
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

    struct BusGains {
        voice: f32,
        drums: f32,
        music: f32,
    }
    let bus_gains = BusGains {
        voice: 1.0,
        drums: 1.0,
        music: 1.0,
    };

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
    let effective_harmonics_gain =
        mix.harmonics * get_bus_gain(routing_table.bus_of(STEM_ID_HARMONICS));
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
    let mut glue_lpf_state_l = sp314_dsp::masking_eq::biquad::BiquadState::default();
    let mut glue_lpf_state_r = sp314_dsp::masking_eq::biquad::BiquadState::default();
    let duck_track: std::rc::Rc<std::cell::RefCell<Vec<f32>>> =
        std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let duck_track_cb = duck_track.clone();
    let duck_track_clone = duck_track.clone();

    let w16_sums = std::rc::Rc::new(std::cell::RefCell::new([0.0f32; 6]));
    let w16_sums_clone = w16_sums.clone();

    let w17_post_sums = std::rc::Rc::new(std::cell::RefCell::new([0.0f32; 6]));
    let w17_post_sums_clone = w17_post_sums.clone();

    let callback = |stems_chunk: &sp314_dsp::stft::two_pass::FiveStemsChunk| {
        let chunk_len = stems_chunk.voice.l.len();
        {
            let mut sums = w16_sums_clone.borrow_mut();
            for i in 0..chunk_len {
                sums[0] += (stems_chunk.voice.l[i] * stems_chunk.voice.l[i] + stems_chunk.voice.r[i] * stems_chunk.voice.r[i]) * 0.5;
                sums[1] += (stems_chunk.drums.l[i] * stems_chunk.drums.l[i] + stems_chunk.drums.r[i] * stems_chunk.drums.r[i]) * 0.5;
                sums[2] += (stems_chunk.bass.l[i] * stems_chunk.bass.l[i] + stems_chunk.bass.r[i] * stems_chunk.bass.r[i]) * 0.5;
                sums[3] += (stems_chunk.harmonics.l[i] * stems_chunk.harmonics.l[i] + stems_chunk.harmonics.r[i] * stems_chunk.harmonics.r[i]) * 0.5;
                sums[4] += (stems_chunk.ambience.l[i] * stems_chunk.ambience.l[i] + stems_chunk.ambience.r[i] * stems_chunk.ambience.r[i]) * 0.5;
                sums[5] += 1.0;
            }
        }

        let mut mv = stems_chunk.voice.clone();
        for i in 0..chunk_len {
            if settings.restoration_enabled {
                let (lg, rg) = vocal_gate.process_stereo(mv.l[i], mv.r[i]);
                mv.l[i] = lg;
                mv.r[i] = rg;
            }
            mv.l[i] *= effective_voice_gain;
            mv.r[i] *= effective_voice_gain;
        }

        let mut md = stems_chunk.drums.clone();
        for i in 0..chunk_len {
            md.l[i] *= effective_drums_gain;
            md.r[i] *= effective_drums_gain;
        }

        let mut mb = stems_chunk.bass.clone();
        for i in 0..chunk_len {
            mb.l[i] *= effective_bass_gain;
            mb.r[i] *= effective_bass_gain;
        }

        let mut mh = stems_chunk.harmonics.clone();
        for i in 0..chunk_len {
            mh.l[i] *= effective_harmonics_gain;
            mh.r[i] *= effective_harmonics_gain;
        }

        {
            let track = duck_track_cb.borrow();
            if !track.is_empty() {
                debug_assert_eq!(mb.l.len(), chunk_len);
                for i in 0..chunk_len {
                    let s = write_offset + i;
                    let f = s / 480;
                    let t = (s % 480) as f32 / 480.0;
                    let f_c = f.min(track.len().saturating_sub(1));
                    let f1_c = (f + 1).min(track.len().saturating_sub(1));
                    let g = track[f_c] * (1.0 - t) + track[f1_c] * t;
                    mb.l[i] *= g;
                    mb.r[i] *= g;
                    mh.l[i] *= g;
                    mh.r[i] *= g;
                }
            }
        }

        let mut ma = stems_chunk.ambience.clone();
        for i in 0..chunk_len {
            if settings.restoration_enabled {
                ma.l[i] = sp314_dsp::masking_eq::biquad::process_tdf2(
                    ma.l[i],
                    &glue_lpf_coeffs,
                    &mut glue_lpf_state_l,
                ) * GLUE_PAD_LINEAR;
                ma.r[i] = sp314_dsp::masking_eq::biquad::process_tdf2(
                    ma.r[i],
                    &glue_lpf_coeffs,
                    &mut glue_lpf_state_r,
                ) * GLUE_PAD_LINEAR;
            }
            ma.l[i] *= mix.ambience;
            ma.r[i] *= mix.ambience;
        }

        {
            let mut sums = w17_post_sums_clone.borrow_mut();
            for i in 0..chunk_len {
                sums[0] += (mv.l[i] * mv.l[i] + mv.r[i] * mv.r[i]) * 0.5;
                sums[1] += (md.l[i] * md.l[i] + md.r[i] * md.r[i]) * 0.5;
                sums[2] += (mb.l[i] * mb.l[i] + mb.r[i] * mb.r[i]) * 0.5;
                sums[3] += (mh.l[i] * mh.l[i] + mh.r[i] * mh.r[i]) * 0.5;
                sums[4] += (ma.l[i] * ma.l[i] + ma.r[i] * ma.r[i]) * 0.5;
                sums[5] += 1.0;
            }
        }

        h_voice.update(unsafe { std::slice::from_raw_parts(mv.l.as_ptr() as *const u8, mv.l.len() * 4) });
        h_voice.update(unsafe { std::slice::from_raw_parts(mv.r.as_ptr() as *const u8, mv.r.len() * 4) });
        h_drums.update(unsafe { std::slice::from_raw_parts(md.l.as_ptr() as *const u8, md.l.len() * 4) });
        h_drums.update(unsafe { std::slice::from_raw_parts(md.r.as_ptr() as *const u8, md.r.len() * 4) });
        h_bass.update(unsafe { std::slice::from_raw_parts(mb.l.as_ptr() as *const u8, mb.l.len() * 4) });
        h_bass.update(unsafe { std::slice::from_raw_parts(mb.r.as_ptr() as *const u8, mb.r.len() * 4) });
        h_harmonics.update(unsafe { std::slice::from_raw_parts(mh.l.as_ptr() as *const u8, mh.l.len() * 4) });
        h_harmonics.update(unsafe { std::slice::from_raw_parts(mh.r.as_ptr() as *const u8, mh.r.len() * 4) });
        h_ambience.update(unsafe { std::slice::from_raw_parts(ma.l.as_ptr() as *const u8, ma.l.len() * 4) });
        h_ambience.update(unsafe { std::slice::from_raw_parts(ma.r.as_ptr() as *const u8, ma.r.len() * 4) });

        if let Some(ref mut vg) = voice_graph_opt {
            let mut frame = 0;
            while frame < chunk_len {
                let end = (frame + 512).min(chunk_len);
                vg.process_block(&mut mv.l[frame..end], &mut mv.r[frame..end]);
                frame = end;
            }
        }

        let stage =
            FiveDotOneStage::render_chunk(&mv, &md, &mb, &mh, &ma, &scout.assignments);
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

    let trace_path = crate::blob_store::vad_trace_path(settings.blob_id);
    // W1 glue-feed: ControlBus observer για το κύκλωμα read-back echo
    let control_bus = std::sync::Arc::new(sp314_dsp::dsp::control_bus::ControlBus::new(
        settings.sample_rate as f32,
    ));
    let mut ducker = sp314_dsp::dsp::control_bus::Ducker::new(settings.sample_rate as f32);
    let echo_path = std::path::PathBuf::from("/tmp/w1_csv_B_bus.csv");
    let _ = std::fs::remove_file(&echo_path);
    let echo_file = std::fs::File::create(&echo_path).ok();
    let mut echo_w = echo_file.map(|f| {
        use std::io::Write;
        let mut bw = std::io::BufWriter::new(f);
        let _ = writeln!(
            bw,
            "frame_index,time_sec,p_speech,voice_gate,old_duck_gain,new_duck_gain"
        );
        bw
    });

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
            settings.use_nmfd,
            settings.boundaries,
            settings.sample_rate as f32,
            settings.noise_floor_dbfs,
            vad_observer,
            false,
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
    let ceiling_linear = settings
        .normalizer_ceiling_db
        .map(|db| 10_f32.powf(db / 20.0))
        .unwrap_or(2.0);
    let gain = if mix_rms > 1e-10 {
        (original_rms / mix_rms).clamp(0.5, ceiling_linear)
    } else {
        1.0
    };
    let gain = if std::env::var("BYPASS_W16").is_ok() { 1.0 } else { gain };
    let bypass = std::env::var("BYPASS_W16").is_ok();

    eprintln!("[W16] original_rms={:.6} mix_rms={:.6} gain={:.4} ceiling={:.4} bypass={}",
        original_rms, mix_rms, gain, ceiling_linear, bypass);

    {
        let sums = w16_sums.borrow();
        if sums[5] > 0.0 {
            let v = libm::sqrtf(sums[0] / sums[5]);
            let d = libm::sqrtf(sums[1] / sums[5]);
            let b = libm::sqrtf(sums[2] / sums[5]);
            let h = libm::sqrtf(sums[3] / sums[5]);
            let a = libm::sqrtf(sums[4] / sums[5]);
            eprintln!("[W16-STEMS] v={:.6} d={:.6} b={:.6} h={:.6} a={:.6}", v, d, b, h, a);
        }
    }

    {
        let sums = w17_post_sums.borrow();
        if sums[5] > 0.0 {
            let v = libm::sqrtf(sums[0] / sums[5]);
            let d = libm::sqrtf(sums[1] / sums[5]);
            let b = libm::sqrtf(sums[2] / sums[5]);
            let h = libm::sqrtf(sums[3] / sums[5]);
            let a = libm::sqrtf(sums[4] / sums[5]);
            eprintln!("[W17-POST] mv={:.6} md={:.6} mb={:.6} mh={:.6} ma={:.6}", v, d, b, h, a);
        }
    }

    // Διαγνωστικό: αποδεικνύει ότι ο Ducker φτάνει το floor.
    // Μετρημένο (W10.c): min 0.2517 = -12dB, 79% των frames
    // κάτω από 0.5 σε podcast υλικό 91% ομιλίας.
    {
        let track = duck_track.borrow();
        if !track.is_empty() {
            let n = track.len();
            let mut mn = 1.0f32; let mut mx = 0.0f32;
            let mut sum = 0.0f64;
            let mut below_09 = 0usize;
            let mut below_05 = 0usize;
            for &g in track.iter() {
                mn = mn.min(g); mx = mx.max(g);
                sum += g as f64;
                if g < 0.9 { below_09 += 1; }
                if g < 0.5 { below_05 += 1; }
            }
            eprintln!("[DUCK] frames={} min={:.4} max={:.4} \
                       mean={:.4} below0.9={} below0.5={}",
                n, mn, mx, (sum / n as f64) as f32,
                below_09, below_05);
        } else {
            eprintln!("[DUCK] track EMPTY");
        }
    }

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
