//! render_node — TwoPassEngine rendering, spatial mixing, and streaming hashes.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P3b

use crate::blob_store::StemFingerprints;
use crate::handlers::master::MixLevels;
use sha2::{Digest, Sha256};
use sp314_dsp::spatial::five_dot_one::FiveDotOneStage;
use sp314_dsp::spatial::renderer::StereoRenderer;
use sp314_dsp::stft::two_pass::{ScoutResult, TwoPassEngine};

/// Mutable output slices for 6-channel
/// spatial rendering. Passed by the caller
/// (dsp_pipeline) which owns the allocations.
/// Lifetime 'a tied to the output buffers.
pub struct SpatialSlicesMut<'a> {
    pub l: &'a mut [f32],
    pub r: &'a mut [f32],
    pub c: &'a mut [f32],
    pub lfe: &'a mut [f32],
    pub ls: &'a mut [f32],
    pub rs: &'a mut [f32],
}

/// Non-audio render configuration for a single chunk.
pub struct RenderSettings<'a> {
    pub ducking_gain: f32,
    pub mix_levels: Option<&'a MixLevels>,
    pub flavour_id: Option<&'a str>,
    pub sample_rate: u32,
}

/// Immutable input audio for a single chunk.
pub struct RenderInputs<'a> {
    pub mono: &'a [f32],
    pub original_left: &'a [f32],
    pub original_right: &'a [f32],
    pub original_sum_sq: f32,
}

// allow: 7 args — 3 are &mut output slices, deliberately positional
// (grouping mutable slices behind a struct adds lifetime noise, not clarity)
#[allow(clippy::too_many_arguments)]
pub fn run(
    two_pass: &mut TwoPassEngine,
    scout: &ScoutResult,
    settings: &RenderSettings<'_>,
    inputs: &RenderInputs<'_>,
    left_slice: &mut [f32],
    right_slice: &mut [f32],
    mut spatial: Option<&mut SpatialSlicesMut<'_>>,
) -> Result<(StemFingerprints, sp314_dsp::stft::two_pass::RenderMetadata), String> {
    let mono = inputs.mono;
    let original_left = inputs.original_left;
    let original_right = inputs.original_right;
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

    let mut _metadata = two_pass
        .process_slices_with_params(
            mono,
            original_left,
            original_right,
            scout,
            ducking_gain,
            |stems_chunk| {
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

                h_voice.update(unsafe {
                    std::slice::from_raw_parts(mv.as_ptr() as *const u8, mv.len() * 4)
                });
                h_drums.update(unsafe {
                    std::slice::from_raw_parts(md.as_ptr() as *const u8, md.len() * 4)
                });
                h_bass.update(unsafe {
                    std::slice::from_raw_parts(mb.as_ptr() as *const u8, mb.len() * 4)
                });
                h_harmonics.update(unsafe {
                    std::slice::from_raw_parts(mh.as_ptr() as *const u8, mh.len() * 4)
                });
                h_ambience.update(unsafe {
                    std::slice::from_raw_parts(ma.as_ptr() as *const u8, ma.len() * 4)
                });

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

                let stage = FiveDotOneStage::render_chunk(
                    &clean_voice,
                    &md,
                    &mb,
                    &mh,
                    &ma,
                    &scout.assignments,
                );
                let mut stage = stage;
                stage.apply_scales(scout.rear_scale, scout.lfe_scale);

                if let Some(ref mut sp) = spatial {
                    use sp314_dsp::spatial::renderer::FiveDotOneRenderer;
                    FiveDotOneRenderer::render_into(
                        &stage,
                        sp.l,
                        sp.r,
                        sp.c,
                        sp.lfe,
                        sp.ls,
                        sp.rs,
                        write_offset,
                    );
                }

                let (sp_l, sp_r) = StereoRenderer::render(&stage);

                let end_offset = write_offset + sp_l.len();
                left_slice[write_offset..end_offset].copy_from_slice(&sp_l);
                right_slice[write_offset..end_offset].copy_from_slice(&sp_r);
                write_offset = end_offset;
            },
        )
        .map_err(|e| format!("TwoPassEngine error: {e}"))?;

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

    let original_rms = libm::sqrtf(inputs.original_sum_sq / (original_left.len() * 2) as f32);
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
