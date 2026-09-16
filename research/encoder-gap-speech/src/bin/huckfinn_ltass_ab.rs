//! ΜΕΤΡΗΣΗ 2026-09-16: ζεύγος ακρόασης Α/Β για το huckfinn_01_twain_apc,
//! ΠΛΗΡΗΣ αλυσίδα (vocal_graph: de-esser → LTASS ×8 → gain), πραγματικό
//! pre_analysis από run_trunk_pass πάνω στο ΙΔΙΟ απόσπασμα που ακούγεται.
//!
//! Α = χωρίς τον πίνακα — τα gains του resolver απευθείας, όπως έτρεχε
//!     πριν το a4e1b82.
//! Β = με τον πίνακα — ο σημερινός κώδικας, ανέγγιχτος.
//!
//! Και τα δύο ΜΕ το ταβάνι (G_MAX_DB=6.0), όπως τρέχει σήμερα — δεν
//! συγκρίνουμε την άλγεβρα, συγκρίνουμε το πριν/μετά του σημερινού
//! βήματος.
//!
//! ΧΡΗΣΗ:
//!   cargo run --release --bin huckfinn_ltass_ab -- pick        (βρίσκει το παράθυρο)
//!   cargo run --release --bin huckfinn_ltass_ab -- render A|B  (γράφει το αρχείο εξόδου)
//!   cargo run --release --bin huckfinn_ltass_ab -- gains       (τυπώνει τον πίνακα οκτώ gains Α/Β)
//!   cargo run --release --bin huckfinn_ltass_ab -- energy <wav> (5-8kHz ενέργεια, dB)

use lineos_corpus::scout::{SegmentBoundary, SegmentType};
use sp314_dsp::masking_eq::biquad::{rbj_highpass, rbj_lowpass, BiquadCoeffs};

const SOURCE_MP3: &str = "/home/aidevcon/Downloads/DATASET/librivox-hq/huckfinn_01_twain_apc.mp3";
const CLIP_PATH: &str = "/tmp/huckfinn-ltass-audition/source_clip.wav";
const CLIP_SECS: f64 = 15.0;
const OUT_DIR: &str = "/tmp/huckfinn-ltass-audition";

struct Biquad {
    c: BiquadCoeffs,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}
impl Biquad {
    fn new(c: BiquadCoeffs) -> Self {
        Self { c, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.c.b0 * x + self.c.b1 * self.x1 + self.c.b2 * self.x2
            - self.c.a1 * self.y1
            - self.c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// 5000-8000 Hz bandpass (HP 5000 → LP 8000, δεύτερης τάξης RBJ, Q=0.707
/// έκαστο) → RMS στο αποτέλεσμα, dB. ΙΔΙΟ όργανο για επιλογή παραθύρου ΚΑΙ
/// τελική αναφορά — συγκρίσιμοι αριθμοί.
fn band_energy_5_8k_db(mono: &[f32], sr: u32) -> f64 {
    let mut hp = Biquad::new(rbj_highpass(5000.0, 0.707, sr as f64));
    let mut lp = Biquad::new(rbj_lowpass(8000.0, 0.707, sr as f64));
    let mut sum_sq = 0.0_f64;
    for &s in mono {
        let y = lp.process(hp.process(s as f64));
        sum_sq += y * y;
    }
    let rms = (sum_sq / mono.len().max(1) as f64).sqrt();
    20.0 * rms.max(1e-12).log10()
}

fn read_mono_dump(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).expect("read dump");
    let n = bytes.len() / 4;
    let mut inter = Vec::with_capacity(n);
    for i in 0..n {
        inter.push(f32::from_le_bytes([
            bytes[i * 4],
            bytes[i * 4 + 1],
            bytes[i * 4 + 2],
            bytes[i * 4 + 3],
        ]));
    }
    inter.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect()
}

/// Σαρώνει ΟΛΟ το βιβλίο σε παράθυρα 15s / hop 5s, μετράει 5-8kHz ενέργεια
/// στο ΚΑΘΕ ένα, επιστρέφει τη θέση (δευτερόλεπτα) του δυνατότερου —
/// «πολλά σίγματα» = μέγιστη ενέργεια εκεί.
fn pick_window() {
    let dump = "/tmp/huckfinn_pick_full.raw";
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(SOURCE_MP3), dump)
        .expect("pass0 decode");
    let mono = read_mono_dump(dump);
    let _ = std::fs::remove_file(dump);

    let sr = 48_000usize;
    let win = (CLIP_SECS * sr as f64) as usize;
    let hop = 5 * sr;
    let mut best_start = 0usize;
    let mut best_db = f64::NEG_INFINITY;

    let mut pos = 0usize;
    while pos + win <= mono.len() {
        let db = band_energy_5_8k_db(&mono[pos..pos + win], sr as u32);
        if db > best_db {
            best_db = db;
            best_start = pos;
        }
        pos += hop;
    }
    let start_sec = best_start as f64 / sr as f64;
    println!(
        "ΚΑΛΥΤΕΡΟ ΠΑΡΑΘΥΡΟ (μέγιστη 5-8kHz ενέργεια, {:.0}s βήμα σάρωσης 5s): {:.1}s, {:.2} dB",
        CLIP_SECS, start_sec, best_db
    );
    println!("ffmpeg -ss {start_sec:.2} -t {CLIP_SECS}");
}

fn extract_clip(start_sec: f64) {
    std::fs::create_dir_all(OUT_DIR).ok();
    let status = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{start_sec:.3}"),
            "-i",
            SOURCE_MP3,
            "-t",
            &format!("{CLIP_SECS}"),
            "-ar",
            "48000",
            "-ac",
            "2",
            "-acodec",
            "pcm_s16le",
            CLIP_PATH,
        ])
        .status()
        .expect("ffmpeg spawn");
    assert!(status.success(), "ffmpeg extract failed");
    println!("Απόσπασμα γράφτηκε: {CLIP_PATH} ({start_sec:.2}s, {CLIP_SECS}s)");
}

fn compute_trunk_report(
    path: &str,
) -> (
    sp314_orchestrator::trunk_pass::TrunkReport,
    lineos_types::pre_analysis::PreAnalysisData,
) {
    let raw = format!("{path}.raw");
    let (metrics, _p0) = m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &raw)
        .expect("pass0 decode");
    let trunk_report =
        sp314_orchestrator::trunk_pass::run_trunk_pass(std::path::Path::new(&raw), false)
            .expect("run_trunk_pass");
    let pre_analysis = trunk_report.to_pre_analysis(
        metrics.true_peak_dbtp,
        metrics.bpm,
        metrics.beats_ms.clone(),
        metrics.downbeats_ms.clone(),
        metrics.transients_ms.clone(),
    );
    let _ = std::fs::remove_file(&raw);
    (trunk_report, pre_analysis)
}

/// Τυπώνει τα οκτώ gains Α (χωρίς πίνακα) και Β (με πίνακα) πλάι-πλάι,
/// υπολογισμένα ΚΑΘΑΡΑ (χωρίς render) πάνω στο πραγματικό spectral profile
/// του αποσπάσματος — ίδια λογική με apply_ltass_band_compensation, χωρίς
/// να αγγίζει τη συνάρτηση (ιδιωτική, άλλο crate).
fn print_gains_table() {
    let (trunk_report, _pre) = compute_trunk_report(CLIP_PATH);

    let profile = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );
    let n = profile.normalization_band_count;
    let raw_profile = trunk_report.metrics.spectral_profile_db;
    let speech_mean: f32 = raw_profile[..n].iter().sum::<f32>() / n as f32;
    let normalized: [f32; 8] = std::array::from_fn(|k| raw_profile[k] - speech_mean);

    const G_MAX_DB: f32 = 6.0;
    let raw: [f32; 8] = std::array::from_fn(|k| {
        let r = profile.spectral_target[k] - normalized[k];
        if r.abs() <= profile.dead_zone_db[k] {
            0.0
        } else {
            r
        }
    });

    #[rustfmt::skip]
    const B_INV_Q0707: [[f32; 8]; 8] = [
        [ 1.3717, -0.5135,  0.2316, -0.0911,  0.0310, -0.0102,  0.0030, -0.0008],
        [-0.4907,  1.7330, -1.0605,  0.4278, -0.1457,  0.0482, -0.0141,  0.0038],
        [ 0.1502, -0.7726,  2.0203, -1.1928,  0.4248, -0.1406,  0.0413, -0.0110],
        [-0.0507,  0.2677, -1.1396,  2.2964, -1.2724,  0.4424, -0.1301,  0.0347],
        [ 0.0199, -0.1049,  0.4686, -1.3901,  2.3899, -1.2671,  0.3908, -0.1044],
        [-0.0073,  0.0388, -0.1739,  0.5396, -1.3760,  2.2805, -1.0778,  0.3007],
        [ 0.0024, -0.0129,  0.0579, -0.1802,  0.4809, -1.1825,  1.9069, -0.8234],
        [-0.0006,  0.0032, -0.0142,  0.0443, -0.1185,  0.3047, -0.7204,  1.7053],
    ];

    let a_final: [f32; 8] = std::array::from_fn(|k| raw[k].clamp(-G_MAX_DB, G_MAX_DB));
    let b_final: [f32; 8] = std::array::from_fn(|i| {
        let post: f32 = B_INV_Q0707[i].iter().zip(raw.iter()).map(|(&b, &r)| b * r).sum();
        post.clamp(-G_MAX_DB, G_MAX_DB)
    });

    const CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
    println!("ΤΑ ΟΚΤΩ ΤΕΛΙΚΑ gains, ΑΠΟΣΠΑΣΜΑ (raw resolver για ΑΥΤΟ το κομμάτι, όχι το βιβλίο):");
    println!("{:<10}{:>10}{:>10}{:>10}", "band", "hz", "Α", "Β");
    for k in 0..8 {
        println!("{:<10}{:>10.1}{:>10.3}{:>10.3}", format!("band {k}"), CFS[k], a_final[k], b_final[k]);
    }
}

fn render(output_path: &str) {
    use m0d::dsp::file_decoder::FileDecoder;
    use sp314_orchestrator::streaming_pipeline::{
        run_streaming_pipeline_with_timeline, StreamingConfig, TimelinePlan,
    };

    let (_trunk_report, pre_analysis) = compute_trunk_report(CLIP_PATH);

    let topology_json = serde_json::json!({
        "topology_id": "dummy_ducking_topology",
        "nodes": [
            { "node_id": "in", "node_type": "Input", "parameters": {} },
            { "node_id": "duck_gain", "node_type": "Gain", "parameters": { "gain": 1.0, "glide_ms": 300.0 } },
            { "node_id": "out", "node_type": "Output", "parameters": {} }
        ],
        "edges": [
            { "source": "in", "target": "duck_gain", "modulation_type": "audio" },
            { "source": "duck_gain", "target": "out", "modulation_type": "audio" }
        ]
    });
    let topology = sp314_nodes::topology::DspTopology::from_json(&topology_json.to_string()).unwrap();

    let boundaries = vec![SegmentBoundary {
        start_sec: 0.0,
        end_sec: CLIP_SECS as f32,
        segment_type: SegmentType::Speech,
        avg_leaning: 0.5,
        // Χαμηλή εμπιστοσύνη ⇒ Hybrid ⇒ τρέχει το vocal_graph (de-esser →
        // LTASS ×8 → gain) — ΙΔΙΟ μοτίβο με streaming_integration.rs's
        // test_vocal_graph_e2e_ltass_proof.
        avg_confidence: 0.2,
    }];

    let (tx_job, rx_job) = std::sync::mpsc::channel();
    let (tx_res, rx_res) = std::sync::mpsc::channel();
    let shadow_reader =
        m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(CLIP_PATH)).unwrap();
    let _worker_handle = m0d::dsp::orchestrator::nmf_worker::spawn(shadow_reader, 48000, rx_job, tx_res);
    let (_, flagged_indices) = m0d::dsp::orchestrator::nmf_worker::dispatch_all_jobs(&boundaries, &tx_job);

    run_streaming_pipeline_with_timeline(
        FileDecoder { path: CLIP_PATH.to_string() },
        output_path,
        &StreamingConfig {
            topology: &topology,
            block_size: 1024,
            sample_rate: 48000,
            ducking_node_id: "duck_gain",
            speech_gain: 1.0,
            music_gain: 0.501,
            pre_gain_linear: 1.0,
            expected_output_frames: None,
            quietest_active_window_dbfs: None,
            max_true_peak_db: lineos_types::presets::PODCAST.max_true_peak_db,
            max_noise_floor_db: lineos_types::presets::PODCAST.max_noise_floor_db,
            input_interior_floor_db: None,
            quiet_window_split_dbfs: None,
            input_fundamental: None,
            intent_dynamics: None,
            restoration_enabled: false,
        },
        TimelinePlan {
            boundaries,
            flagged_indices,
            pre_analysis: Some(&pre_analysis),
        },
        rx_res,
    )
    .expect("render failed");
    println!("Γράφτηκε: {output_path}");
}

fn energy_of(path: &str) {
    let dump = format!("{path}.energy.raw");
    m0d::dsp::input_lufs::pass0_decode_to_dump(std::path::Path::new(path), &dump)
        .expect("pass0 decode");
    let mono = read_mono_dump(&dump);
    let _ = std::fs::remove_file(&dump);
    let db = band_energy_5_8k_db(&mono, 48_000);
    println!("5-8kHz ενέργεια {path}: {db:.2} dB");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("pick") => pick_window(),
        Some("extract") => {
            let start: f64 = args.get(2).and_then(|s| s.parse().ok()).expect("start_sec arg");
            extract_clip(start);
        }
        Some("gains") => print_gains_table(),
        Some("render") => {
            let label = args.get(2).map(|s| s.as_str()).unwrap_or("B");
            let out = format!("{OUT_DIR}/{label}.wav");
            render(&out);
        }
        Some("energy") => {
            let path = args.get(2).expect("wav path arg");
            energy_of(path);
        }
        _ => eprintln!("usage: huckfinn_ltass_ab <pick|extract <start_sec>|gains|render <A|B>|energy <wav>>"),
    }
}
