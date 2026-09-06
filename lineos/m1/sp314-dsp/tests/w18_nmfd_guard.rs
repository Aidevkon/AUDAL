use sp314_dsp::analysis::mel_128::fold_to_mel;
use sp314_dsp::stft::nmfd::nmfd_f32_h_only;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::StftEngine;
use std::path::Path;

fn read_audio_mono(path: &Path) -> (Vec<f32>, u32) {
    let mut reader = hound::WavReader::open(path).expect(&format!("Cannot open wav: {:?}", path));
    let spec = reader.spec();
    let samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader.samples::<f32>().map(|s| s.unwrap()).collect()
    } else {
        reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
    };
    let mut mono = Vec::new();
    if spec.channels == 2 {
        for i in 0..(samples.len() / 2) {
            mono.push((samples[2 * i] + samples[2 * i + 1]) * 0.5);
        }
    } else {
        mono = samples;
    }
    (mono, spec.sample_rate)
}

fn run_pipeline_for_activations(signal: &[f32], sample_rate: u32, profile: Option<sp314_dsp::spatial::user_profile::UserSpatialProfile>) -> (Vec<f64>, usize) {
    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout_with_profile(signal, signal, sample_rate, None, None, true, profile);

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(signal);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; 1025];
        for b in 0..1025 {
            mag_frame[b] = cplx_frames[f][b].norm();
        }
        let mel_frame = fold_to_mel(&mag_frame);
        for b in 0..128 {
            c_v[b * n_frames + f] = mel_frame[b];
        }
    }

    let nmfd_k = scout.tensor_w.len() / (128 * 8);
    let nmfd_iter = 12;
    let init_val = 0.1_f32;
    let init_h = vec![init_val; nmfd_k * n_frames];

    for c in 0..nmfd_k {
        let mut sum_w = 0.0_f64;
        for m in 0..128 {
            for tau in 0..scout.tau {
                sum_w += scout.tensor_w[m * nmfd_k * scout.tau + c * scout.tau + tau] as f64;
            }
        }
        println!("DEBUG W sum for slot {}: {:.6}", c, sum_w);
    }

    let (nmfd_h, _) = nmfd_f32_h_only(
        &c_v,
        &scout.tensor_w,
        &init_h,
        128,
        nmfd_k,
        n_frames,
        scout.tau,
        nmfd_iter,
    );

    let mut mean_h = vec![0.0f64; nmfd_k];
    for c in 0..nmfd_k {
        let mut sum = 0.0f64;
        for f in 0..n_frames {
            sum += nmfd_h[c * n_frames + f] as f64;
        }
        mean_h[c] = if n_frames > 0 { sum / n_frames as f64 } else { 0.0 };
    }

    (mean_h, n_frames)
}

#[test]
fn test_w18_nmfd_guard() {
    let speech_path = Path::new("/tmp/w9/podcast_realistic.wav");
    let music_path = Path::new("/tmp/blue/comp0.wav");

    // ⚠ F-071: ΠΕΡΝΑΕΙ ΚΕΝΟ ΣΤΟ CI. Το fixture
    // /tmp/w9/podcast_realistic.wav δεν υπάρχει και ΔΕΝ ΑΝΑΠΑΡΑΓΕΤΑΙ
    // (F-072: συνταγή μόνο περιγραφική στο 52e2a31, το rebuild δίνει ΑΛΛΟ αρχείο).
    // ΔΕΝ γίνεται panic ΓΙΑΤΙ τρέχει σε τρία CI workflows με
    // cargo test --workspace (ci.yml:59 · constitutional-gates.yml:61 ·
    // red-freeze.yml:37)· μόνιμα κόκκινο CI είναι ο ίδιος μηχανισμός με
    // μόνιμα πράσινο ψεύτικο.
    // ΞΥΠΝΑΕΙ ΟΤΑΝ: το fixture μπει in-repo (F-072).
    // ΜΕΤΡΙΕΤΑΙ ΑΠΟ: scripts/empty-pass-lint.sh
    if !speech_path.exists() {
        println!("SKIPPED: speech missing");
        return;
    }
    
    // Take 30s max
    let (mut speech_sig, speech_sr) = read_audio_mono(speech_path);
    speech_sig.truncate(speech_sr as usize * 30);
    
    let (mean_h_speech, _) = run_pipeline_for_activations(&speech_sig, speech_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_podcast()));

    println!("=== PODCAST ACTIVATIONS ===");
    for c in 0..mean_h_speech.len() {
        println!("Slot {}: {:.6}", c, mean_h_speech[c]);
    }
    
    let max_speech = (0..4).map(|i| mean_h_speech[i]).fold(0.0f64, |a, b| a.max(b));
    let max_drums = if mean_h_speech.len() > 6 { (4..7).map(|i| mean_h_speech[i]).fold(0.0f64, |a, b| a.max(b)) } else { 0.0 };
    
    let ratio = max_drums / max_speech.max(1e-12);
    println!("MAX SPEECH (0-3): {:.6}", max_speech);
    println!("MAX DRUMS (4-6) (Free slots): {:.6}", max_drums);
    
    assert_eq!(mean_h_speech.len(), 8, "Podcast should completely bypass drum templates");
    println!("PODCAST GUARD: PASS (bypassed)");
    
    if music_path.exists() {
        let (mut music_sig, music_sr) = read_audio_mono(music_path);
        music_sig.truncate(music_sr as usize * 30);
        
        let (mean_h_music, _) = run_pipeline_for_activations(&music_sig, music_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
        assert_eq!(mean_h_music.len(), 14, "Music should load 6 music templates");
        
        let max_drums = (4..=6).map(|i| mean_h_music[i]).fold(0.0f64, |a, b| a.max(b));
        let routed_sung = mean_h_music[7];
        let max_free = (10..=13).map(|i| mean_h_music[i]).fold(0.0f64, |a, b| a.max(b));
        
        let r1 = max_drums / max_free.max(1e-12);
        let r2 = routed_sung / max_free.max(1e-12);
        let soak = mean_h_music[8] / max_free.max(1e-12);
        let discarded_c2 = mean_h_music[9] / max_free.max(1e-12);

        println!("SYNTHETIC R1 (drums): {:.4}", r1);
        println!("SYNTHETIC R2 (routed-sung C0): {:.4}", r2);
        println!("INFO: soak C1: {:.4}", soak);
        println!("INFO: discarded C2: {:.4}", discarded_c2);
        
        assert!(r1 <= 0.08, "Synthetic track triggered drum templates! R1: {:.4}", r1);
        assert!(r2 <= 0.08, "Synthetic track triggered routed-sung template C0! R2: {:.4}", r2);
        println!("SYNTHETIC GUARD: PASS");
    }
    
    let acoustic_path = Path::new("tests/fixtures/am_contra_30s.wav");
    if acoustic_path.exists() {
        let (mut acoustic_sig, acoustic_sr) = read_audio_mono(acoustic_path);
        
        let (mean_h_ac, _) = run_pipeline_for_activations(&acoustic_sig, acoustic_sr, Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music()));
        assert_eq!(mean_h_ac.len(), 14, "Music should load 6 music templates");
        
        let max_drums = (4..10).map(|i| mean_h_ac[i]).fold(0.0f64, |a, b| a.max(b));
        assert!(max_drums > 1e-6, "Acoustic track should have healthy drum activations, got {:e}", max_drums);
        println!("ACOUSTIC DRUMS GUARD: PASS");
    }
}

#[test]
fn test_k14_e2e_hash() {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;

    let acoustic_path = Path::new("tests/fixtures/am_contra_30s.wav");
    assert!(acoustic_path.exists(), "Missing fixture: {} — in-repo tracked fixture — αν λείπει, το checkout είναι ελλιπές (git lfs / sparse checkout;)", acoustic_path.display());

    let (mut acoustic_sig, acoustic_sr) = read_audio_mono(acoustic_path);
    // Use first 5 seconds to keep the test fast
    acoustic_sig.truncate(acoustic_sr as usize * 5);

    let mut two_pass = TwoPassEngine::new();
    let scout = two_pass.scout_with_profile(
        &acoustic_sig, 
        &acoustic_sig, 
        acoustic_sr, 
        None, 
        None, 
        true, 
        Some(sp314_dsp::spatial::user_profile::UserSpatialProfile::default_music())
    );

    let mut stft = StftEngine::new();
    let (cplx_frames, n_frames) = stft.forward(&acoustic_sig);

    let mut c_v = vec![0.0_f32; 128 * n_frames];
    for f in 0..n_frames {
        let mut mag_frame = [0.0_f32; 1025];
        for b in 0..1025 {
            mag_frame[b] = cplx_frames[f][b].norm();
        }
        let mel_frame = fold_to_mel(&mag_frame);
        for b in 0..128 {
            c_v[b * n_frames + f] = mel_frame[b];
        }
    }

    let nmfd_k = 14;
    let init_h = vec![0.1_f32; nmfd_k * n_frames];

    let (nmfd_h, _) = nmfd_f32_h_only(
        &c_v,
        &scout.tensor_w,
        &init_h,
        128,
        nmfd_k,
        n_frames,
        scout.tau,
        12,
    );

    // Hash the resulting activations (nmfd_h) as a proxy for the entire e2e state
    let mut hasher = DefaultHasher::new();
    for &val in &nmfd_h {
        // Hash the bit representation to ensure perfect determinism
        val.to_bits().hash(&mut hasher);
    }
    let hash_val = hasher.finish();

    println!("K=14 e2e Hash (5s am_contra): {}", hash_val);
    
    // We just enforce that we don't crash and we get a hash.
    // If the golden changes, this will catch it in CI if we hardcode it.
    assert_eq!(hash_val, 16101438349242031503);
}
