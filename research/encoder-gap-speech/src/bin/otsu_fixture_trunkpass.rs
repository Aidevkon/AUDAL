//! Βοηθητικό, ΓΙΑ ΑΥΤΟ ΤΟ TASK ΜΟΝΟ: το Otsu στα δώδεκα δοκίμια, μέσω
//! ΤΟΥ ΠΡΑΓΜΑΤΙΚΟΥ trunk_pass::run_trunk_pass — ΟΧΙ scan_file-style
//! αναπαραγωγή (F-111: cepstral_flux διαφέρει σε 98-100% των windows
//! ανάμεσα στα δύο, ήδη μετρημένο στα βιβλία 79→75/128→114). Πριν/μετά
//! το OTSU-TEMP-20260917 patch (ίδιο με το ζεύγος render).
//! ΧΡΗΣΗ: cargo run --release --bin otsu_fixture_trunkpass -- <state:before|after>
use lineos_corpus::scout::{SegmentBoundary, SegmentType};
use std::path::Path;

const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const BOUNDARY_TOLERANCE_SECS: f32 = 5.0;

fn expected_type(schedule: &[(f32, f32, SegmentType)], center: f32) -> SegmentType {
    for &(s, e, ty) in schedule {
        if center >= s && center < e {
            return ty;
        }
    }
    schedule.last().unwrap().2
}

fn expected_boundaries(schedule: &[(f32, f32, SegmentType)]) -> Vec<f32> {
    let mut out = Vec::new();
    for w in schedule.windows(2) {
        if w[0].2 != w[1].2 {
            out.push(w[1].0);
        }
    }
    out
}

fn match_boundaries(expected: &[f32], produced: &[f32], tol: f32) -> (usize, usize, usize) {
    let mut used = vec![false; produced.len()];
    let mut found = 0usize;
    for &e in expected {
        if let Some(idx) = produced
            .iter()
            .enumerate()
            .filter(|(i, &p)| !used[*i] && (p - e).abs() <= tol)
            .min_by(|a, b| (a.1 - e).abs().partial_cmp(&(b.1 - e).abs()).unwrap())
            .map(|(i, _)| i)
        {
            used[idx] = true;
            found += 1;
        }
    }
    let missed = expected.len() - found;
    let extra = used.iter().filter(|&&u| !u).count();
    (found, missed, extra)
}

struct Fixture {
    label: &'static str,
    wav: &'static str,
    schedule: Vec<(f32, f32, SegmentType)>,
}

fn accuracy_for(
    boundaries: &[SegmentBoundary],
    schedule: &[(f32, f32, SegmentType)],
    duration_sec: f32,
) -> (usize, usize) {
    let n_windows = (((duration_sec - WINDOW_SECS) / HOP_SECS).floor() as i64 + 1).max(0) as usize;
    let mut correct = 0usize;
    for w in 0..n_windows {
        let start = w as f32 * HOP_SECS;
        let center = start + WINDOW_SECS / 2.0;
        let exp = expected_type(schedule, center);
        let actual = boundaries
            .iter()
            .find(|b| center >= b.start_sec && center < b.end_sec)
            .map(|b| b.segment_type)
            .unwrap_or(SegmentType::Music);
        if actual == exp {
            correct += 1;
        }
    }
    (correct, n_windows)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let state = args.get(1).cloned().unwrap_or_else(|| "?".to_string());

    use SegmentType::{Music, Speech};
    let fixtures = vec![
        Fixture { label: "1 ΣΚΕΤΗ ΑΦΗΓΗΣΗ", wav: "test1_narration_only.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "2 ΣΚΕΤΟ BED", wav: "test2_bed_only.wav", schedule: vec![(0.0, 120.0, Music)] },
        Fixture { label: "3 ΑΦΗΓΗΣΗ+BED -20dB", wav: "test3_narration_bed_m20dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "4 ΑΦΗΓΗΣΗ+BED -12dB", wav: "test4_narration_bed_m12dB.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "5 ΕΝΑΛΛΑΓΗ 30/60/90", wav: "test5_alternation_30_60_90.wav", schedule: vec![(0.0, 30.0, Speech), (30.0, 60.0, Music), (60.0, 90.0, Speech), (90.0, 120.0, Music)] },
        Fixture { label: "6 BED ΜΠΑΙΝΕΙ ΣΤΑ 60s", wav: "test6_bed_enters_60s.wav", schedule: vec![(0.0, 120.0, Speech)] },
        Fixture { label: "7 ΜΗ-ΣΤΡΟΓΓΥΛΕΣ", wav: "test7_nonround_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
        Fixture { label: "8 ΣΥΝΤΟΜΕΣ 20s×6", wav: "test8_short_20s_x6.wav", schedule: vec![(0.0, 20.0, Speech), (20.0, 40.0, Music), (40.0, 60.0, Speech), (60.0, 80.0, Music), (80.0, 100.0, Speech), (100.0, 120.0, Music)] },
        Fixture { label: "9 ΑΣΥΜΜΕΤΡΕΣ", wav: "test9_asymmetric.wav", schedule: vec![(0.0, 45.0, Speech), (45.0, 53.0, Music), (53.0, 103.0, Speech), (103.0, 111.0, Music), (111.0, 120.0, Speech)] },
        Fixture { label: "10 BED ΣΥΝΕΧΙΖΕΙ", wav: "test10_bed_continues_under.wav", schedule: vec![(0.0, 40.0, Speech), (40.0, 60.0, Music), (60.0, 120.0, Speech)] },
        Fixture { label: "11 ΑΛΛΟ BED", wav: "test11_altbed_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
        Fixture { label: "12 ΑΛΛΟΣ ΑΦΗΓΗΤΗΣ", wav: "test12_altnarrator_17_41_68_94.wav", schedule: vec![(0.0, 17.0, Speech), (17.0, 41.0, Music), (41.0, 68.0, Speech), (68.0, 94.0, Music), (94.0, 120.0, Speech)] },
    ];

    println!("state={state} (via real run_trunk_pass, ΑΝΟΧΗ={BOUNDARY_TOLERANCE_SECS}s)");

    for fx in &fixtures {
        let wav_path = format!("/tmp/scout-groundtruth/{}", fx.wav);
        let dump_path = format!("/tmp/otsu_fixture_{}.raw", fx.wav.trim_end_matches(".wav"));
        let (_metrics, _dec) = m0d::dsp::input_lufs::pass0_decode_to_dump(
            Path::new(&wav_path),
            &dump_path,
        )
        .unwrap_or_else(|e| panic!("decode {wav_path}: {e}"));

        let report = sp314_orchestrator::trunk_pass::run_trunk_pass(Path::new(&dump_path), false)
            .unwrap_or_else(|e| panic!("run_trunk_pass {}: {e}", fx.label));

        let exp_bounds = expected_boundaries(&fx.schedule);
        let prod_bounds: Vec<f32> = report.boundaries.windows(2).map(|w| w[1].start_sec).collect();
        let (found, missed, extra) = match_boundaries(&exp_bounds, &prod_bounds, BOUNDARY_TOLERANCE_SECS);

        let duration_sec = report.boundaries.last().map(|b| b.end_sec).unwrap_or(120.0);
        let (correct, total) = accuracy_for(&report.boundaries, &fx.schedule, duration_sec);

        // Degenerate: πάντα ομιλία σε όλο το αρχείο.
        let n_windows_d = (((duration_sec - WINDOW_SECS) / HOP_SECS).floor() as i64 + 1).max(0) as usize;
        let mut correct_d = 0usize;
        for w in 0..n_windows_d {
            let center = w as f32 * HOP_SECS + WINDOW_SECS / 2.0;
            if expected_type(&fx.schedule, center) == Speech {
                correct_d += 1;
            }
        }

        println!(
            "{:<24} segs={:<3} acc={:>3}/{} ({:>5.1}%)  όρια f={}/{} m={} e={}  Δ_acc={:>3}/{} ({:>5.1}%)",
            fx.label,
            report.boundaries.len(),
            correct, total, 100.0 * correct as f32 / total.max(1) as f32,
            found, exp_bounds.len(), missed, extra,
            correct_d, n_windows_d, 100.0 * correct_d as f32 / n_windows_d.max(1) as f32,
        );

        let _ = std::fs::remove_file(&dump_path);
    }
}
