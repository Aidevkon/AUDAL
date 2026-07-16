//! decode_node — audio ingestion, validation, AudioPayload build.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P1
//! Extracted from dsp_pipeline.rs with zero behavior change.

use crate::handlers::decode;
use sha2::Digest;

/// Output of decode_node — everything downstream needs.
pub struct DecodedAudio {
    pub payload: lineos_types::AudioPayload,
    pub input_blake3_hex: String,
    pub input_sha256_hex: String,
    pub pcm_channels: u16,    // Needed for Phase 9 Telemetry
    pub pcm_sample_rate: u32, // Needed for Phase 9 Telemetry
    pub target_lufs: Option<f32>,
    pub input_hash_hex: String,
    pub seed: u64, // derive_seed returns u64
    pub original_sr: u32,
    pub original_ch: u16,
    pub duration_ms: f64,
}

pub fn run(audio_path: &str, preset_id: &str, blob_id: &str) -> Result<DecodedAudio, String> {
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
    let input_hash_hex = hex::encode(path_hash);
    let seed = derive_seed(&path_hash);

    let payload = decode::decode_smart(audio_path).map_err(|e| format!("Decode error: {e}"))?;

    match payload {
        lineos_types::AudioPayload::Stereo(buf) => {
            // Manually re-interleave to preserve exact byte-for-byte hashes and disk dumps
            let mut interleaved = Vec::with_capacity(buf.num_frames * 2);
            for i in 0..buf.num_frames {
                interleaved.push(buf.left[i]);
                interleaved.push(buf.right[i]);
            }

            let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
            // SAFETY: pcm.samples is a Vec<f32>, reinterpreted as raw bytes for direct
            // disk write. Native-endian, in-process only (same architecture as the
            // reader, xaak's PcmTransfer) — not a portable serialization format,
            // matches the byte layout the old executor.rs dump already used (verified
            // byte-for-byte equivalent before this change, not assumed).
            let raw_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(interleaved.as_ptr() as *const u8, interleaved.len() * 4)
            };
            std::fs::write(&raw_path, raw_bytes)
                .map_err(|e| format!("Failed to write raw dump: {e}"))?;
            eprintln!(
                "[RAW-SAVE] wrote raw PCM {} bytes to {}",
                raw_bytes.len(),
                raw_path
            );

            let mut blake3_hasher = blake3::Hasher::new();
            let mut sha256_hasher = sha2::Sha256::new();
            for &sample in &interleaved {
                blake3_hasher.update(&sample.to_le_bytes());
                sha2::Digest::update(&mut sha256_hasher, sample.to_be_bytes());
            }
            let input_blake3_hex = blake3_hasher.finalize().to_hex().to_string();
            let input_sha256_hex = format!("{:x}", sha2::Digest::finalize(sha256_hasher));

            // In AudioPayload we dropped the "original" metadata, so we use the validated ones
            let original_sr = buf.sample_rate;
            let original_ch = 2;
            let duration_ms = (buf.num_frames as f64 / buf.sample_rate as f64) * 1000.0;
            let pcm_channels_for_telemetry = 2;
            let pcm_sr_for_telemetry = buf.sample_rate;

            // Silence guard
            let rms = compute_rms(&interleaved);
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

            Ok(DecodedAudio {
                payload: lineos_types::AudioPayload::Stereo(buf),
                input_blake3_hex,
                input_sha256_hex,
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
        lineos_types::AudioPayload::FiveDotOne {
            channels,
            sample_rate,
            num_frames,
        } => {
            // Interleave 6ch: L R C LFE Ls Rs
            // per frame — ίδια λογική με το
            // Stereo arm αλλά για 6 κανάλια.
            let mut interleaved = Vec::with_capacity(num_frames * 6);
            for i in 0..num_frames {
                for channel in channels.iter() {
                    interleaved.push(channel[i]);
                }
            }

            let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
            let raw_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(interleaved.as_ptr() as *const u8, interleaved.len() * 4)
            };
            std::fs::write(&raw_path, raw_bytes)
                .map_err(|e| format!("Failed to write raw dump: {e}"))?;

            let mut blake3_hasher = blake3::Hasher::new();
            let mut sha256_hasher = sha2::Sha256::new();
            for &sample in &interleaved {
                blake3_hasher.update(&sample.to_le_bytes());
                sha2::Digest::update(&mut sha256_hasher, sample.to_be_bytes());
            }
            let input_blake3_hex = blake3_hasher.finalize().to_hex().to_string();
            let input_sha256_hex = format!("{:x}", sha2::Digest::finalize(sha256_hasher));

            let duration_ms = (num_frames as f64 / sample_rate as f64) * 1000.0;

            // Silence guard (ίδιο threshold
            // με Stereo arm: -60 dBFS)
            let rms = compute_rms(&interleaved);
            let rms_dbfs = if rms > 0.0 {
                20.0 * (rms as f64).log10() as f32
            } else {
                f32::NEG_INFINITY
            };
            if rms_dbfs < -60.0 {
                return Err(format!(
                    "Input validation failed: 5.1 audio is silence (RMS = {rms_dbfs:.1} dBFS)"
                ));
            }

            // Normalization guard — για 5.1
            // δεν κάνουμε LUFS normalization
            // στο decode_node (γίνεται στο
            // BS.775 telemetry path). Ελέγχουμε
            // μόνο για extreme overflow.
            let rough_lufs = rms_to_lufs(rms);
            let rough_gain_db = -18.0_f32 - rough_lufs;
            if rough_gain_db > 30.0 {
                return Err(format!(
                    "DSP arithmetic error — 5.1 normalization gain would exceed 32× (RMS = {rms_dbfs:.1} dBFS, est. gain = {rough_gain_db:.1} dB)."
                ));
            }

            Ok(DecodedAudio {
                payload: lineos_types::AudioPayload::FiveDotOne {
                    channels,
                    sample_rate,
                    num_frames,
                },
                input_blake3_hex,
                input_sha256_hex,
                pcm_channels: 6,
                pcm_sample_rate: sample_rate,
                target_lufs,
                input_hash_hex,
                seed,
                original_sr: sample_rate,
                original_ch: 6,
                duration_ms,
            })
        }
        lineos_types::AudioPayload::Stems {
            voice,
            drums,
            bass,
            harmonics,
            ambience,
            sample_rate,
            num_frames,
        } => {
            // Interleave 5 stems (L+R each)
            // = 10 channels για hashing
            let mut interleaved = Vec::with_capacity(num_frames * 10);
            for i in 0..num_frames {
                interleaved.push(voice.left[i]);
                interleaved.push(voice.right[i]);
                interleaved.push(drums.left[i]);
                interleaved.push(drums.right[i]);
                interleaved.push(bass.left[i]);
                interleaved.push(bass.right[i]);
                interleaved.push(harmonics.left[i]);
                interleaved.push(harmonics.right[i]);
                interleaved.push(ambience.left[i]);
                interleaved.push(ambience.right[i]);
            }
            let mut blake3_hasher = blake3::Hasher::new();
            let mut sha256_hasher = sha2::Sha256::new();
            for &sample in &interleaved {
                blake3_hasher.update(&sample.to_le_bytes());
                sha2::Digest::update(&mut sha256_hasher, sample.to_be_bytes());
            }
            let input_blake3_hex = blake3_hasher.finalize().to_hex().to_string();
            let input_sha256_hex = format!("{:x}", sha2::Digest::finalize(sha256_hasher));
            let duration_ms = (num_frames as f64 / sample_rate as f64) * 1000.0;
            let rms = compute_rms(&interleaved);
            let rms_dbfs = if rms > 0.0 {
                20.0 * (rms as f64).log10() as f32
            } else {
                f32::NEG_INFINITY
            };
            if rms_dbfs < -60.0 {
                return Err(format!(
                    "Input validation failed: \
                     stems audio is silence \
                     (RMS = {rms_dbfs:.1} dBFS)"
                ));
            }
            Ok(DecodedAudio {
                payload: lineos_types::AudioPayload::Stems {
                    voice,
                    drums,
                    bass,
                    harmonics,
                    ambience,
                    sample_rate,
                    num_frames,
                },
                input_blake3_hex,
                input_sha256_hex,
                pcm_channels: 10,
                pcm_sample_rate: sample_rate,
                target_lufs,
                input_hash_hex,
                seed,
                original_sr: sample_rate,
                original_ch: 10,
                duration_ms,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use sha2::Digest;

    #[test]
    fn streaming_hash_matches_batch_hash() {
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();

        // Batch (old way)
        let batch_le_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let batch_blake3 = blake3::hash(&batch_le_bytes).to_hex().to_string();

        let batch_be_bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_be_bytes()).collect();
        let mut batch_sha256 = sha2::Sha256::new();
        sha2::Digest::update(&mut batch_sha256, &batch_be_bytes);
        let batch_sha256_hex = format!("{:x}", sha2::Digest::finalize(batch_sha256));

        // Streaming (new way) — feed in small chunks, not all at once, to prove
        // chunking doesn't change the result
        let mut stream_blake3 = blake3::Hasher::new();
        let mut stream_sha256 = sha2::Sha256::new();
        for chunk in samples.chunks(7) {
            // deliberately odd chunk size, not aligned to anything
            for &s in chunk {
                stream_blake3.update(&s.to_le_bytes());
                sha2::Digest::update(&mut stream_sha256, s.to_be_bytes());
            }
        }
        let stream_blake3_hex = stream_blake3.finalize().to_hex().to_string();
        let stream_sha256_hex = format!("{:x}", sha2::Digest::finalize(stream_sha256));

        assert_eq!(
            batch_blake3, stream_blake3_hex,
            "BLAKE3 streaming hash must match batch hash"
        );
        assert_eq!(
            batch_sha256_hex, stream_sha256_hex,
            "SHA-256 streaming hash must match batch hash"
        );
    }
}
