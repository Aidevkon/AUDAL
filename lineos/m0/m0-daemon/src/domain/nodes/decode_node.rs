//! decode_node — audio ingestion, validation, AudioChunk build.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P1
//! Extracted from dsp_pipeline.rs with zero behavior change.

use crate::handlers::decode;
use lineos_types::AudioChunk;

/// Output of decode_node — everything downstream needs.
pub struct DecodedAudio {
    pub chunk: AudioChunk,
    pub chunk_original: AudioChunk, // Needed for A/B processing
    pub pcm_samples: Vec<f32>,      // Needed for Phase 9 Telemetry
    pub pcm_channels: u16,          // Needed for Phase 9 Telemetry
    pub pcm_sample_rate: u32,       // Needed for Phase 9 Telemetry
    pub target_lufs: Option<f32>,
    pub input_hash_hex: String,
    pub seed: u64, // derive_seed returns u64
    pub original_sr: u32,
    pub original_ch: u16,
    pub duration_ms: f64,
}

pub fn run(audio_path: &str, preset_id: &str) -> Result<DecodedAudio, String> {
    use crate::domain::dsp_pipeline::{
        compute_rms, compute_sha256_bytes, derive_seed, rms_to_lufs,
    };

    // Load schema
    let schema: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../shared/schema/bmr-128.schema.json"
    ))
    .map_err(|e| format!("Schema load error: {e}"))?;

    let target_lufs: Option<f32> = schema
        .get("presets")
        .and_then(|p| p.get(preset_id))
        .and_then(|p| p.get("target_lufs"))
        .and_then(|l| l.as_f64())
        .map(|lufs| (lufs as f32).clamp(-40.0, 0.0));

    let path_hash = compute_sha256_bytes(audio_path.as_bytes());
    let input_hash_hex = hex::encode(&path_hash);
    let seed = derive_seed(&path_hash);

    let pcm = decode::decode_audio(audio_path).map_err(|e| format!("Decode error: {e}"))?;

    let original_sr = pcm.original_sr;
    let original_ch = pcm.original_ch;
    let duration_ms = pcm.duration_ms as f64;

    let pcm_samples_for_telemetry = pcm.samples.clone();
    let pcm_channels_for_telemetry = pcm.channels;
    let pcm_sr_for_telemetry = pcm.sample_rate;

    // Silence guard
    let rms = compute_rms(&pcm.samples);
    let rms_dbfs = if rms > 0.0 {
        20.0 * (rms as f64).log10() as f32
    } else {
        f32::NEG_INFINITY
    };
    if rms_dbfs < -60.0 {
        return Err(format!(
            "Input validation failed: audio is silence (RMS = {rms_dbfs:.1} dBFS)"
        ));
    }

    // Normalization overflow guard
    let rough_lufs = rms_to_lufs(rms);
    let rough_gain_db = -14.0_f32 - rough_lufs;
    if rough_gain_db > 30.0 {
        return Err(format!(
            "DSP arithmetic error — normalization gain would exceed 32× \
             (input RMS = {rms_dbfs:.1} dBFS, est. gain = {rough_gain_db:.1} dB). \
             Track too quiet or too short (< 400ms) for loudness normalization."
        ));
    }

    let chunk = AudioChunk {
        left: pcm.samples.iter().step_by(2).copied().collect(),
        right: pcm.samples.iter().skip(1).step_by(2).copied().collect(),
        sample_rate: pcm.sample_rate,
        num_frames: pcm.samples.len() / 2,
    };

    let chunk_original = chunk.clone();

    Ok(DecodedAudio {
        chunk,
        chunk_original,
        pcm_samples: pcm_samples_for_telemetry,
        pcm_channels: pcm_channels_for_telemetry,
        pcm_sample_rate: pcm_sr_for_telemetry,
        target_lufs,
        input_hash_hex,
        seed,
        original_sr,
        original_ch,
        duration_ms,
    })
}
