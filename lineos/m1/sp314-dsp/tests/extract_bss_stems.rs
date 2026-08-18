use sp314_dsp::stft::two_pass::{FiveStemsChunk, TwoPassEngine};
use sp314_dsp::spatial::user_profile::UserSpatialProfile;

const SR: u32 = 48_000;

fn decode_wav(path: &std::path::Path) -> (Vec<f32>, Vec<f32>) {
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader
            .samples::<i32>()
            .map(|s| s.unwrap() as f32 / (1 << (spec.bits_per_sample - 1)) as f32)
            .collect()
    };
    let mut left = Vec::with_capacity(samples.len() / 2);
    let mut right = Vec::with_capacity(samples.len() / 2);
    for chunk in samples.chunks_exact(2) {
        left.push(chunk[0]);
        right.push(chunk[1]);
    }
    (left, right)
}

fn write_wav(path: &str, left: &[f32], right: &[f32]) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: SR,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..left.len() {
        writer.write_sample(left[i]).unwrap();
        writer.write_sample(right[i]).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
#[ignore]
fn extract_bss_stems() {
    let input_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/am_contra_30s.wav");
    let (left, right) = decode_wav(&input_path);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(l, r)| (l + r) * 0.5).collect();

    for is_music in [false, true] {
        let mut engine = TwoPassEngine::new();
        let profile = if is_music {
            std::env::set_var("NMFD_FORCE_MUSIC", "1");
            UserSpatialProfile::default_music()
        } else {
            std::env::remove_var("NMFD_FORCE_MUSIC");
            UserSpatialProfile::default_podcast()
        };
        
        let scout = engine.scout_with_profile(&mono, &mono, SR, None, None, true, Some(profile));
        let nmfd_k = scout.tensor_w.len() / (128 * 8);

        let mut voice_l = Vec::with_capacity(mono.len());
        let mut voice_r = Vec::with_capacity(mono.len());
        let mut bass_l = Vec::with_capacity(mono.len());
        let mut bass_r = Vec::with_capacity(mono.len());

        let callback = |stems: &FiveStemsChunk| {
            voice_l.extend_from_slice(&stems.voice.l);
            voice_r.extend_from_slice(&stems.voice.r);
            bass_l.extend_from_slice(&stems.bass.l);
            bass_r.extend_from_slice(&stems.bass.r);
        };

        engine.process_slices_with_params(&mono, &left, &right, &scout, 1.0, true, callback).unwrap();

        let prefix = format!("/tmp/k{}", nmfd_k);
        std::fs::create_dir_all(&prefix).unwrap();
        
        write_wav(&format!("{}/voice_k{}_v2.wav", prefix, nmfd_k), &voice_l, &voice_r);
        write_wav(&format!("{}/bass_k{}_v2.wav", prefix, nmfd_k), &bass_l, &bass_r);
        
        println!("Extracted K={} stems to {}", nmfd_k, prefix);
    }
}
