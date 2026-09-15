//! ΕΠΑΛΗΘΕΥΣΗ, ΟΧΙ ΜΟΝΙΜΟ ΤΕΣΤ: τρέχει ΠΡΑΓΜΑΤΙΚΑ renders μέσω
//! `agents::executor::execute_streaming_plan` — η ΙΔΙΑ συνάρτηση που
//! χτίζει το `corrections` block (executor.rs) — και τυπώνει το
//! ΠΡΑΓΜΑΤΙΚΟ JSON. Για το IMPLEMENT task "ο ανιχνευτής βόμβου γράφει
//! το δεύτερο record στο corrections".
//!
//! ΣΗΜΕΙΩΣΗ: dsp_pipeline::run_dsp (offline path, certificate_node::run)
//! ΔΕΝ περνάει από αυτό το corrections block — μόνο η streaming
//! διαδρομή (certificate_node::run_streaming, agents/executor.rs) το
//! χτίζει. Ίδιο idiom με batch.rs:209-222 (StreamingPlan +
//! execute_streaming_plan).

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::blob_store::BlobVariant;

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

fn run_once(wav_path: &str, preset: &str, session_id: &str) -> m0d::blob_store::StoredBlobV2 {
    let plan = StreamingPlan {
        audio_path: wav_path.to_string(),
        preset_id: preset.to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: session_id.to_string(),
    };
    let (_output, blob) = execute_streaming_plan(&plan, None).expect("execute_streaming_plan failed");
    blob
}

fn print_corrections(label: &str, blob: &m0d::blob_store::StoredBlobV2) {
    println!("=== {} ===", label);
    match &blob.variant {
        BlobVariant::Certified { corrections, .. } => {
            println!("{}", serde_json::to_string_pretty(corrections).unwrap());
        }
        BlobVariant::Uncertified { reason } => {
            println!("Uncertified: {:?}", reason);
        }
    }
}

fn main() {
    let sr = 48_000;

    let wav_path = "/tmp/corrections_verify_acx.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path);
    let blob = run_once(wav_path, "acx", "corrections-verify-acx");
    print_corrections("preset=acx (declares floor+edge)", &blob);

    let wav_path2 = "/tmp/corrections_verify_spotify.wav";
    write_wav(&generate_speech_like_fixture(sr, 12.0), sr, wav_path2);
    let blob2 = run_once(wav_path2, "spotify", "corrections-verify-spotify");
    print_corrections("preset=spotify (no floor declared)", &blob2);
}
