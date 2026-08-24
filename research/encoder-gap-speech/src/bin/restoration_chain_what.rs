//! ΜΕΤΡΗΣΗ 2026-08-24 — τι κάνει το RestorationChain, στην πράξη,
//! πάνω σε πραγματικό audiobook υλικό.
//!
//! Δύο μέρη, ΠΡΑΓΜΑΤΙΚΟΣ κώδικας και στα δύο:
//!  (a) `execute_streaming_plan` — η ΠΛΗΡΗΣ streaming διαδρομή
//!      (ό,τι πραγματικά ακούει ο χρήστης). Παράγει το master +
//!      ένα 20s κλιπ ακρόασης.
//!  (b) `RestorationChain::new` απομονωμένο, με αυξητικά configs
//!      (lowcut μόνο → +gate → +dehum → +deess), ΓΙΑ να πάρουμε
//!      per-stage null test — η πλήρης διαδρομή δεν εκθέτει
//!      per-stage taps, οπότε αυτό είναι ο μόνος τρόπος να
//!      απομονωθεί κάθε στάδιο χωρίς re-implementation.
//!
//! usage: restoration_chain_what <wav_clip_path> <out_dir>

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::dsp::lazy_reader::LazyAudioReader;
use sp314_dsp::restoration::{RestorationChain, RestorationConfig};
use std::io::Write;

fn sha256_of<P: AsRef<std::path::Path>>(path: P) -> String {
    let out = std::process::Command::new("sha256sum")
        .arg(path.as_ref())
        .output()
        .expect("sha256sum");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or("?")
        .to_string()
}

fn rms_db(sig: &[f32]) -> f32 {
    let sum_sq: f64 = sig.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum_sq / sig.len().max(1) as f64).sqrt();
    if rms > 1e-10 {
        (20.0 * rms.log10()) as f32
    } else {
        -144.0
    }
}

/// RMS-of-delta in dB between two equal-length signals — how much a
/// stage actually changed the signal, not just "on vs off" labels.
fn delta_rms_db(a: &[f32], b: &[f32]) -> f32 {
    let delta: Vec<f32> = a.iter().zip(b.iter()).map(|(x, y)| x - y).collect();
    rms_db(&delta)
}

fn write_wav_mono(path: &str, sig: &[f32], sr: u32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("wav create");
    for &s in sig {
        w.write_sample(s).expect("wav write");
    }
    w.finalize().expect("wav finalize");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: restoration_chain_what <wav_clip_path> <out_dir>");
    let wav_path = args[0].clone();
    let out_dir = std::path::PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out_dir).unwrap();

    // ── (a) ΠΛΗΡΗΣ streaming διαδρομή — ό,τι πραγματικά ακούει ο χρήστης ──
    eprintln!("=== (a) execute_streaming_plan — ΠΛΗΡΗΣ streaming διαδρομή ===");
    let plan = StreamingPlan {
        audio_path: wav_path.clone(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "restoration-what".to_string(),
    };
    let t0 = std::time::Instant::now();
    let result = execute_streaming_plan(&plan, None);
    let elapsed = t0.elapsed().as_secs_f64();

    match &result {
        Ok((_output, blob)) => {
            let master_path = m0d::blob_store::mastered_path(&blob.core.id);
            let sha = sha256_of(&master_path);
            eprintln!("  OK  blob_id={}  sha256={sha}  elapsed={elapsed:.2}s", blob.core.id);
            eprintln!("  master (raw pcm f32 stereo, presumed 48k): {}", master_path.display());

            // Copy the raw master out of the spool before anything can touch it.
            let saved_master = out_dir.join("full_streaming_master.pcm");
            std::fs::copy(&master_path, &saved_master).ok();
            eprintln!("  αντιγράφηκε -> {}", saved_master.display());

            // 20s audition clip via ffmpeg, from raw f32le stereo @48k.
            let clip_path = out_dir.join("audition_20s.wav");
            let status = std::process::Command::new("ffmpeg")
                .args([
                    "-y", "-f", "f32le", "-ar", "48000", "-ac", "2",
                    "-i",
                ])
                .arg(&saved_master)
                .args(["-t", "20", "-ss", "5"])
                .arg(&clip_path)
                .status();
            eprintln!("  20s κλιπ ακρόασης -> {} (ffmpeg status={:?})", clip_path.display(), status);
        }
        Err(e) => {
            eprintln!("  FAILED: {e:?}");
        }
    }

    // ── (b) RestorationChain απομονωμένο — per-stage null test ──
    eprintln!();
    eprintln!("=== (b) RestorationChain απομονωμένο — per-stage null test ===");
    let mut reader = LazyAudioReader::open(std::path::Path::new(&wav_path)).expect("open clip");
    let sr = reader.sample_rate();
    let ch = reader.channels();
    let mut interleaved = Vec::new();
    let mut buf = vec![0.0f32; 65536 * ch];
    loop {
        let got = reader.fill_buffer(&mut buf).expect("fill_buffer");
        if got == 0 {
            break;
        }
        interleaved.extend_from_slice(&buf[..got * ch]);
    }
    let n = interleaved.len() / ch;
    let mut left: Vec<f32> = (0..n).map(|i| interleaved[i * ch]).collect();
    let mut right: Vec<f32> = (0..n).map(|i| interleaved[i * ch + (ch - 1)]).collect();
    eprintln!("  decoded: sr={sr} ch={ch} n_frames={n} ({:.1}s)", n as f32 / sr as f32);

    // Reasonable measured-like gate threshold: -45 dBFS default policy
    // (same fallback the real orchestrator uses when trunk noise_floor_dbfs
    // is unavailable to this isolated harness — see restoration-contradiction.md §3).
    let gate_threshold_db = -45.0_f32;

    let dry_l = left.clone();
    let dry_r = right.clone();

    let configs: [(&str, RestorationConfig); 5] = [
        (
            "0_dry",
            RestorationConfig {
                lowcut_enabled: false,
                hum_enabled: false,
                deess_enabled: false,
                gate_enabled: false,
            },
        ),
        (
            "1_+lowcut",
            RestorationConfig {
                lowcut_enabled: true,
                hum_enabled: false,
                deess_enabled: false,
                gate_enabled: false,
            },
        ),
        (
            "2_+gate",
            RestorationConfig {
                lowcut_enabled: true,
                hum_enabled: false,
                deess_enabled: false,
                gate_enabled: true,
            },
        ),
        (
            "3_+dehum",
            RestorationConfig {
                lowcut_enabled: true,
                hum_enabled: true,
                deess_enabled: false,
                gate_enabled: true,
            },
        ),
        (
            "4_+deess(=voice())",
            RestorationConfig {
                lowcut_enabled: true,
                hum_enabled: true,
                deess_enabled: true,
                gate_enabled: true,
            },
        ),
    ];

    let mut prev_l = dry_l.clone();
    let mut prev_r = dry_r.clone();
    let mut prev_label = "dry";

    for (label, cfg) in &configs {
        let mut l = dry_l.clone();
        let mut r = dry_r.clone();
        let mut chain = RestorationChain::new(sr as f32, cfg.clone(), 0.0, gate_threshold_db);
        chain.process(&mut l, &mut r);

        let stage_delta_l = delta_rms_db(&prev_l, &l);
        let stage_delta_r = delta_rms_db(&prev_r, &r);
        let vs_dry_l = delta_rms_db(&dry_l, &l);

        eprintln!(
            "  {label:22} vs {prev_label:10} : ΔRMS(L)={stage_delta_l:7.2}dB ΔRMS(R)={stage_delta_r:7.2}dB   vs DRY(L)={vs_dry_l:7.2}dB  out_rms(L)={:7.2}dB",
            rms_db(&l)
        );

        let wav_out = out_dir.join(format!("stage_{label}.wav"));
        write_wav_mono(wav_out.to_str().unwrap(), &l, sr);

        prev_l = l;
        prev_r = r;
        prev_label = label;
    }

    let _ = std::io::stderr().flush();
    let _ = (&mut left, &mut right); // silence unused-mut if any path skips reassignment
}
