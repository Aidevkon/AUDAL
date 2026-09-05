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

fn measure_rms_db_silent(signal: &[f32], sr: u32) -> f32 {
    let silent_secs = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 22, 23];
    let mut powers = Vec::new();
    
    for &sec in &silent_secs {
        let start_idx = sec * sr as usize;
        let end_idx = (sec + 1) * sr as usize;
        if start_idx >= signal.len() { continue; }
        let actual_end = end_idx.min(signal.len());
        let slice = &signal[start_idx..actual_end];
        
        let mut sum_sq = 0.0;
        for &s in slice {
            sum_sq += s * s;
        }
        let rms = (sum_sq / slice.len() as f32).sqrt();
        if rms < 1e-10 {
            powers.push(1e-10_f32);
        } else {
            powers.push(rms * rms); // power
        }
    }
    
    let mean_power = powers.iter().sum::<f32>() / powers.len() as f32;
    10.0 * mean_power.log10()
}

// Known legacy leak, discovered 2026-08-18 listening trial.
// Cure: voice-H gating — see ledger.
// This test is the cure's baseline.
#[test]
#[ignore = "baseline της θεραπείας γνωστής διαρροής (δες σχόλιο από πάνω: 18/08, cure = voice-H gating), ΟΧΙ ελεύθερος φρουρός· ΑΡΓΟ ~43s στο in-repo am_contra_30s.wav. Το ξυπνά: scripts/run-ignored.sh"]
fn test_w19_silence_leak() {
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

        let callback = |stems: &FiveStemsChunk| {
            voice_l.extend_from_slice(&stems.voice.l);
            voice_r.extend_from_slice(&stems.voice.r);
        };

        engine.process_slices_with_params(&mono, &left, &right, &scout, 1.0, true, callback).unwrap();
        
        let voice_mono: Vec<f32> = voice_l.iter().zip(voice_r.iter()).map(|(l, r)| (l + r) * 0.5).collect();
        
        // Measure silent seconds 
        let rms_db = measure_rms_db_silent(&voice_mono, SR);
        
        println!("K={} Silence RMS: {:.2} dB", nmfd_k, rms_db);
        
        if nmfd_k == 8 {
            let target = -35.66; // Equivalent to Python's -38.9 due to stereo downmix scaling
            assert!((rms_db - target).abs() <= 1.5, "K=8 leak outside tolerance: {} dB (target {})", rms_db, target);
        } else if nmfd_k == 14 {
            let target = -34.30; // Equivalent to Python's -37.6 due to stereo downmix scaling
            assert!((rms_db - target).abs() <= 1.5, "K=14 leak outside tolerance: {} dB (target {})", rms_db, target);
        } else {
            panic!("Unexpected K={}", nmfd_k);
        }
    }
}
