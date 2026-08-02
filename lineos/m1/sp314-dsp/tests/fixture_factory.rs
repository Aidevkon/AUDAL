#[cfg(test)]
mod tests {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    use sha2::{Digest, Sha256};
    use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
    use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
    use sp314_dsp::io::flac_writer::FlacWriter;
    use sp314_dsp::metering::lufs::measure_integrated_lufs;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    struct Lcg {
        state: u64,
    }
    impl Lcg {
        fn new(seed: u64) -> Self {
            Lcg { state: seed }
        }
        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            self.state
        }
        fn next_float(&mut self) -> f32 {
            (self.next() >> 40) as f32 / 16777216.0
        }
    }

    fn hash_file(path: &str) -> String {
        let data = std::fs::read(path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        format!("{:x}", hasher.finalize())
    }

    fn get_audio_excerpt(path: &str, target_sr: u32, duration_sec: f32, rng: &mut Lcg) -> Vec<f32> {
        let file = Box::new(File::open(path).unwrap());
        let mss = MediaSourceStream::new(file, Default::default());
        let hint = Hint::new();
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap();
        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        let track_id = track.id;
        let original_sr = track.codec_params.sample_rate.unwrap();

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .unwrap();

        let mut mono_samples = Vec::new();
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(_) => break,
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let channels = decoded.spec().channels.count();
                    let mut sample_buf =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    sample_buf.copy_interleaved_ref(decoded.clone());
                    let samples = sample_buf.samples();
                    for i in (0..samples.len()).step_by(channels) {
                        let mut sum = 0.0;
                        for c in 0..channels {
                            sum += samples[i + c];
                        }
                        mono_samples.push(sum / channels as f32);
                    }
                }
                Err(_) => break,
            }
        }

        let req_samples = (duration_sec * original_sr as f32) as usize;
        let max_offset = mono_samples.len().saturating_sub(req_samples);
        let offset = (rng.next_float() * max_offset as f32) as usize;
        let excerpt =
            mono_samples[offset..std::cmp::min(offset + req_samples, mono_samples.len())].to_vec();

        if original_sr == target_sr {
            return excerpt;
        }

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler = SincFixedIn::<f32>::new(
            target_sr as f64 / original_sr as f64,
            2.0,
            params,
            excerpt.len(),
            1,
        )
        .unwrap();

        let waves_in = vec![excerpt];
        let waves_out = resampler.process(&waves_in, None).unwrap();
        waves_out[0].clone()
    }

    fn evaluate_vad(audio: &[f32]) -> f32 {
        let mut ext = VadFeatureExtractor::new();
        let feats = ext.process_chunk(audio, audio, audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &feats {
            if vad.process(f, -144.0).is_speech {
                speech_frames += 1;
            }
        }
        100.0 * speech_frames as f32 / feats.len().max(1) as f32
    }

    #[test]
    #[ignore]
    fn build_fixtures() {
        let base_out = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
        std::fs::create_dir_all(base_out).unwrap();

        let mut rng = Lcg::new(314159);
        let target_sr = 48000;

        let voices = [
            "/tmp/fixture-factory/voice1.mp3",
            "/tmp/fixture-factory/voice2.mp3",
            "/tmp/fixture-factory/voice3.mp3",
        ];
        let musics = [
            "/tmp/fixture-factory/music1.mp3",
            "/tmp/fixture-factory/music2.mp3",
            "/tmp/fixture-factory/music3.mp3",
        ];

        let mut manifest: HashMap<String, serde_json::Value> = HashMap::new();
        manifest.insert("seed".to_string(), serde_json::json!(314159));
        manifest.insert(
            "resampler".to_string(),
            serde_json::json!("rubato SincFixedIn"),
        );

        let mut outputs = HashMap::new();

        for i in 0..3 {
            let v_cut = get_audio_excerpt(voices[i], target_sr as u32, 25.0, &mut rng);
            let v_lufs = measure_integrated_lufs(&v_cut, &v_cut);

            let m_cut = get_audio_excerpt(musics[i], target_sr as u32, 25.0, &mut rng);
            let m_lufs = measure_integrated_lufs(&m_cut, &m_cut);

            let min_len = std::cmp::min(v_cut.len(), m_cut.len());
            let v_cut = &v_cut[..min_len];
            let m_cut = &m_cut[..min_len];

            let v_out_path = format!("{}/voice_{}.flac", base_out, i + 1);
            FlacWriter::write(&v_out_path, v_cut, v_cut, target_sr as u32).unwrap();
            outputs.insert(
                format!("voice_{}.flac", i + 1),
                serde_json::json!(hash_file(&v_out_path)),
            );

            let snrs = [-6.0, -15.0, -25.0];
            for &snr in &snrs {
                let target_m_lufs = v_lufs + snr;
                let m_gain = 10.0_f32.powf((target_m_lufs - m_lufs) / 20.0);

                let mut m_scaled = vec![0.0; min_len];
                let mut mix = vec![0.0; min_len];
                for j in 0..min_len {
                    m_scaled[j] = m_cut[j] * m_gain;
                    mix[j] = v_cut[j] + m_scaled[j];
                }

                for j in 0..min_len {
                    mix[j] = mix[j].clamp(-1.0, 1.0);
                    m_scaled[j] = m_scaled[j].clamp(-1.0, 1.0);
                }

                let mix_name = format!("mix_{}_snr{}.flac", i + 1, snr as i32);
                let mix_path = format!("{}/{}", base_out, mix_name);
                FlacWriter::write(&mix_path, &mix, &mix, target_sr as u32).unwrap();

                let mut mix_stats = serde_json::Map::new();
                mix_stats.insert("hash".to_string(), serde_json::json!(hash_file(&mix_path)));
                mix_stats.insert("voice_lufs".to_string(), serde_json::json!(v_lufs));
                mix_stats.insert("music_original_lufs".to_string(), serde_json::json!(m_lufs));
                mix_stats.insert(
                    "music_target_lufs".to_string(),
                    serde_json::json!(target_m_lufs),
                );
                mix_stats.insert("music_gain_linear".to_string(), serde_json::json!(m_gain));
                outputs.insert(mix_name.clone(), serde_json::Value::Object(mix_stats));

                let m_name = format!("music_{}_snr{}.flac", i + 1, snr as i32);
                let m_out_path = format!("{}/{}", base_out, m_name);
                FlacWriter::write(&m_out_path, &m_scaled, &m_scaled, target_sr as u32).unwrap();
                outputs.insert(m_name, serde_json::json!(hash_file(&m_out_path)));

                let vad_mix = evaluate_vad(&mix);
                println!("FACTORY|vad|file={}|speech_pct={:.2}", mix_name, vad_mix);
            }

            let vad_voice = evaluate_vad(v_cut);
            println!(
                "FACTORY|vad|file=voice_{}.flac|speech_pct={:.2}",
                i + 1,
                vad_voice
            );
        }

        manifest.insert(
            "hashes".to_string(),
            serde_json::to_value(&outputs).unwrap(),
        );
        let manifest_path = format!("{}/manifest.json", base_out);
        let mut manifest_file = File::create(&manifest_path).unwrap();
        manifest_file
            .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
            .unwrap();

        println!("Determinism gate hashes:");
        let mut keys: Vec<_> = outputs.keys().collect();
        keys.sort();
        for k in &keys {
            println!("{}: {}", k, outputs[*k]);
        }
    }
}
