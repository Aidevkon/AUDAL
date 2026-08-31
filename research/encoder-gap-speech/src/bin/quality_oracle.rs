//! F-085 wiring ORACLE: γνωστές σωστές απαντήσεις, όχι μεταβλητότητα.
//!
//! ΜΕΤΑΒΛΗΤΟΤΗΤΑ ≠ ΑΛΗΘΕΙΑ. Μετρήσεις της ΕΙΣΟΔΟΥ θα μετέβαλλαν κι
//! αυτές ανά αρχείο — άρα το «τα τρία διαφέρουν» δεν αποδεικνύει
//! τίποτα. Εδώ φτιάχνουμε σήματα με ΓΝΩΣΤΗ απάντηση και ελέγχουμε τι
//! γράφει το cert.
//!
//! Τρέχει τη ΖΩΝΤΑΝΗ διαδρομή (execute_streaming_plan) — ό,τι ακριβώς
//! spawn-άρει το POST /master/streaming.

use m0d::blob_store::BlobVariant;

fn write_wav(path: &str, left: &[f32], right: &[f32], sr: u32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: sr,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w = hound::WavWriter::create(path, spec).expect("create wav");
    for i in 0..left.len() {
        w.write_sample(left[i]).unwrap();
        w.write_sample(right[i]).unwrap();
    }
    w.finalize().unwrap();
}

fn run_live(path: &str, tag: &str) -> Option<(f32, f32, f32, f32, f32, f32, f32)> {
    let abs = std::fs::canonicalize(path).expect("resolve");
    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: format!("oracle-{tag}"),
    };
    match m0d::agents::executor::execute_streaming_plan(&plan, None) {
        Ok((_out, blob)) => {
            if let BlobVariant::Certified { quality, loudness, .. } = &blob.variant {
                Some((
                    quality.stereo_correlation,
                    quality.dynamic_range_db,
                    quality.rms_db,
                    loudness.integrated_lufs,
                    quality.stereo_width,
                    quality.spectral_centroid,
                    quality.spectral_flatness,
                ))
            } else {
                None
            }
        }
        Err(e) => {
            println!("  {tag}: FAILED {e:?}");
            None
        }
    }
}

fn main() {
    let sr = 48_000u32;
    let n = sr as usize * 5; // 5 s
    let dir = std::env::temp_dir();

    // ── ORACLE 1: correlation με γνωστή απάντηση ──
    let amp = libm::powf(10.0, -20.0 / 20.0); // −20 dBFS peak
    let tone: Vec<f32> = (0..n)
        .map(|i| amp * libm::sinf(2.0 * std::f32::consts::PI * 440.0 * (i as f32) / sr as f32))
        .collect();

    // ΤΑΥΤΟΣΗΜΑ κανάλια ⇒ corr = +1.0 ΑΚΡΙΒΩΣ
    let p_id = dir.join("oracle_identical.wav");
    write_wav(p_id.to_str().unwrap(), &tone, &tone, sr);

    // ΑΝΤΙΘΕΤΗ ΦΑΣΗ ⇒ corr = −1.0 ΑΚΡΙΒΩΣ
    let inv: Vec<f32> = tone.iter().map(|s| -s).collect();
    let p_inv = dir.join("oracle_antiphase.wav");
    write_wav(p_inv.to_str().unwrap(), &tone, &inv, sr);

    // ΑΝΕΞΑΡΤΗΤΟΣ ΘΟΡΥΒΟΣ ⇒ corr ≈ 0
    let mut s1 = 0x2026_08_25u32;
    let mut s2 = 0x0BAD_F00Du32;
    let mut lcg = |s: &mut u32| {
        *s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        ((*s >> 8) as f32 / 8_388_608.0 - 1.0) * amp
    };
    let nl: Vec<f32> = (0..n).map(|_| lcg(&mut s1)).collect();
    let nr: Vec<f32> = (0..n).map(|_| lcg(&mut s2)).collect();
    let p_noise = dir.join("oracle_noise.wav");
    write_wav(p_noise.to_str().unwrap(), &nl, &nr, sr);

    println!("=== ORACLE 1: ΣΥΣΧΕΤΙΣΗ (γνωστή σωστή απάντηση) ===");
    println!("  fixture            expected   cert_corr    dr        rms       lufs");
    for (p, tag, exp) in [
        (&p_id, "identical", 1.0f32),
        (&p_inv, "antiphase", -1.0),
        (&p_noise, "indep-noise", 0.0),
    ] {
        match run_live(p.to_str().unwrap(), tag) {
            Some((c, d, r, l, _w, _ce, _fl)) => println!(
                "  {tag:<18} {exp:>+6.2}   {c:>+9.5}  {d:>8.3}  {r:>8.3}  {l:>8.3}"
            ),
            None => println!("  {tag:<18} {exp:>+6.2}   (no cert)"),
        }
    }

    // ── ORACLE 2: γνωστό RMS ── ημίτονο −20 dBFS ⇒ RMS = −23.01
    println!();
    println!("=== ORACLE 2: ΓΝΩΣΤΟ RMS (−20 dBFS ημίτονο ⇒ −23.01 θεωρητικό) ===");
    println!("  ΤΟ ΚΡΙΣΙΜΟ ΕΙΝΑΙ Η ΣΥΜΦΩΝΙΑ ΜΕ ffmpeg astats ΣΤΟ ΙΔΙΟ ΑΡΧΕΙΟ,");
    println!("  ΟΧΙ με το θεωρητικό — η αλυσίδα αλλάζει τη στάθμη.");
    if let Some((c, d, r, l, _w, _ce, _fl)) = run_live(p_id.to_str().unwrap(), "rms") {
        println!("  cert: corr={c:+.5} dr={d:.3} rms_db={r:.4} lufs={l:.4}");
        println!("  lufs+3.0 (το ΠΑΛΙΟ fallback) = {:.4}", l + 3.0);
        println!("  διαφορά rms_db από fallback   = {:.4} dB", r - (l + 3.0));
    }

    // ── ORACLE 3: WIDTH + ΦΑΣΜΑΤΙΚΑ, γνωστές σωστές απαντήσεις ──
    // width = 1 − |corr| (stereo.rs:32) ⇒ ΜΗ-ΜΟΝΟΤΟΝΟ:
    //   ταυτόσημα (corr=+1) → 0.0 · αντίθετη φάση (corr=−1) → 0.0
    //   ασυσχέτιστος θόρυβος (corr≈0) → ≈1.0  ← ΤΟ ΜΕΓΙΣΤΟ
    println!();
    println!("=== ORACLE 3: WIDTH (μη-μονότονο: τα άκρα δίνουν ΤΟ ΙΔΙΟ) ===");
    println!("  fixture            exp_width  cert_width  cert_corr");
    for (tag, path, expw) in [
        ("identical", &p_id, 0.0f32),
        ("antiphase", &p_inv, 0.0f32),
        ("uncorrelated", &p_noise, 1.0f32),
    ] {
        match run_live(path.to_str().unwrap(), tag) {
            Some((c, _d, _r, _l, w, _ce, _fl)) => println!(
                "  {tag:<18} {expw:>6.2}     {w:>7.4}    {c:>+7.4}"
            ),
            None => println!("  {tag:<18} (no cert)"),
        }
    }

    // Φασματικά: ημίτονο 1k → centroid≈1000, flat≈0
    //            ημίτονο 8k → centroid≈8000  (ΟΧΙ 3200!)
    //            λευκός θόρυβος → flat≈1.0, centroid≈sr/4
    println!();
    println!("=== ORACLE 4: ΦΑΣΜΑΤΙΚΑ (γνωστές σωστές απαντήσεις) ===");
    println!("  fixture       exp_centroid  cert_centroid  exp_flat  cert_flat");
    let sr = 48_000u32;
    let n = sr as usize * 3;
    let mk_sine = |f: f32| -> Vec<f32> {
        (0..n).map(|i| 0.1 * libm::sinf(2.0 * std::f32::consts::PI * f * (i as f32) / sr as f32)).collect()
    };
    let s1k = mk_sine(1000.0);
    let s8k = mk_sine(8000.0);
    let mut st = 0x2026_08_25u64;
    let noise: Vec<f32> = (0..n).map(|_| {
        st = st.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((st >> 33) as f32 / (1u64 << 31) as f32 - 1.0) * 0.1
    }).collect();

    for (tag, sig, expc, expf) in [
        ("sine_1k", &s1k, 1000.0f32, 0.0f32),
        ("sine_8k", &s8k, 8000.0f32, 0.0f32),
        ("white_noise", &noise, (sr as f32) / 4.0, 1.0f32),
    ] {
        let p = dir.join(format!("{tag}.wav"));
        write_wav(p.to_str().unwrap(), sig, sig, sr);
        match run_live(p.to_str().unwrap(), tag) {
            Some((_c, _d, _r, _l, _w, ce, fl)) => println!(
                "  {tag:<13} {expc:>10.0}  {ce:>13.1}  {expf:>8.2}  {fl:>9.4}"
            ),
            None => println!("  {tag:<13} (no cert)"),
        }
    }

    println!();
    println!("DONE");
}
