//! Βοηθητικό, ΓΙΑ ΑΥΤΟ ΤΟ TASK ΜΟΝΟ: σύγκριση πριν/μετά το
//! OTSU-TEMP-20260917 patch — boundaries που εξαφανίστηκαν/εμφανίστηκαν
//! (±5s ανοχή, ίδιο ταίριασμα με attack_dynamic_k_battery.rs), RMS
//! 200ms εκατέρωθεν, φάσμα οκτώ μπαντών (pre_analysis.rs
//! spectral_profile_levels, ΠΡΑΓΜΑΤΙΚΟ), και μέγιστη στιγμιαία διαφορά
//! δείγματος ανάμεσα στα δύο renders γύρω από κάθε boundary. ΔΕΝ
//! κρίνει αν ακούγεται — μόνο μετράει.
//! ΧΡΗΣΗ: cargo run --release --bin otsu_boundary_diff -- <label>
use sp314_dsp::analysis::pre_analysis::spectral_profile_levels;

const SR: u32 = 48_000;

fn read_boundaries(path: &str) -> Vec<(f32, f32, String)> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let parts: Vec<&str> = l.split('\t').collect();
            (
                parts[0].parse::<f32>().unwrap(),
                parts[1].parse::<f32>().unwrap(),
                parts[2].to_string(),
            )
        })
        .collect()
}

fn boundary_times(segs: &[(f32, f32, String)]) -> Vec<f32> {
    segs.windows(2).map(|w| w[1].0).collect()
}

/// ΙΔΙΟ ταίριασμα ±tol με attack_dynamic_k_battery.rs::match_boundaries,
/// αλλά επιστρέφει ΠΟΙΑ ΧΑΘΗΚΑΝ/ΝΕΑ, όχι μόνο μέτρημα.
fn diff_boundaries(before: &[f32], after: &[f32], tol: f32) -> (Vec<f32>, Vec<f32>) {
    let mut used_after = vec![false; after.len()];
    let mut vanished = Vec::new();
    for &b in before {
        let m = after
            .iter()
            .enumerate()
            .filter(|(i, &a)| !used_after[*i] && (a - b).abs() <= tol)
            .min_by(|x, y| (x.1 - b).abs().partial_cmp(&(y.1 - b).abs()).unwrap())
            .map(|(i, _)| i);
        match m {
            Some(i) => used_after[i] = true,
            None => vanished.push(b),
        }
    }
    let new: Vec<f32> = after
        .iter()
        .enumerate()
        .filter(|(i, _)| !used_after[*i])
        .map(|(_, &a)| a)
        .collect();
    (vanished, new)
}

fn read_pcm_stereo(path: &str) -> (Vec<f32>, Vec<f32>) {
    let buf = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let n = buf.len() / 8;
    let mut l = Vec::with_capacity(n);
    let mut r = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * 8;
        l.push(f32::from_le_bytes([buf[base], buf[base + 1], buf[base + 2], buf[base + 3]]));
        r.push(f32::from_le_bytes([buf[base + 4], buf[base + 5], buf[base + 6], buf[base + 7]]));
    }
    (l, r)
}

fn rms_db(l: &[f32], r: &[f32]) -> f32 {
    if l.is_empty() {
        return -144.0;
    }
    let sum_sq: f64 = l.iter().chain(r.iter()).map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (sum_sq / (2.0 * l.len() as f64)).sqrt();
    if rms > 1e-20 {
        (20.0 * rms.log10()) as f32
    } else {
        -144.0
    }
}

fn slice_at(l: &[f32], r: &[f32], center_sec: f32, half_span_sec: f32) -> Option<(usize, usize)> {
    let center = (center_sec * SR as f32) as i64;
    let half = (half_span_sec * SR as f32) as i64;
    let start = (center - half).max(0) as usize;
    let end = ((center + half) as usize).min(l.len()).min(r.len());
    if start >= end {
        None
    } else {
        Some((start, end))
    }
}

fn max_sample_diff(l1: &[f32], r1: &[f32], l2: &[f32], r2: &[f32], start: usize, end: usize) -> f32 {
    let e = end.min(l1.len()).min(l2.len());
    let mut m = 0.0_f32;
    for i in start..e {
        m = m.max((l1[i] - l2[i]).abs()).max((r1[i] - r2[i]).abs());
    }
    m
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let label = args[1].clone();

    let before_b = read_boundaries(&format!("/tmp/otsu_pair_{label}_before_boundaries.txt"));
    let after_b = read_boundaries(&format!("/tmp/otsu_pair_{label}_after_boundaries.txt"));
    let bt_before = boundary_times(&before_b);
    let bt_after = boundary_times(&after_b);

    println!(
        "label={label} segments_before={} segments_after={} boundary_points_before={} boundary_points_after={}",
        before_b.len(), after_b.len(), bt_before.len(), bt_after.len()
    );

    let (vanished, new) = diff_boundaries(&bt_before, &bt_after, 5.0);
    println!("vanished={} new={}", vanished.len(), new.len());

    let (l_before, r_before) = read_pcm_stereo(&format!("/tmp/otsu_pair_{label}_before.pcm"));
    let (l_after, r_after) = read_pcm_stereo(&format!("/tmp/otsu_pair_{label}_after.pcm"));
    println!(
        "frames_before={} frames_after={} (ίδιο μήκος => ίδιο index-alignment)",
        l_before.len(), l_after.len()
    );

    let mut best: Option<(f32, f32)> = None; // (time, max_diff)

    let mut report = |kind: &str, t: f32| {
        let pre_before = slice_at(&l_before, &r_before, t - 0.1, 0.1);
        let post_before = slice_at(&l_before, &r_before, t + 0.1, 0.1);
        let pre_after = slice_at(&l_after, &r_after, t - 0.1, 0.1);
        let post_after = slice_at(&l_after, &r_after, t + 0.1, 0.1);

        let rms = |sl: Option<(usize, usize)>, l: &[f32], r: &[f32]| -> f32 {
            sl.map(|(s, e)| rms_db(&l[s..e], &r[s..e])).unwrap_or(-144.0)
        };

        let rms_pre_before = rms(pre_before, &l_before, &r_before);
        let rms_post_before = rms(post_before, &l_before, &r_before);
        let rms_pre_after = rms(pre_after, &l_after, &r_after);
        let rms_post_after = rms(post_after, &l_after, &r_after);

        let spec_before = slice_at(&l_before, &r_before, t, 0.1)
            .map(|(s, e)| spectral_profile_levels(&l_before[s..e], &r_before[s..e], SR))
            .unwrap_or([-144.0; 8]);
        let spec_after = slice_at(&l_after, &r_after, t, 0.1)
            .map(|(s, e)| spectral_profile_levels(&l_after[s..e], &r_after[s..e], SR))
            .unwrap_or([-144.0; 8]);
        let spec_delta: Vec<f32> = spec_before.iter().zip(spec_after.iter()).map(|(a, b)| b - a).collect();

        let (ds, de) = slice_at(&l_before, &r_before, t, 0.5).unwrap_or((0, 0));
        let maxdiff = max_sample_diff(&l_before, &r_before, &l_after, &r_after, ds, de);

        println!(
            "{kind} t={t:.3}s  RMS200ms(before) pre={rms_pre_before:.2}dB post={rms_post_before:.2}dB  \
             RMS200ms(after) pre={rms_pre_after:.2}dB post={rms_post_after:.2}dB  \
             8band_delta(after-before)_dB={spec_delta:?}  max_sample_diff(±0.5s)={maxdiff:.6}"
        );

        if best.is_none() || maxdiff > best.unwrap().1 {
            best = Some((t, maxdiff));
        }
    };

    for &t in &vanished {
        report("VANISHED", t);
    }
    for &t in &new {
        report("NEW", t);
    }

    if let Some((t, maxdiff)) = best {
        println!("ΕΠΙΛΕΓΜΕΝΟ_ΓΙΑ_ΑΠΟΣΠΑΣΜΑ t={t:.3}s max_sample_diff={maxdiff:.6} (μεγαλύτερη διαφορά μεταξύ όλων των αλλαγμένων boundaries)");
    } else {
        println!("ΚΑΝΕΝΑ ΑΛΛΑΓΜΕΝΟ BOUNDARY — ο φρουρός κρατάει");
    }
}
