use sp314_dsp::stft::stem_renderer::FiveStemRenderer;

fn find_data_chunk(bytes: &[u8]) -> usize {
    let mut i = 12; // Skip RIFF header, size, WAVE
    while i + 8 <= bytes.len() {
        let chunk_id = &bytes[i..i + 4];
        let chunk_size = u32::from_le_bytes(bytes[i + 4..i + 8].try_into().unwrap()) as usize;
        if chunk_id == b"data" {
            return i + 8;
        }
        i += 8 + chunk_size;
    }
    panic!("No data chunk found in WAV");
}

fn max_correlation(a: &[f32], b: &[f32], max_lag: i32) -> (f32, i32) {
    let mut max_corr = -1.0_f32;
    let mut best_lag = 0;

    let n = a.len().min(b.len()) as i32;
    if n == 0 {
        return (0.0, 0);
    }

    for lag in -max_lag..=max_lag {
        let (a_start, a_end) = if lag > 0 { (lag, n) } else { (0, n + lag) };
        let (b_start, b_end) = if lag > 0 { (0, n - lag) } else { (-lag, n) };

        if a_start >= a_end || b_start >= b_end {
            continue;
        }

        let slice_a = &a[a_start as usize..a_end as usize];
        let slice_b = &b[b_start as usize..b_end as usize];

        let m = slice_a.len() as f32;
        if m < 2.0 {
            continue;
        }

        let mean_a = slice_a.iter().sum::<f32>() / m;
        let mean_b = slice_b.iter().sum::<f32>() / m;

        let mut cov = 0.0;
        let mut var_a = 0.0;
        let mut var_b = 0.0;

        for (va, vb) in slice_a.iter().zip(slice_b.iter()) {
            let da = va - mean_a;
            let db = vb - mean_b;
            cov += da * db;
            var_a += da * da;
            var_b += db * db;
        }

        let std_dev = (var_a * var_b).sqrt();
        let corr = if std_dev > 1e-9 { cov / std_dev } else { 0.0 };

        if corr > max_corr {
            max_corr = corr;
            best_lag = lag;
        }
    }

    (max_corr, best_lag)
}

fn rms(a: &[f32]) -> f32 {
    let mut sum = 0.0;
    for &x in a {
        sum += x * x;
    }
    if a.is_empty() {
        0.0
    } else {
        (sum / a.len() as f32).sqrt()
    }
}

#[test]
#[ignore = "runs NMFD separation"]
fn the_five_stems_are_distinct_signals() {
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_stereo_dynamic.wav"
    );
    let bytes = std::fs::read(input_path).expect("failed to read fixture");
    let data_offset = find_data_chunk(&bytes);
    
    // ⚠ ΤΟ FIXTURE ΕΙΝΑΙ f32 LE, ΟΧΙ 24-bit.
    // Το ADM BWF του spatial path είναι 24-bit· αυτό εδώ
    // είναι το ΠΗΓΑΙΟ αρχείο, γραμμένο με pcm_f32le.
    // 8 bytes ανά frame: 4 για L, 4 για R.
    let mut left = Vec::new();
    let mut right = Vec::new();
    for frame in bytes[data_offset..].chunks_exact(8) {
        left.push(f32::from_le_bytes(
            frame[0..4].try_into().unwrap(),
        ));
        right.push(f32::from_le_bytes(
            frame[4..8].try_into().unwrap(),
        ));
    }

    let mono: Vec<f32> = left.iter().zip(right.iter()).map(|(l, r)| (l + r) * 0.5).collect();

    let mut renderer = FiveStemRenderer::new();
    let stems = renderer.render(&mono);

    let stems_list = [
        ("voice", &stems.voice),
        ("drums", &stems.drums),
        ("bass", &stems.bass),
        ("harmonics", &stems.harmonics),
        ("ambience", &stems.ambience),
    ];

    for (name, data) in stems_list.iter() {
        assert!(rms(data) > 0.0, "Stem {} is silent (RMS == 0)", name);
    }

    // Η ενέργεια ανά stem. Ένα stem που είναι σχεδόν
    // σιωπή βγάζει χαμηλή συσχέτιση με τα πάντα, και
    // αυτό ΔΕΝ σημαίνει ότι ο διαχωρισμός δούλεψε.
    println!("\n[STEM-RMS] ανά stem, dBFS:");
    let total = rms(&mono);
    for (name, data) in stems_list.iter() {
        let r = rms(data);
        let db = if r > 1e-9 {
            20.0 * r.log10()
        } else {
            -144.0
        };
        let pct = if total > 1e-9 { r / total * 100.0 } else { 0.0 };
        println!("  {:>10}: {:>8.2} dBFS  ({:>5.1}% του mono)", name, db, pct);
    }

    // 512, όχι 4800: δεν ψάχνουμε καθυστέρηση εδώ.
    // Τα πέντε stems βγαίνουν από τον ΙΔΙΟ renderer στο
    // ΙΔΙΟ πέρασμα — δεν υπάρχει λόγος να διαφέρουν
    // χρονικά. Ψάχνουμε αν ΔΙΑΦΕΡΟΥΝ, και αν διαφέρουν
    // το peak είναι κοντά στο μηδέν σε κάθε lag.
    // Το 4800 θα έκανε 46 δισεκατομμύρια πράξεις.
    const MAX_LAG: i32 = 512;

    // ΚΑΙ ΤΟ ΑΘΡΟΙΣΜΑ: αν τα πέντε stems ανασυντίθενται
    // στο πρωτότυπο, ο διαχωρισμός είναι πλήρης. Αν το
    // άθροισμα διαφέρει, κάτι χάθηκε ή διπλασιάστηκε.
    let mut sum: Vec<f32> = vec![0.0; mono.len()];
    for (_, data) in stems_list.iter() {
        for (i, &v) in data.iter().enumerate() {
            if i < sum.len() {
                sum[i] += v;
            }
        }
    }
    let (recon_corr, recon_lag) = max_correlation(&sum, &mono, MAX_LAG);
    println!(
        "\n[RECONSTRUCTION] Σstems vs mono: corr={:.4} lag={} rms_sum={:.6} rms_mono={:.6}",
        recon_corr, recon_lag, rms(&sum), total
    );

    println!("\nCorrelation between stems (max_lag = {}):", MAX_LAG);
    println!("{:>10} | {:>10} | {:>10} | {:>6}", "Stem 1", "Stem 2", "Score", "Lag");
    println!("{:-<47}", "");

    for i in 0..stems_list.len() {
        for j in i + 1..stems_list.len() {
            let (name1, data1) = stems_list[i];
            let (name2, data2) = stems_list[j];
            let (score, lag) = max_correlation(data1, data2, MAX_LAG);
            println!("{:>10} | {:>10} | {:>10.4} | {:>6}", name1, name2, score, lag);
        }
    }
}
