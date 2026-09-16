//! RECON 2026-09-16: πού ακριβώς διαφωνούν scan_file (whole-buffer)
//! και run_trunk_pass (streaming reader) — F-116 μέτρησε 79 έναντι 75
//! τμήματα στο ΙΔΙΟ dump και δήλωσε την αιτία αδιερεύνητη. Read-only,
//! ΓΡΑΜΜΕΣ ΑΥΤΟΥΣΙΕΣ, ΜΗΔΕΝ αλλαγή.
//!
//! ΔΕΝ τροποποιεί το trunk_pass.rs — αντίγραφο, γραμμή-προς-γραμμή,
//! ΜΟΝΟ της λογικής παραθύρωσης/MFCC-ring (trunk_pass.rs:629-884,
//! χωρίς τα άσχετα meters/VAD/spectral) — χρησιμοποιεί το ΙΔΙΟ,
//! ΠΡΑΓΜΑΤΙΚΟ RawPcmFileSource (sp314_dsp::stft::raw_pcm_source,
//! ΙΔΙΟ reader με την παραγωγή, ΟΧΙ δικός μας) και τις ΙΔΙΕΣ
//! SegmentScout/MfccAnalyzer/mfcc_euclidean_distance/
//! compute_scout_decision.
//!
//! Η ΔΙΑΦΟΡΑ ΠΟΥ ΒΡΕΘΗΚΕ ΔΙΑΒΑΖΟΝΤΑΣ ΤΟΝ ΚΩΔΙΚΑ, ΠΡΙΝ ΤΗ ΜΕΤΡΗΣΗ
//! (item 3, δηλωμένο εκ των προτέρων): το cepstral_flux στο
//! streaming path (trunk_pass.rs:820-845) είναι ΚΥΛΙΟΜΕΝΟΣ ΜΕΣΟΣ
//! ΟΡΟΣ πάνω σε ΕΩΣ 466 αποστάσεις MFCC-πλαισίων, με `mfcc_prev`
//! που ΔΕΝ μηδενίζεται ΠΟΤΕ — επιβιώνει σε όλο το αρχείο. Στο
//! scan_file (scout_scanner.rs:51, verified στο F-110), κάθε
//! παράθυρο φτιάχνει ΝΕΟ MfccAnalyzer και υπολογίζει το flux ΜΟΝΟ
//! από τα δικά του ~469 πλαίσια — ΜΗΔΕΝ ιστορικό. Δύο θεμελιωδώς
//! διαφορετικοί υπολογισμοί, όχι απλώς διαφορετικό I/O.
//!
//! ΧΡΗΣΗ: cargo run --release --bin io_layer_diag

use lineos_corpus::mfcc::MfccAnalyzer;
use lineos_corpus::scout::{compute_cepstral_flux, compute_scout_decision, mfcc_euclidean_distance};
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_dsp::stft::raw_pcm_source::RawPcmFileSource;
use sp314_dsp::stft::sliding_overlap_reader::ChunkSource;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const CHUNK_FRAMES: usize = 4096; // ΑΥΤΟΥΣΙΟ trunk_pass.rs:193

struct Window {
    t: f32,
    cv_ioi: f32,
    cepstral_flux: f32,
}

/// Path A: scan_file-συμβατό, whole-buffer. ΑΝΤΙΓΡΑΦΟ verified στο
/// F-110 (0 αναντιστοιχίες έναντι της πραγματικής scan_file).
fn windows_whole_buffer(mono: &[f32], sample_rate: u32) -> Vec<Window> {
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize;
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize;
    let mut scout = SegmentScout::new();
    let mut out = Vec::new();
    let mut start = 0usize;
    while start + win_samples <= mono.len() {
        let mono_slice = &mono[start..start + win_samples];
        let start_sec = start as f32 / sample_rate as f32;

        let mut mfcc_analyzer = MfccAnalyzer::new(); // ΝΕΟΣ ανά παράθυρο — ΜΗΔΕΝ ιστορικό
        let mut mfccs = Vec::new();
        let mut f = 0;
        while f + 1024 <= mono_slice.len() {
            mfccs.push(mfcc_analyzer.compute(&mono_slice[f..f + 1024]));
            f += 512;
        }
        let cepstral_flux = compute_cepstral_flux(&mfccs);
        let meas = scout.measure(mono_slice, cepstral_flux, sample_rate);
        out.push(Window { t: start_sec, cv_ioi: meas.cv_ioi, cepstral_flux: meas.cepstral_flux });
        start += hop_samples;
    }
    out
}

/// Path B: ΑΝΤΙΓΡΑΦΟ, γραμμή-προς-γραμμή, της παραθύρωσης+MFCC-ring
/// του trunk_pass.rs (γρ.629-884) — ΤΟ ΠΡΑΓΜΑΤΙΚΟ RawPcmFileSource,
/// ΤΟ ΠΡΑΓΜΑΤΙΚΟ SegmentScout/MfccAnalyzer/mfcc_euclidean_distance/
/// compute_scout_decision. Μόνη παράλειψη: τα meters/VAD/spectral
/// που δεν επηρεάζουν το cv_ioi/cepstral_flux/decisions.
fn windows_streaming(dump_path: &str, sample_rate: u32) -> Vec<Window> {
    let mut source = RawPcmFileSource::new(std::path::Path::new(dump_path), 2).expect("open dump");
    let win_samples = (WINDOW_SECS * sample_rate as f32) as usize; // trunk_pass.rs:634
    let hop_samples = (HOP_SECS * sample_rate as f32) as usize; // trunk_pass.rs:635
    let mut scout = SegmentScout::new(); // trunk_pass.rs:636

    let mut hist_mono: Vec<f32> = Vec::with_capacity(win_samples + CHUNK_FRAMES); // trunk_pass.rs:652
    let mut hist_base: usize = 0; // trunk_pass.rs:654
    let mut next_window_start: usize = 0; // trunk_pass.rs:656

    let mut mfcc_analyzer = MfccAnalyzer::new(); // trunk_pass.rs:659 — ΜΙΑ instance, ΟΛΟ το αρχείο
    let mut mfcc_prev: Option<[f32; 13]> = None; // trunk_pass.rs:660 — ΔΕΝ μηδενίζεται ποτέ
    let mut flux_distances: std::collections::VecDeque<f32> = std::collections::VecDeque::with_capacity(467); // trunk_pass.rs:661
    let mut flux_running_sum: f64 = 0.0; // trunk_pass.rs:662
    let mut next_mfcc_frame_start: usize = 0; // trunk_pass.rs:663

    let mut interleaved = vec![0f32; CHUNK_FRAMES * 2]; // trunk_pass.rs:669
    let mut mono_chunk = vec![0f32; CHUNK_FRAMES]; // trunk_pass.rs:672

    let mut out = Vec::new();

    loop {
        let frames = source.fill_buffer(&mut interleaved).expect("fill_buffer"); // trunk_pass.rs:685
        if frames == 0 {
            break; // trunk_pass.rs:687
        }
        for i in 0..frames {
            mono_chunk[i] = (interleaved[i * 2] + interleaved[i * 2 + 1]) * 0.5; // trunk_pass.rs:691-696
        }
        hist_mono.extend_from_slice(&mono_chunk[..frames]);

        let hist_end = hist_base + hist_mono.len(); // trunk_pass.rs:818

        // --- MFCC distance ring, ΑΥΤΟΥΣΙΟ trunk_pass.rs:820-845 ---
        while next_mfcc_frame_start + 1024 <= hist_end {
            let local_start = next_mfcc_frame_start - hist_base;
            let mfcc_curr = mfcc_analyzer.compute(&hist_mono[local_start..local_start + 1024]);
            if let Some(prev) = mfcc_prev {
                let dist = mfcc_euclidean_distance(&prev, &mfcc_curr);
                flux_distances.push_back(dist);
                flux_running_sum += dist as f64;
                if flux_distances.len() > 466 {
                    let oldest = flux_distances.pop_front().unwrap();
                    flux_running_sum -= oldest as f64;
                }
            }
            mfcc_prev = Some(mfcc_curr);
            next_mfcc_frame_start += 512;
        }

        // --- Serve windows, ΑΥΤΟΥΣΙΟ trunk_pass.rs:847-866 ---
        while next_window_start + win_samples <= hist_end {
            let local_start = next_window_start - hist_base;
            let local_end = local_start + win_samples;
            let start_sec = next_window_start as f32 / sample_rate as f32;
            let cepstral_flux = if flux_distances.is_empty() {
                0.0
            } else {
                (flux_running_sum / flux_distances.len() as f64) as f32
            };
            let meas = scout.measure(&hist_mono[local_start..local_end], cepstral_flux, sample_rate);
            out.push(Window { t: start_sec, cv_ioi: meas.cv_ioi, cepstral_flux: meas.cepstral_flux });
            let _ = compute_scout_decision(&meas); // ΙΔΙΑ κλήση με την παραγωγή, δεν χρειαζόμαστε το αποτέλεσμα εδώ
            next_window_start += hop_samples;
        }

        // --- Drain, ΑΥΤΟΥΣΙΟ trunk_pass.rs:868-883 ---
        let mut keep_from = if next_window_start >= win_samples { next_window_start - win_samples + hop_samples } else { 0 };
        keep_from = keep_from.min(next_mfcc_frame_start);
        if keep_from > hist_base {
            let drain_count = keep_from - hist_base;
            if drain_count > 0 && drain_count <= hist_mono.len() {
                hist_mono.drain(..drain_count);
                hist_base = keep_from;
            }
        }
    }
    out
}

fn read_dump_stereo(path: &str) -> (Vec<f32>, Vec<f32>) {
    const DUMP_FRAME_BYTES: usize = 8;
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read dump {path}: {e}"));
    let full_frames = buf.len() / DUMP_FRAME_BYTES;
    let mut left = Vec::with_capacity(full_frames);
    let mut right = Vec::with_capacity(full_frames);
    for i in 0..full_frames {
        let base = i * DUMP_FRAME_BYTES;
        left.push(f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]));
        right.push(f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]));
    }
    (left, right)
}

fn compare(label: &str, path: &str) {
    let dump = format!("/tmp/io_layer_diag_{}.raw", label);
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump).unwrap_or_else(|e| panic!("decode {path}: {e}"));
    let sample_rate = lineos_types::analysis::ANALYSIS_SAMPLE_RATE;

    let (left, right) = read_dump_stereo(&dump);
    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(&l, &r)| (l + r) * 0.5).collect();
    let a = windows_whole_buffer(&mono, sample_rate);
    let b = windows_streaming(&dump, sample_rate);
    let _ = std::fs::remove_file(&dump);

    println!("=== {label} ===");
    println!("  παράθυρα scan_file(A)={}  streaming(B)={}", a.len(), b.len());

    let n = a.len().min(b.len());
    let mut any_diff = 0usize;
    let mut big_diff_cv = 0usize; // >0.0001
    let mut big_diff_flux = 0usize;
    let mut first_diff: Option<(usize, f32, f32, f32, f32)> = None; // (idx, t, cv_a, cv_b, flux diffs not shown here)
    let mut diff_positions: Vec<usize> = Vec::new();

    for i in 0..n {
        let cv_a = a[i].cv_ioi;
        let cv_b = b[i].cv_ioi;
        let fl_a = a[i].cepstral_flux;
        let fl_b = b[i].cepstral_flux;
        let cv_eq = (cv_a.is_nan() && cv_b.is_nan()) || cv_a == cv_b;
        let fl_eq = fl_a == fl_b;
        if !cv_eq || !fl_eq {
            any_diff += 1;
            diff_positions.push(i);
            if first_diff.is_none() {
                first_diff = Some((i, a[i].t, cv_a, cv_b, 0.0));
            }
        }
        let cv_gap = if cv_a.is_nan() || cv_b.is_nan() { 0.0 } else { (cv_a - cv_b).abs() };
        let fl_gap = (fl_a - fl_b).abs();
        if cv_gap > 0.0001 {
            big_diff_cv += 1;
        }
        if fl_gap > 0.0001 {
            big_diff_flux += 1;
        }
    }

    println!("  συγκρίσιμα παράθυρα (min)={n}");
    println!("  διαφωνούν ΚΑΘΟΛΟΥ (cv_ioi ή cepstral_flux, οποιαδήποτε ψηφιακή διαφορά)={any_diff}/{n}");
    println!("  διαφωνούν >0.0001 στο cv_ioi={big_diff_cv}/{n}   στο cepstral_flux={big_diff_flux}/{n}");
    if let Some((idx, t, cv_a, cv_b, _)) = first_diff {
        println!(
            "  ΠΡΩΤΟ παράθυρο που διαφωνεί: idx={idx} t={t:.2}s  cv_ioi A={:.6} B={:.6}  flux A={:.6} B={:.6}",
            cv_a, cv_b, a[idx].cepstral_flux, b[idx].cepstral_flux
        );
    } else {
        println!("  ΚΑΜΙΑ διαφωνία στα συγκρίσιμα παράθυρα.");
    }
    if !diff_positions.is_empty() {
        let show: Vec<String> = diff_positions.iter().take(20).map(|i| i.to_string()).collect();
        println!(
            "  θέσεις διαφωνιών (πρώτες 20 από {}): [{}]",
            diff_positions.len(),
            show.join(",")
        );
        if diff_positions.len() >= 2 {
            let gaps: Vec<usize> = diff_positions.windows(2).map(|w| w[1] - w[0]).collect();
            let min_gap = *gaps.iter().min().unwrap();
            let max_gap = *gaps.iter().max().unwrap();
            println!("  απόσταση διαδοχικών διαφωνιών: [{min_gap}-{max_gap}]");
        }
    }
    println!();
}

fn main() {
    let dir = "/home/aidevcon/Downloads/DATASET/librivox-hq";
    let files = [
        ("secretgarden", "secretgarden_01_burnett.mp3"),
        ("dracula", "dracula_01_stoker.mp3"),
        ("count_of_monte_cristo", "count_of_monte_cristo_001_dumas.mp3"),
        ("peterpan", "peterpan_01_barrie.mp3"),
        ("anne_of_green_gables", "anne_of_green_gables_01_montgomery.mp3"),
        ("tale_of_two_cities", "tale_of_two_cities_01_dickens.mp3"),
        ("adventurespinocchio", "adventurespinocchio_01_collodi.mp3"),
        ("huckfinn", "huckfinn_01_twain_apc.mp3"),
        ("janeeyre", "janeeyre_01_bronte.mp3"),
    ];
    for (label, fname) in files {
        let path = format!("{dir}/{fname}");
        compare(label, &path);
    }
}
