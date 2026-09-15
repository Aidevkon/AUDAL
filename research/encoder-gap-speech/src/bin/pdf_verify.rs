//! ΕΠΑΛΗΘΕΥΣΗ, ΟΧΙ ΜΟΝΙΜΟ ΤΕΣΤ: παράγει ένα πραγματικό certificate PDF από
//! ΑΦΗΓΗΣΗ (preset "acx" -> ContentType::Episode -> skip_stems() ==
//! true -> bypassed_render() -> StemFingerprints::default()), εξάγει το
//! κείμενό του (pdftotext) και το τυπώνει αυτούσιο, για το IMPLEMENT
//! task "το PDF παύει να ισχυρίζεται ό,τι δεν ελέγχει" (F-076).
//!
//! Ίδιο fixture idiom με tests/e2e_acx_certificate.rs
//! (generate_speech_like_fixture) — δεν επινοεί νέο υλικό.

use m0d::handlers::master::MasterRequest;

fn generate_speech_like_fixture(sr: u32, dur_secs: f32) -> Vec<f32> {
    let n = (sr as f32 * dur_secs) as usize;
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f32 / sr as f32;
        let in_burst = (t % 2.0) < 1.0;
        let v = if in_burst {
            0.1 * (2.0 * std::f32::consts::PI * 200.0 * t).sin()
                + 0.03 * (2.0 * std::f32::consts::PI * 600.0 * t).sin()
        } else {
            0.000_3 * (2.0 * std::f32::consts::PI * 120.0 * t).sin()
        };
        out.push(v);
        out.push(v);
    }
    out
}

fn write_wav(samples: &[f32], sr: u32, path: &str) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for &s in samples {
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let sr = 48_000;
    let wav_path = "/tmp/pdf_verify_narration.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path);

    let req = MasterRequest {
        audio_path: wav_path.to_string(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: Some("pdf_verify".to_string()),
        track_id: Some("pdf-verify-1".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };
    let state = std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
        xaak::repo::DspState::default(),
    ));
    let state_tmp = tempfile::TempDir::new().unwrap();
    let (blob, _, _, _, _, _artifacts) = m0d::domain::dsp_pipeline::run_dsp(
        &req,
        std::time::Instant::now(),
        state,
        None,
        None,
        "pdf-verify-1".to_string(),
        state_tmp.path().to_str().unwrap(),
        "/tmp",
    )
    .expect("run_dsp failed");

    println!(
        "stem_fingerprints = {:?}",
        blob.stem_fingerprints()
    );

    let pdf_path = "/tmp/pdf_verify_narration_certificate.pdf";
    m0d::handlers::pdf_gen::generate_silent_certificate(&blob, pdf_path);

    let out = std::process::Command::new("pdftotext")
        .arg(pdf_path)
        .arg("-")
        .output()
        .expect("pdftotext failed to run");
    println!("=== pdftotext exit: {} ===", out.status);
    println!("=== BEGIN PDF TEXT ===");
    print!("{}", String::from_utf8_lossy(&out.stdout));
    println!("=== END PDF TEXT ===");
}
