//! decode_node — audio ingestion, validation, AudioPayload build.
//! Authority: dsp-pipeline-refactor-spec-v1_0.md R-P1
//! Extracted from dsp_pipeline.rs with zero behavior change.

use crate::handlers::decode;
use sha2::Digest;

/// Output of decode_node — everything downstream needs.
pub struct DecodedAudio {
    pub payload: Option<lineos_types::AudioPayload>,
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
    #[allow(clippy::type_complexity)]
    pub beat_data: Option<(f32, Vec<u32>, Vec<u32>, Vec<u32>)>,
    pub original_sum_sq: f32,
    pub left_sum_sq: f32,
    pub right_sum_sq: f32,
    pub total_frames: usize,
}

pub fn run(audio_path: &str, preset_id: &str, blob_id: &str) -> Result<DecodedAudio, String> {
    use crate::domain::content_type::ContentTypeExt;
    use crate::domain::dsp_pipeline::{
        compute_rms, compute_sha256_bytes, derive_seed, rms_to_lufs,
    };
    use crate::dsp::audio_source::AudioSource;
    use std::io::Write;

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

    let (probe_ch, true_original_sr) = {
        let reader =
            crate::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(audio_path))
                .map_err(|e| format!("Failed to probe channels: {e}"))?;
        (reader.channels(), reader.sample_rate())
    };

    if probe_ch == 6 {
        let mut stream = crate::dsp::six_channel_stream::StandardizedSixChannelStream::open(
            std::path::Path::new(audio_path),
        )
        .map_err(|e| format!("Decode error: {e}"))?;

        let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
        let mut dump_file = std::fs::File::create(&raw_path)
            .map_err(|e| format!("Failed to write raw dump: {e}"))?;

        let mut buf = vec![0f32; 4096 * 6];
        let mut left_sum_sq = 0.0_f32;
        let mut right_sum_sq = 0.0_f32;
        let mut total_frames: usize = 0;
        loop {
            let frames = stream
                .fill_buffer(&mut buf)
                .map_err(|e| format!("Decode error: {e}"))?;
            if frames == 0 {
                break;
            }
            let valid_samples = &buf[..frames * 6];

            let raw_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    valid_samples.as_ptr() as *const u8,
                    valid_samples.len() * 4,
                )
            };
            std::io::Write::write_all(&mut dump_file, raw_bytes)
                .map_err(|e| format!("Failed to write raw dump: {e}"))?;

            total_frames += frames;

            for i in 0..frames {
                let l = buf[i * 6];
                let r = buf[i * 6 + 1];
                left_sum_sq += l * l;
                right_sum_sq += r * r;
            }
        }

        let (input_blake3_hex, input_sha256_hex) = stream.input_hashes();
        let duration_ms =
            (total_frames as f64 / crate::dsp::standardized_stream::TARGET_SR as f64) * 1000.0;

        return Ok(DecodedAudio {
            payload: None,
            input_blake3_hex,
            input_sha256_hex,
            pcm_channels: 6,
            pcm_sample_rate: crate::dsp::standardized_stream::TARGET_SR,
            target_lufs,
            input_hash_hex,
            seed,
            original_sr: true_original_sr,
            original_ch: 6,
            duration_ms,
            beat_data: None,
            original_sum_sq: 0.0,
            left_sum_sq,
            right_sum_sq,
            total_frames,
        });
    }

    // decode_smart's exact decision logic is: `match original_ch { 6 => FiveDotOne, _ => Stereo }`.
    if probe_ch != 6 {
        let mut stream = crate::dsp::standardized_stream::StandardizedAudioStream::open(
            std::path::Path::new(audio_path),
        )
        .map_err(|e| format!("Decode error: {e}"))?;

        let mut streaming_beat: Option<crate::dsp::beat_detector::StreamingBeatDetector> =
            if !crate::domain::content_type::ContentType::from_preset(preset_id).skip_stems() {
                Some(crate::dsp::beat_detector::StreamingBeatDetector::new(
                    crate::dsp::standardized_stream::TARGET_SR,
                ))
            } else {
                None
            };
        let mut mono_chunk: Option<Vec<f32>> = if streaming_beat.is_some() {
            Some(Vec::with_capacity(4096))
        } else {
            None
        };

        let raw_path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
        let mut dump_file = std::fs::File::create(&raw_path)
            .map_err(|e| format!("Failed to write raw dump: {e}"))?;

        let mut buf = vec![0f32; 4096 * 2];
        let mut original_sum_sq = 0.0_f32;
        let mut left_sum_sq = 0.0_f32;
        let mut right_sum_sq = 0.0_f32;
        let mut total_frames: usize = 0;
        loop {
            let frames = stream
                .fill_buffer(&mut buf)
                .map_err(|e| format!("Decode error: {e}"))?;
            if frames == 0 {
                break;
            }
            let valid_samples = &buf[..frames * 2];

            let raw_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    valid_samples.as_ptr() as *const u8,
                    valid_samples.len() * 4,
                )
            };
            dump_file
                .write_all(raw_bytes)
                .map_err(|e| format!("Failed to write raw dump: {e}"))?;

            if let Some(mono) = mono_chunk.as_mut() {
                mono.clear();
            }
            total_frames += frames;

            for i in 0..frames {
                let l = buf[i * 2];
                let r = buf[i * 2 + 1];
                original_sum_sq += l * l + r * r;
                left_sum_sq += l * l;
                right_sum_sq += r * r;
                if let Some(mono) = mono_chunk.as_mut() {
                    mono.push((l + r) * 0.5);
                }
            }

            if let Some(detector) = streaming_beat.as_mut() {
                detector.feed_chunk(mono_chunk.as_ref().unwrap());
            }
        }

        eprintln!(
            "[RAW-SAVE] wrote raw PCM {} bytes to {}",
            total_frames * 2 * 4,
            raw_path
        );

        let (input_blake3_hex, input_sha256_hex) = stream.input_hashes();

        // Use tier2_verdict's message verbatim. Nothing downstream string-matches it.
        stream.tier2_verdict()?;

        let duration_ms =
            (total_frames as f64 / crate::dsp::standardized_stream::TARGET_SR as f64) * 1000.0;
        let beat_data = streaming_beat.map(|d| d.finish());

        return Ok(DecodedAudio {
            // A3 1.2b: Stereo streaming path materializes no payload —
            // audio lives in the raw dump (/tmp/m0d-raw-{blob_id}.pcm).
            payload: None,
            input_blake3_hex,
            input_sha256_hex,
            pcm_channels: 2,
            pcm_sample_rate: 48000,
            target_lufs,
            input_hash_hex,
            seed,
            original_sr: true_original_sr,
            original_ch: probe_ch as u16,
            duration_ms,
            beat_data,
            original_sum_sq,
            left_sum_sq,
            right_sum_sq,
            total_frames,
        });
    }

    let payload = decode::decode_smart(audio_path).map_err(|e| format!("Decode error: {e}"))?;

    // A4-i: unreachable from production (6ch intercepted upstream
    // via StandardizedSixChannelStream); kept while decode_smart's
    // 6ch tests live. Deletion target at A4 close.
    match payload {
        lineos_types::AudioPayload::Stereo(_) => {
            unreachable!("Stereo payload handled by streaming path")
        }
        lineos_types::AudioPayload::FiveDotOne {
            channels,
            sample_rate,
            num_frames,
        } => {
            // Interleave 6ch: L R C LFE Ls Rs
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
            eprintln!(
                "[RAW-SAVE] wrote 5.1 raw PCM {} bytes to {}",
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
            let original_sr = sample_rate;
            let original_ch = 6;
            let duration_ms = (num_frames as f64 / sample_rate as f64) * 1000.0;
            let pcm_channels_for_telemetry = 6;
            let pcm_sr_for_telemetry = sample_rate;

            let mut left_sum_sq = 0.0_f32;
            let mut right_sum_sq = 0.0_f32;
            for (l, r) in channels[0].iter().zip(channels[1].iter()).take(num_frames) {
                left_sum_sq += l * l;
                right_sum_sq += r * r;
            }

            // Silence / Gain guards for 5.1
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

            let rough_lufs = rms_to_lufs(rms);
            let rough_gain_db = -14.0_f32 - rough_lufs;
            if rough_gain_db > 30.0 {
                return Err(format!(
                    "DSP arithmetic error — 5.1 normalization gain would exceed 32× (RMS = {rms_dbfs:.1} dBFS, est. gain = {rough_gain_db:.1} dB)."
                ));
            }

            Ok(DecodedAudio {
                payload: Some(lineos_types::AudioPayload::FiveDotOne {
                    channels,
                    sample_rate,
                    num_frames,
                }),
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
                beat_data: None,
                original_sum_sq: 0.0,
                left_sum_sq,
                right_sum_sq,
                total_frames: num_frames,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_dump(blob_id: &str) -> (Vec<f32>, Vec<f32>) {
        let path = format!("/tmp/m0d-raw-{}.pcm", blob_id);
        let bytes = std::fs::read(&path).expect("raw dump must exist");
        assert_eq!(bytes.len() % 8, 0, "dump must be whole frames");
        let mut l = Vec::with_capacity(bytes.len() / 8);
        let mut r = Vec::with_capacity(bytes.len() / 8);
        for f in bytes.chunks_exact(8) {
            l.push(f32::from_le_bytes([f[0], f[1], f[2], f[3]]));
            r.push(f32::from_le_bytes([f[4], f[5], f[6], f[7]]));
        }
        (l, r)
    }

    #[test]
    fn streaming_hash_matches_batch_hash() {
        let audio_path = "/tmp/test_stream_hash.wav";
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(audio_path, spec).unwrap();
        for _ in 0..88200 {
            w.write_sample(0.5f32).unwrap();
        }
        w.finalize().unwrap();

        let batch_payload = crate::handlers::decode::decode_smart(audio_path).unwrap();
        let buf = match batch_payload {
            lineos_types::AudioPayload::Stereo(b) => b,
            _ => panic!("expected stereo"),
        };
        let mut interleaved = Vec::with_capacity(buf.num_frames * 2);
        for i in 0..buf.num_frames {
            interleaved.push(buf.left[i]);
            interleaved.push(buf.right[i]);
        }
        let mut sha_batch = sha2::Sha256::new();
        for &sample in &interleaved {
            sha2::Digest::update(&mut sha_batch, sample.to_be_bytes());
        }
        let batch_sha256_hex = format!("{:x}", sha2::Digest::finalize(sha_batch));

        use crate::dsp::audio_source::AudioSource;
        let mut stream = crate::dsp::standardized_stream::StandardizedAudioStream::open(
            std::path::Path::new(audio_path),
        )
        .unwrap();
        let mut stream_buf = vec![0f32; 1024];
        loop {
            let n = stream.fill_buffer(&mut stream_buf).unwrap();
            if n == 0 {
                break;
            }
        }
        let (_, stream_sha256_hex) = stream.input_hashes();

        let _ = std::fs::remove_file(audio_path);

        assert_eq!(
            batch_sha256_hex, stream_sha256_hex,
            "SHA-256 streaming hash must match batch hash"
        );
    }

    #[test]
    fn decode_node_stereo_matches_legacy_batch() {
        for &sr in &[44_100, 48_000] {
            let path = format!("/tmp/decode_node_stereo_parity_{}.wav", sr);
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: sr,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&path, spec).unwrap();
            let n = sr as usize * 2;
            for i in 0..n {
                let v =
                    0.3 * libm::sinf(2.0 * std::f32::consts::PI * 220.0 * (i as f32 / sr as f32));
                w.write_sample(v).unwrap(); // L
                w.write_sample(v).unwrap(); // R
            }
            w.finalize().unwrap();

            let preset_id = "podcast";
            let blob_id = "test-blob-123";

            let new_result = super::run(&path, preset_id, blob_id).expect("new path failed");

            // Oracle
            let (_, true_original_sr, _) = crate::handlers::decode::decode_raw_interleaved(&path)
                .expect("decode_raw_interleaved failed");
            let payload =
                crate::handlers::decode::decode_smart(&path).expect("decode_smart failed");
            let buf = match payload {
                lineos_types::AudioPayload::Stereo(b) => b,
                _ => panic!("expected stereo"),
            };
            let mut interleaved = Vec::with_capacity(buf.num_frames * 2);
            for i in 0..buf.num_frames {
                interleaved.push(buf.left[i]);
                interleaved.push(buf.right[i]);
            }
            let mut blake3_hasher = blake3::Hasher::new();
            let mut sha256_hasher = sha2::Sha256::new();
            for &sample in &interleaved {
                blake3_hasher.update(&sample.to_le_bytes());
                sha2::Digest::update(&mut sha256_hasher, sample.to_be_bytes());
            }
            let old_blake3 = blake3_hasher.finalize().to_hex().to_string();
            let old_sha256 = format!("{:x}", sha2::Digest::finalize(sha256_hasher));
            let old_original_sr = true_original_sr;
            let old_duration_ms = (buf.num_frames as f64 / buf.sample_rate as f64) * 1000.0;

            assert_eq!(new_result.input_blake3_hex, old_blake3, "blake3 mismatch");
            assert_eq!(new_result.input_sha256_hex, old_sha256, "sha256 mismatch");
            assert_eq!(
                new_result.original_sr, old_original_sr,
                "original_sr mismatch"
            );
            assert_eq!(new_result.duration_ms, old_duration_ms, "duration mismatch");

            let (dump_left, dump_right) = read_dump(blob_id);
            assert_eq!(dump_left.len(), buf.left.len(), "left channel len mismatch");
            assert_eq!(
                dump_right.len(),
                buf.right.len(),
                "right channel len mismatch"
            );
            for i in 0..buf.left.len() {
                assert_eq!(
                    dump_left[i].to_bits(),
                    buf.left[i].to_bits(),
                    "left channel samples mismatch at {i}"
                );
                assert_eq!(
                    dump_right[i].to_bits(),
                    buf.right[i].to_bits(),
                    "right channel samples mismatch at {i}"
                );
            }

            let _ = std::fs::remove_file(&path);
            let _ = std::fs::remove_file(format!("/tmp/m0d-raw-{}.pcm", blob_id));
        }
    }

    #[test]
    fn decode_node_streaming_beat_matches_batch() {
        let sr = 48_000;
        let path = format!("/tmp/decode_node_streaming_beat_{}.wav", sr);
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        // Generate a 10s click track at 120BPM
        let n = sr as usize * 10;
        let interval_samples = (60.0 / 120.0 * sr as f32) as usize;
        let mut pos = 0;
        let mut audio = vec![0.0f32; n];
        while pos + 20 < n {
            for i in 0..20 {
                audio[pos + i] = 0.9 * (1.0 - i as f32 / 20.0);
            }
            pos += interval_samples;
        }
        for &s in &audio {
            w.write_sample(s).unwrap(); // L
            w.write_sample(s).unwrap(); // R
        }
        w.finalize().unwrap();

        let preset_id = "spotify"; // Music path -> skip_stems() is false
        let blob_id = "test-blob-beat";

        let new_result = super::run(&path, preset_id, blob_id).expect("new path failed");
        let streaming_beat = new_result
            .beat_data
            .expect("Music path should populate beat_data");

        // Oracle batch pass
        let payload = crate::handlers::decode::decode_smart(&path).expect("decode_smart failed");
        let buf = match payload {
            lineos_types::AudioPayload::Stereo(b) => b,
            _ => panic!("expected stereo"),
        };
        let mono_samples: Vec<f32> = buf
            .left
            .iter()
            .zip(buf.right.iter())
            .map(|(l, r)| (*l + *r) * 0.5)
            .collect();
        let detector = crate::dsp::beat_detector::BeatDetector::new(buf.sample_rate);
        let batch_beat = detector.analyze(&mono_samples);

        assert_eq!(streaming_beat.0, batch_beat.0, "BPM mismatch");
        assert_eq!(streaming_beat.1, batch_beat.1, "beats_ms mismatch");
        assert_eq!(streaming_beat.2, batch_beat.2, "downbeats_ms mismatch");
        assert_eq!(streaming_beat.3, batch_beat.3, "transients_ms mismatch");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn original_sum_sq_bit_identical() {
        let path = "/tmp/test_original_sum_sq.wav";
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let sr = 48000_f32;
        // write some non-trivial floating point data
        for i in 0..10000 {
            let t = i as f32 / sr;
            w.write_sample(libm::sinf(t * 1000.0) * 0.5).unwrap();
            w.write_sample(libm::cosf(t * 1500.0) * 0.25).unwrap();
        }
        w.finalize().unwrap();

        let audio = run(path, "test", "blob").unwrap();
        let (dump_left, dump_right) = read_dump("blob");

        let old_sum_sq = dump_left
            .iter()
            .zip(dump_right.iter())
            .map(|(l, r)| l * l + r * r)
            .sum::<f32>();

        let old_rms = libm::sqrtf(old_sum_sq / (dump_left.len() * 2) as f32);
        let new_rms = libm::sqrtf(audio.original_sum_sq / (dump_left.len() * 2) as f32);

        assert_eq!(
            old_rms.to_bits(),
            new_rms.to_bits(),
            "f32 bit-identity check failed! old_rms bits: {:032b}, new_rms bits: {:032b}",
            old_rms.to_bits(),
            new_rms.to_bits()
        );

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file("/tmp/m0d-raw-blob.pcm");
    }

    #[test]
    fn channel_sum_sq_and_total_frames_bit_identical() {
        let path = "/tmp/test_channel_sum_sq.wav";
        let blob_id = "test-blob-sum-sq";
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        let sr = 48000_f32;
        // write some non-trivial floating point data
        for i in 0..10000 {
            let t = i as f32 / sr;
            w.write_sample(libm::sinf(t * 1000.0) * 0.5).unwrap();
            w.write_sample(libm::cosf(t * 1500.0) * 0.25).unwrap();
        }
        w.finalize().unwrap();

        let audio = run(path, "test", blob_id).unwrap();
        let (dump_left, dump_right) = read_dump(blob_id);

        assert_eq!(
            audio.total_frames,
            dump_left.len(),
            "total_frames must exactly match left.len()"
        );

        let mut expected_left_sum_sq = 0.0_f32;
        let mut expected_right_sum_sq = 0.0_f32;
        for i in 0..dump_left.len() {
            let l = dump_left[i];
            let r = dump_right[i];
            expected_left_sum_sq += l * l;
            expected_right_sum_sq += r * r;
        }

        assert_eq!(
            expected_left_sum_sq.to_bits(),
            audio.left_sum_sq.to_bits(),
            "f32 bit-identity check failed for left_sum_sq"
        );
        assert_eq!(
            expected_right_sum_sq.to_bits(),
            audio.right_sum_sq.to_bits(),
            "f32 bit-identity check failed for right_sum_sq"
        );

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("/tmp/m0d-raw-{}.pcm", blob_id));
    }
}
