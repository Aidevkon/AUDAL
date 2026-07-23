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

    pub fn finish(mut self) -> Result<usize, String> {
        use std::io::Write;
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
}

/// Immutable input audio for a single chunk.
pub struct RenderInputs {
    pub original_sum_sq: f32,
    pub total_frames: usize,
    pub stream_source: sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader<
        sp314_orchestrator::raw_pcm_source::RawPcmFileSource,
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

    let original_sum_sq = inputs.original_sum_sq;
    // Captured error from spatial I/O inside the infallible callback.
    let mut spatial_err: Option<String> = None;

    let callback = |stems_chunk: &sp314_dsp::stft::two_pass::FiveStemsChunk| {
        let chunk_len = stems_chunk.voice.len();

        let mv: Vec<f32> = stems_chunk.voice.iter().map(|s| s * mix.voice).collect();
        let md: Vec<f32> = stems_chunk.drums.iter().map(|s| s * mix.drums).collect();
        let mb: Vec<f32> = stems_chunk.bass.iter().map(|s| s * mix.bass).collect();
        let mh: Vec<f32> = stems_chunk
            .harmonics
            .iter()
            .map(|s| s * mix.harmonics)
            .collect();
        let ma: Vec<f32> = stems_chunk
            .ambience
            .iter()
            .map(|s| s * mix.ambience)
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

    let mut _metadata = two_pass
        .process_stream_with_params(inputs.stream_source, scout, ducking_gain, callback)
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
