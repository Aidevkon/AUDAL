//! ΑΠΟΔΕΙΞΗ 2026-08-24 — impulse test στην ΠΛΗΡΗ ζωντανή διαδρομή.
//! Παίρνει πραγματικό 45s ομιλίας (ώστε το Scout να το ταξινομήσει ως
//! Speech και να ανάψει ΟΛΗ η αλυσίδα: RestorationChain + vocal_graph
//! LTASS), προσθέτει ένα δυνατό, εντοπίσιμο click σε γνωστό sample,
//! τρέχει μέσα από execute_streaming_plan (πραγματική συνάρτηση), και
//! μετράει ΠΟΥ βγαίνει η κορυφή στην έξοδο.
//!
//! usage: latency_impulse_test <clean_45s.wav> <out_dir>

use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use m0d::dsp::lazy_reader::LazyAudioReader;
use std::io::Write;

const IMPULSE_SAMPLE_OFFSET: usize = 20 * 48_000; // 20.0s στο πρωτότυπο
const IMPULSE_AMPLITUDE: f32 = 12.0; // πολύ πάνω από κανονική ομιλία (|x|<=1 τυπικά)
const IMPULSE_WIDTH: usize = 3; // 3 δείγματα, για να επιβιώσει ρεαλιστικά φίλτρα

fn write_wav_stereo(path: &str, left: &[f32], right: &[f32], sr: u32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("wav create");
    for i in 0..left.len() {
        w.write_sample(left[i]).unwrap();
        w.write_sample(right[i]).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "usage: latency_impulse_test <clean_45s.wav> <out_dir>");
    let clean_path = &args[0];
    let out_dir = std::path::PathBuf::from(&args[1]);
    std::fs::create_dir_all(&out_dir).unwrap();

    // ── Decode το καθαρό clip, πρόσθεσε το click ──
    let mut reader = LazyAudioReader::open(std::path::Path::new(clean_path)).expect("open clip");
    let sr = reader.sample_rate();
    let ch = reader.channels();
    assert_eq!(sr, 48_000, "expected 48k source");
    let mut interleaved = Vec::new();
    let mut buf = vec![0.0f32; 65536 * ch];
    loop {
        let got = reader.fill_buffer(&mut buf).unwrap();
        if got == 0 {
            break;
        }
        interleaved.extend_from_slice(&buf[..got * ch]);
    }
    let n = interleaved.len() / ch;
    let mut left: Vec<f32> = (0..n).map(|i| interleaved[i * ch]).collect();
    let mut right: Vec<f32> = (0..n).map(|i| interleaved[i * ch + (ch - 1)]).collect();

    assert!(
        IMPULSE_SAMPLE_OFFSET + IMPULSE_WIDTH < n,
        "clip too short for impulse offset"
    );
    for w in 0..IMPULSE_WIDTH {
        left[IMPULSE_SAMPLE_OFFSET + w] += IMPULSE_AMPLITUDE;
        right[IMPULSE_SAMPLE_OFFSET + w] += IMPULSE_AMPLITUDE;
    }

    let impulse_wav = out_dir.join("clip_with_impulse.wav");
    write_wav_stereo(impulse_wav.to_str().unwrap(), &left, &right, sr);
    let impulse_secs = IMPULSE_SAMPLE_OFFSET as f64 / sr as f64;
    eprintln!(
        "impulse εισήχθη σε sample {IMPULSE_SAMPLE_OFFSET} (={impulse_secs:.3}s), πλάτος +{IMPULSE_AMPLITUDE}, {IMPULSE_WIDTH} δείγματα"
    );
    eprintln!("γράφτηκε: {}", impulse_wav.display());

    // ── Τρέξε την ΠΛΗΡΗ ζωντανή διαδρομή ──
    let plan = StreamingPlan {
        audio_path: impulse_wav.to_str().unwrap().to_string(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "latency-impulse".to_string(),
    };
    let result = execute_streaming_plan(&plan, None);
    let (blob_id, master_path) = match &result {
        Ok((_output, blob)) => {
            let p = m0d::blob_store::mastered_path(&blob.core.id);
            eprintln!("OK  blob_id={}  sample_rate={}", blob.core.id, blob.core.sample_rate);
            (blob.core.id.clone(), p)
        }
        Err(e) => {
            eprintln!("FAILED: {e:?}");
            return;
        }
    };

    let saved = out_dir.join(format!("master_{blob_id}.pcm"));
    std::fs::copy(&master_path, &saved).ok();

    // ── Διάβασε raw f32le stereo 48k, βρες την κορυφή γύρω από το αναμενόμενο σημείο ──
    let raw = std::fs::read(&saved).expect("read master pcm");
    let n_samples = raw.len() / 4;
    let mut mono_out = Vec::with_capacity(n_samples / 2);
    let mut i = 0usize;
    while i + 16 <= raw.len() {
        let l = f32::from_le_bytes([raw[i], raw[i + 1], raw[i + 2], raw[i + 3]]);
        let r = f32::from_le_bytes([raw[i + 4], raw[i + 5], raw[i + 6], raw[i + 7]]);
        mono_out.push((l + r) * 0.5);
        i += 8;
    }
    eprintln!("output frames (stereo pairs): {}", mono_out.len());

    // Search window: ±2000 δείγματα γύρω από το αναμενόμενο σημείο (καλύπτει έως ~40ms πιθανή μετατόπιση)
    let search_lo = IMPULSE_SAMPLE_OFFSET.saturating_sub(2000);
    let search_hi = (IMPULSE_SAMPLE_OFFSET + 2000).min(mono_out.len());
    let mut best_idx = search_lo;
    let mut best_val = 0.0f32;
    for idx in search_lo..search_hi {
        let v = mono_out[idx].abs();
        if v > best_val {
            best_val = v;
            best_idx = idx;
        }
    }
    let shift = best_idx as i64 - IMPULSE_SAMPLE_OFFSET as i64;
    eprintln!();
    eprintln!("=== IMPULSE PEAK ===");
    eprintln!("είσοδος (πριν το κόψιμο):  sample {IMPULSE_SAMPLE_OFFSET}");
    eprintln!("έξοδος (κορυφή βρέθηκε):  sample {best_idx}  (|value|={best_val:.4})");
    eprintln!("ΜΕΤΑΤΟΠΙΣΗ (samples): {shift:+}");
    eprintln!(
        "ΜΕΤΑΤΟΠΙΣΗ (ms @48k): {:+.3}",
        shift as f64 / 48.0
    );

    // ── Cross-correlation ως backup/επιβεβαίωση, τοπικό παράθυρο ──
    let win = 4000usize;
    let clean_win_start = IMPULSE_SAMPLE_OFFSET.saturating_sub(win / 2);
    let clean_ref: Vec<f32> = (0..win)
        .map(|k| {
            let idx = clean_win_start + k;
            if idx < left.len() {
                (left[idx] + right[idx]) * 0.5
            } else {
                0.0
            }
        })
        .collect();

    let corr_search_lo = clean_win_start.saturating_sub(2000);
    let corr_search_hi = (clean_win_start + win + 2000).min(mono_out.len());
    let mut best_lag = 0i64;
    let mut best_corr = f64::MIN;
    for out_start in corr_search_lo..corr_search_hi.saturating_sub(win) {
        let mut acc = 0.0f64;
        for k in 0..win {
            acc += (clean_ref[k] as f64) * (mono_out[out_start + k] as f64);
        }
        if acc > best_corr {
            best_corr = acc;
            best_lag = out_start as i64 - clean_win_start as i64;
        }
    }
    eprintln!();
    eprintln!("=== CROSS-CORRELATION (τοπικό παράθυρο {win} δειγμάτων γύρω από το impulse) ===");
    eprintln!("καλύτερο lag (samples): {best_lag:+}  (corr={best_corr:.2})");

    let _ = std::io::stderr().flush();
}
