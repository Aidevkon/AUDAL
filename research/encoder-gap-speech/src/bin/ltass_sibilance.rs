//! ΜΕΤΡΗΣΗ: δημιουργεί το LTASS σιβιλάντ;
//!
//! Χτίζει ΑΚΡΙΒΩΣ τους 8 LTASS κόμβους του streaming_pipeline
//! (BiquadFilter, filter_type 3 = peaking, Q=0.707, κέντρα
//! 50/150/350/750/1500/3000/6000/12000) μέσω του ΠΡΑΓΜΑΤΙΚΟΥ
//! DspTopologyBuilder/DspGraph, παίρνει τα gains από τον ΠΡΑΓΜΑΤΙΚΟ
//! ReferenceResolver, και μετράει ενέργεια 5-9 kHz πριν/μετά.
//!
//! Είσοδος: το post-de-esser στάδιο (stage_4) — ΑΚΡΙΒΩΣ ό,τι βλέπει
//! το LTASS στη ζωντανή αλυσίδα.


const CENTERS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];

fn read_wav(path: &str) -> (Vec<f32>, Vec<f32>, u32) {
    let mut r = hound::WavReader::open(path).expect("open wav");
    let spec = r.spec();
    let ch = spec.channels as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.unwrap() as f32 / max).collect()
        }
    };
    let mut l = Vec::with_capacity(samples.len() / ch);
    let mut rt = Vec::with_capacity(samples.len() / ch);
    for f in samples.chunks(ch) {
        l.push(f[0]);
        rt.push(if ch >= 2 { f[1] } else { f[0] });
    }
    (l, rt, spec.sample_rate)
}

/// Ενέργεια σε ζώνη [lo,hi] Hz, μέσω της ΥΠΑΡΧΟΥΣΑΣ measure_band_energy_hz
/// (spectral.rs:176 — η αδελφή με το ΣΩΣΤΟ bin_hz, F-089).
fn band_db(sig: &[f32], sr: u32, lo: f32, hi: f32) -> f32 {
    // chunked ώστε το FFT να μη γίνει τεράστιο· άθροισμα ενεργειών
    let win = 1 << 15;
    let mut total = 0.0f64;
    let mut n = 0usize;
    for c in sig.chunks(win) {
        if c.len() < win / 2 {
            break;
        }
        total += sp314_dsp::analysis::spectral::measure_band_energy_hz(c, sr, lo, hi) as f64;
        n += 1;
    }
    if n == 0 || total <= 0.0 {
        return f32::NEG_INFINITY;
    }
    10.0 * (total / n as f64).log10() as f32
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: ltass_sibilance <wav>");
    let (l, r, sr) = read_wav(&path);
    println!("MEASURED input: {} frames @ {} Hz", l.len(), sr);

    // ── 1. Τα 8 gains από τον ΠΡΑΓΜΑΤΙΚΟ resolver, μέσω του
    //      ΠΡΑΓΜΑΤΙΚΟΥ trunk pass (ό,τι κάνει ο executor.rs:282).
    //      Το trunk διαβάζει raw interleaved f32 dump — το γράφουμε.
    let dump = std::env::temp_dir().join("ltass_probe_raw.pcm");
    {
        use std::io::Write;
        let f = std::fs::File::create(&dump).expect("create dump");
        let mut w = std::io::BufWriter::new(f);
        for i in 0..l.len() {
            w.write_all(&l[i].to_ne_bytes()).unwrap();
            w.write_all(&r[i].to_ne_bytes()).unwrap();
        }
    }
    let trunk = sp314_orchestrator::trunk_pass::run_trunk_pass(&dump, false)
        .expect("trunk pass");
    let profile_db = trunk.spectral_profile_db;
    let prof = aether_bridge::reference_resolver::ReferenceProfile::load(
        aether_bridge::reference_resolver::ProfileId::PodcastV1,
    );
    let n = prof.normalization_band_count;
    let mean: f32 = profile_db[..n].iter().sum::<f32>() / n as f32;
    let normalized: [f32; 8] = std::array::from_fn(|k| profile_db[k] - mean);
    let gains = aether_bridge::reference_resolver::ReferenceResolver::resolve(&normalized, &prof);

    println!("\n=== ΤΑ 8 GAINS ΠΟΥ ΕΦΑΡΜΟΖΕΙ ΤΟ LTASS ===");
    println!("  band  centre    signal_db   gain_db");
    for k in 0..8 {
        let mark = if k >= 6 { "  ← 5-9 kHz ζώνη" } else { "" };
        println!(
            "   [{}]  {:>7.0}  {:>9.3}  {:>+8.3}{}",
            k, CENTERS[k], normalized[k], gains[k], mark
        );
    }

    // ── 2. Εφαρμογή των 8 biquads μέσω του ΠΡΑΓΜΑΤΙΚΟΥ DspGraph ──
    let mut b = sp314_nodes::topology::DspTopologyBuilder::new("ltass_only");
    let inp = b.add_node("in", "Input", serde_json::json!({}));
    let mut prev = inp;
    let mut ids = Vec::new();
    for k in 0..8 {
        let id = b.add_node(
            &format!("ltass_band_{k}"),
            "BiquadFilter",
            serde_json::json!({ "freq_hz": CENTERS[k], "q": 0.707,
                                "filter_type": 3.0, "gain_db": 0.0 }),
        );
        b.connect(&prev, &id);
        prev = id.clone();
        ids.push(id);
    }
    let out = b.add_node("out", "Output", serde_json::json!({}));
    b.connect(&prev, &out);
    let topo = b.build();
    let mut graph = sp314_nodes::graph::DspGraph::from_topology(&topo, 1024, sr)
        .expect("build ltass graph");
    for (k, id) in ids.iter().enumerate() {
        graph
            .set_node_parameter_no_glide(&id.0, "gain_db", gains[k])
            .expect("set gain");
    }

    let mut ol = l.clone();
    let mut or_ = r.clone();
    for (cl, cr) in ol.chunks_mut(1024).zip(or_.chunks_mut(1024)) {
        graph.process_block(cl, cr);
    }

    // ── 3. Ενέργεια 5-9 kHz πριν/μετά ──
    println!("\n=== ΕΝΕΡΓΕΙΑ ΑΝΑ ΖΩΝΗ, ΠΡΙΝ vs ΜΕΤΑ (mono L) ===");
    println!("  ζώνη           πριν(dB)    μετά(dB)      Δ(dB)");
    for (lo, hi, tag) in [
        (5000.0f32, 9000.0f32, "5-9 kHz  ★"),
        (4000.0, 8000.0, "4-8 kHz     "),
        (8000.0, 12000.0, "8-12 kHz    "),
        (2000.0, 4000.0, "2-4 kHz     "),
        (100.0, 1000.0, "100-1000 Hz "),
    ] {
        let a = band_db(&l, sr, lo, hi);
        let bdb = band_db(&ol, sr, lo, hi);
        println!("  {tag}  {a:>9.4}  {bdb:>10.4}  {:>+9.4}", bdb - a);
    }

    // Γράψε το αποτέλεσμα για ενδεχόμενη ακρόαση
    let outp = std::env::args().nth(2);
    if let Some(op) = outp {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&op, spec).expect("create out");
        for i in 0..ol.len() {
            w.write_sample(ol[i]).unwrap();
            w.write_sample(or_[i]).unwrap();
        }
        w.finalize().unwrap();
        println!("\nWROTE {op}");
    }
    println!("DONE");
}
