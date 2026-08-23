//! ΜΕΤΡΗΣΗ 2026-08-24 — «κάνουν κάτι οι κόμβοι restoration;»
//! READ+RUN, standalone research binary. ΔΕΝ αγγίζει production code —
//! καλεί τα ΠΡΑΓΜΑΤΙΚΑ DspNode impls (sp314-nodes) με τις ΠΡΑΓΜΑΤΙΚΕΣ
//! παραμέτρους από τα production call sites, όχι re-implementation.
//!
//! Νόμος 3: atomic single-script, sha256 + provenance ανά render.
//!
//! Production params πηγή:
//!  - DeEsser: sp314-orchestrator/src/streaming_pipeline.rs:114-118
//!    (threshold_db 0.0, frequency_hz 6000.0 — αυτό που έδειξε το recon
//!    ως ύποπτο, ΟΧΙ το POXVoice -24.0)
//!  - DeHum, NoiseGate: το streaming_pipeline.rs ΔΕΝ τα δηλώνει καθόλου
//!    (μόνο DeEsser+BiquadFilter+Gain στο vocal topology). Η μόνη
//!    πραγματική παραγωγική καλωδίωση αυτών των δύο node types είναι το
//!    POXVoice topology (pipelines/pipelineforge/src/flavor.rs:159-177),
//!    που είναι ΑΚΡΙΒΩΣ η υποψήφια αλυσίδα για wiring στο ACX (το θέμα
//!    της εντολής). Χρησιμοποιώ ΑΥΤΑ αντί για streaming_pipeline.rs —
//!    ρητή απόκλιση, δηλωμένη εδώ ΚΑΙ στην αναφορά, όχι σιωπηλή.
//!  - Bonus: sp314_dsp::restoration::gate::NoiseGate (render_node.rs:230)
//!    είναι ΑΛΛΗ υλοποίηση gate, διαφορετική από sp314-nodes::NoiseGateNode
//!    (POXVoice). Μετρήθηκε ξεχωριστά.

use sha2::{Digest, Sha256};
use sp314_nodes::node::DspNode;
use sp314_nodes::nodes::deesser::DeEsserNode;
use sp314_nodes::nodes::dehum::DeHumNode;
use sp314_nodes::nodes::noisegate::NoiseGateNode;
use sp314_dsp::restoration::gate::NoiseGate as CoreGate;
use std::path::PathBuf;

fn decode_stereo_native(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let probe = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=sample_rate", "-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(path)
        .output()
        .expect("spawn ffprobe");
    let sr: u32 = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .next()
        .expect("no sr")
        .trim()
        .parse()
        .expect("bad sr");

    let tmp = PathBuf::from(format!("/tmp/restoration_alive_{}.pcm", std::process::id()));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-ac", "2", "-ar", &sr.to_string(), "-f", "f32le"])
        .arg(&tmp)
        .status()
        .expect("spawn ffmpeg");
    assert!(status.success(), "ffmpeg decode failed");
    let bytes = std::fs::read(&tmp).expect("read pcm");
    let _ = std::fs::remove_file(&tmp);
    let interleaved: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let mut l = Vec::with_capacity(interleaved.len() / 2);
    let mut r = Vec::with_capacity(interleaved.len() / 2);
    for pair in interleaved.chunks_exact(2) {
        l.push(pair[0]);
        r.push(pair[1]);
    }
    (l, r, sr)
}

fn sha256_stereo(l: &[f32], r: &[f32]) -> String {
    let mut h = Sha256::new();
    for &s in l {
        h.update(s.to_le_bytes());
    }
    for &s in r {
        h.update(s.to_le_bytes());
    }
    format!("{:x}", h.finalize())
}

fn rms_db(v: &[f32]) -> f64 {
    if v.is_empty() {
        return f64::NEG_INFINITY;
    }
    let sum_sq: f64 = v.iter().map(|&x| (x as f64) * (x as f64)).sum();
    let rms = (sum_sq / v.len() as f64).sqrt();
    if rms < 1e-12 {
        -240.0
    } else {
        20.0 * rms.log10()
    }
}

struct NullResult {
    label: String,
    sha_in: String,
    sha_out: String,
    identical: bool,
    in_rms_db: f64,
    out_rms_db: f64,
    null_rms_db: f64,
    max_abs_delta: f32,
}

fn run_case<F: FnOnce(&mut [f32], &mut [f32])>(
    label: &str,
    l0: &[f32],
    r0: &[f32],
    process: F,
) -> NullResult {
    let mut l = l0.to_vec();
    let mut r = r0.to_vec();
    let sha_in = sha256_stereo(&l, &r);
    let in_rms_db = (rms_db(&l) + rms_db(&r)) / 2.0;

    process(&mut l, &mut r);

    let sha_out = sha256_stereo(&l, &r);
    let out_rms_db = (rms_db(&l) + rms_db(&r)) / 2.0;

    let mut diff = Vec::with_capacity(l.len() * 2);
    let mut max_abs_delta = 0.0f32;
    for i in 0..l.len() {
        let dl = l0[i] - l[i];
        let dr = r0[i] - r[i];
        max_abs_delta = max_abs_delta.max(dl.abs()).max(dr.abs());
        diff.push(dl);
        diff.push(dr);
    }
    let null_rms_db = rms_db(&diff);
    let identical = sha_in == sha_out;

    NullResult {
        label: label.to_string(),
        sha_in,
        sha_out,
        identical,
        in_rms_db,
        out_rms_db,
        null_rms_db,
        max_abs_delta,
    }
}

fn print_result(r: &NullResult) {
    println!("--- {} ---", r.label);
    println!("  sha256 in  = {}", r.sha_in);
    println!("  sha256 out = {}", r.sha_out);
    println!(
        "  sha ταυτόσημα = {}  ({})",
        r.identical,
        if r.identical { "ΑΔΡΑΝΗΣ κατά byte" } else { "ΔΙΑΦΕΡΕΙ" }
    );
    println!("  in_rms_db  = {:.4}", r.in_rms_db);
    println!("  out_rms_db = {:.4}", r.out_rms_db);
    println!("  null (in-out) rms_db = {:.4}", r.null_rms_db);
    println!("  max |Δsample| = {:.8} ({:.2} dBFS)", r.max_abs_delta,
        if r.max_abs_delta > 0.0 { 20.0 * (r.max_abs_delta as f64).log10() } else { f64::NEG_INFINITY });
    println!();
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    assert!(!paths.is_empty(), "usage: restoration_alive <file1> [file2] ...");

    for path in &paths {
        println!("══════════════════════════════════════════════════════");
        println!("input: {path}");
        let (l0, r0, sr) = decode_stereo_native(path);
        println!("sr={sr} samples={} dur={:.2}s", l0.len(), l0.len() as f64 / sr as f64);
        println!();

        // ── 1. DeEsser ──────────────────────────────────────────────
        // production: streaming_pipeline.rs:114-118 (threshold_db 0.0,
        // frequency_hz 6000.0 — ratio ΔΕΝ γίνεται override, μένει το
        // constructor default 4.0)
        let prod = run_case("DeEsser @ PRODUCTION (streaming_pipeline.rs: thr=0.0, freq=6000.0, ratio=4.0[default])", &l0, &r0, |l, r| {
            let mut n = DeEsserNode::new(sr);
            n.set_parameter("threshold_db", 0.0);
            n.set_parameter("frequency_hz", 6000.0);
            n.process_stereo(l, r);
        });
        print_result(&prod);

        // default: DeEsserNode::new() ολόκληρο (thr -24.0, freq 6000.0, ratio 4.0)
        let def = run_case("DeEsser @ DEFAULT (DeEsserNode::new(): thr=-24.0, freq=6000.0, ratio=4.0)", &l0, &r0, |l, r| {
            let mut n = DeEsserNode::new(sr);
            n.process_stereo(l, r);
        });
        print_result(&def);

        // ── 2. DeHum ─────────────────────────────────────────────────
        // streaming_pipeline.rs ΔΕΝ δηλώνει DeHum καθόλου. Μόνη
        // πραγματική καλωδίωση: POXVoice (flavor.rs:164) — enabled
        // true, fundamental_hz 50.0. Δεν κάνει override "harmonics" —
        // μένει το constructor default 3.0. ΑΥΤΟ ΕΙΝΑΙ ΤΑΥΤΟΣΗΜΟ με τα
        // defaults του DeHumNode::new() (enabled true, fundamental 50.0,
        // harmonics 3.0) — δεν υπάρχει διαφορά production/default εδώ,
        // αναφέρεται ρητά, όχι κατασκευασμένη.
        let dehum_prod = run_case("DeHum @ PRODUCTION (POXVoice/flavor.rs:164: enabled=true, fundamental_hz=50.0, harmonics=3.0[default, όχι override])", &l0, &r0, |l, r| {
            let mut n = DeHumNode::new(sr);
            n.set_parameter("enabled", 1.0);
            n.set_parameter("fundamental_hz", 50.0);
            n.process_stereo(l, r);
        });
        print_result(&dehum_prod);
        println!("  (ΣΗΜΕΙΩΣΗ: DeHum production == DeHumNode::new() defaults ΑΚΡΙΒΩΣ — καμία δεύτερη μέτρηση δεν προσθέτει πληροφορία, δεν έγινε.)\n");

        // ── 3. NoiseGate (sp314-nodes::NoiseGateNode, μέσω POXVoice) ──
        // production: flavor.rs:163 thr=-40.0 attack=1.0 hold=50.0 release=150.0
        let gate_prod = run_case("NoiseGateNode @ PRODUCTION (POXVoice/flavor.rs:163: thr=-40.0, attack=1.0ms, hold=50.0ms, release=150.0ms)", &l0, &r0, |l, r| {
            let mut n = NoiseGateNode::new(sr);
            n.set_params(-40.0, 1.0, 50.0, 150.0);
            n.process_stereo(l, r);
        });
        print_result(&gate_prod);

        // default: NoiseGateNode::new() thr=-60.0 attack=5.0 hold=50.0 release=100.0
        let gate_def = run_case("NoiseGateNode @ DEFAULT (NoiseGateNode::new(): thr=-60.0, attack=5.0ms, hold=50.0ms, release=100.0ms)", &l0, &r0, |l, r| {
            let mut n = NoiseGateNode::new(sr);
            n.process_stereo(l, r);
        });
        print_result(&gate_def);

        // ── BONUS: sp314_dsp::restoration::gate::NoiseGate (ΑΛΛΗ υλοποίηση,
        // render_node.rs:230, gated by restoration_enabled) ────────────
        let core_gate = run_case("CoreGate (sp314-dsp::restoration::gate::NoiseGate) @ render_node.rs:230 params (pad=0.0, threshold=-45.0 — ΤΟ ΙΔΙΟ με το doc-commented default του, δεν υπάρχει ξεχωριστός constructor default)", &l0, &r0, |l, r| {
            let mut g = CoreGate::new(sr as f32, 0.0, -45.0);
            for i in 0..l.len() {
                let (gl, gr) = g.process_stereo(l[i], r[i]);
                l[i] = gl;
                r[i] = gr;
            }
        });
        print_result(&core_gate);

        println!();
    }
}
