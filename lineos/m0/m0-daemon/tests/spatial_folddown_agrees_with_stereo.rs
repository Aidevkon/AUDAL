use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use sp314_dsp::spatial::five_dot_one::FiveDotOneStage;
use sp314_dsp::spatial::renderer::StereoRenderer;
use std::sync::Arc;
use std::time::Instant;
use xaak::repo::DspState;

fn i24_le_to_f32(b: &[u8]) -> f32 {
    let raw = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i8 as i32) << 16);
    raw as f32 / 8_388_607.0
}

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

    /// Το stereo master ΕΙΝΑΙ το fold-down του 5.1.
    ///
    /// ΜΕΤΡΗΜΕΝΟ, render_node.rs:344-356: το
    /// FiveDotOneStage χτίζεται από τα stems, γράφεται
    /// στο spatial dump, και ΤΟ ΙΔΙΟ stage περνάει από
    /// StereoRenderer::render για να δώσει το stereo
    /// master. Μία πηγή, δύο έξοδοι — αλλά η δεύτερη
    /// είναι ΠΑΡΑΓΩΓΗ της πρώτης, όχι αδελφή της.
    ///
    /// ΑΡΑ ΑΥΤΟ ΤΟ TEST ΔΕΝ ΑΠΟΔΕΙΚΝΥΕΙ ότι δύο
    /// ανεξάρτητες προβολές συμφωνούν. Δεν είναι
    /// ανεξάρτητες.
    ///
    /// ΤΙ ΑΠΟΔΕΙΚΝΥΕΙ: ότι το 5.1 επιβιώνει της
    /// διαδρομής του. Ανάμεσα στα δύο σημεία μεσολαβούν
    ///   · κβαντισμός σε 24-bit
    ///   · εγγραφή και ανάγνωση ADM BWF
    ///   · το conformance gain του spatial (-18 LUFS
    ///     και per-channel true peak scale)
    ///   · το limiting του stereo
    /// Αν κάποιο από αυτά αλλοίωνε το σήμα δομικά — λάθος
    /// κανάλι, αντεστραμμένη φάση, χαμένο stem, σφάλμα
    /// στο i24 round-trip — η συσχέτιση θα κατέρρεε.
    ///
    /// ΚΑΙ ΕΝΑ ΑΝΟΙΧΤΟ: επειδή το fold-down αθροίζει
    /// L+0.707·C+0.707·Ls+LFE και R+0.707·C+0.707·Rs+LFE,
    /// αν το FiveDotOneStage παράγει συμμετρικά ls/rs
    /// τότε L_out == R_out ΕΞ ΟΡΙΣΜΟΥ — και το
    /// [BISECT-3-RENDER] το επιβεβαιώνει με
    /// ratio=1.0000 ακόμα και σε fixture που μπαίνει με
    /// 4.2 dB διαφορά L/R.
    /// Δηλαδή το stereo output είναι στην πράξη mono.
    /// ΔΕΝ διορθώνεται εδώ· καταγράφεται.
#[test]
#[ignore]
fn spatial_folddown_agrees_with_stereo() {
    // ΠΡΑΓΜΑΤΙΚΟ ΥΛΙΚΟ, ΟΧΙ ΗΜΙΤΟΝΟ.
    //
    // Το test_stereo_input.wav είναι καθαρό ημίτονο:
    // crest 1.414 (√2, δηλαδή 3 dB), LRA 0.0,
    // L_rms == R_rms με ακρίβεια έξι δεκαδικών. Με αυτό
    // το fold-down έδινε corr=1.0000 — αλλά επειδή ο
    // limiter δεν ενεργοποιούνταν ΚΑΘΟΛΟΥ
    // (correction -0.038 dB, peak 12 dB κάτω από το
    // ceiling). Δύο ΓΡΑΜΜΙΚΕΣ διαδρομές συμφωνούσαν, που
    // δεν αποδεικνύει τίποτα για το πραγματικό σύστημα.
    //
    // Αυτό εδώ είναι 10s πραγματικής μουσικής:
    //   LUFS -26.7, peak R -9.86 → shift +12.7 dB προς
    //   τα -14 → peak +2.84 dBTP έναντι ceiling -1.0.
    //   Ο limiter ΘΑ κόψει ~4 dB.
    // Το fixture είναι ΠΡΑΓΜΑΤΙΚΑ stereo — L peak -14.07,
    // R peak -9.86 — αλλά ΤΟ STEREO WIDTH ΔΕΝ ΔΟΚΙΜΑΖΕΤΑΙ:
    // μπαίνει με 4.2 dB διαφορά L/R (peak L -14.07,
    // R -9.86): το fold-down το ισοπεδώνει. Βλ. doc
    // comment στην κορυφή.
    //
    // ΤΟ ΣΚΟΡ ΔΕΝ ΕΠΕΣΕ: 0.9992. Η μη γραμμικότητα του
    // limiter επηρεάζει ΚΑΙ ΤΙΣ ΔΥΟ πλευρές, γιατί το
    // stereo ΕΙΝΑΙ το fold-down. Βλ. doc comment.
    let input_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/test_stereo_dynamic.wav"
    );

    let req = MasterRequest {
        audio_path: input_path.to_string(),
        preset_id: "spatial_upmix".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        project_id: None,
        track_id: Some("test-spatial-folddown-001".to_string()),
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let head_state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let state_tmp = tempfile::TempDir::new().unwrap();

    let result = run_dsp(
        &req,
        Instant::now(),
        head_state,
        None,
        None,
        "job-folddown-test".to_string(),
        state_tmp.path().to_str().unwrap(),
        "/tmp",
    );

    assert!(result.is_ok(), "run_dsp failed: {:?}", result.err());

    let (stereo_blob, spatial_blob_opt, _, _, _, _) = result.unwrap();
    let spatial_blob = spatial_blob_opt.expect("spatial_upmix preset must produce a spatial blob");

    let stereo_data = std::fs::read(stereo_blob.core.audio_path.path()).expect("failed to read stereo blob");
    if stereo_data.starts_with(b"RIFF") {
        panic!("Stereo audio_path is not raw f32 (it is a WAV container)");
    }

    let mut stereo_l = Vec::new();
    let mut stereo_r = Vec::new();
    for chunk in stereo_data.chunks_exact(8) {
        stereo_l.push(f32::from_le_bytes(chunk[0..4].try_into().unwrap()));
        stereo_r.push(f32::from_le_bytes(chunk[4..8].try_into().unwrap()));
    }

    let spatial_data = std::fs::read(spatial_blob.core.audio_path.path()).expect("failed to read spatial blob");
    let data_offset = find_data_chunk(&spatial_data);

    let mut spatial_l = Vec::new();
    let mut spatial_r = Vec::new();
    let mut spatial_c = Vec::new();
    let mut spatial_lfe = Vec::new();
    let mut spatial_ls = Vec::new();
    let mut spatial_rs = Vec::new();

    let bytes_per_frame = 6 * 3; // 6 channels * 3 bytes (24-bit)
    for chunk in spatial_data[data_offset..].chunks_exact(bytes_per_frame) {
        spatial_l.push(i24_le_to_f32(&chunk[0..3]));
        spatial_r.push(i24_le_to_f32(&chunk[3..6]));
        spatial_c.push(i24_le_to_f32(&chunk[6..9]));
        spatial_lfe.push(i24_le_to_f32(&chunk[9..12]));
        spatial_ls.push(i24_le_to_f32(&chunk[12..15]));
        spatial_rs.push(i24_le_to_f32(&chunk[15..18]));
    }

    let stage = FiveDotOneStage {
        l: spatial_l,
        r: spatial_r,
        c: spatial_c,
        ls: spatial_ls,
        rs: spatial_rs,
        lfe: spatial_lfe,
    };
    let (fold_l, fold_r) = StereoRenderer::render(&stage);

    let n_spatial = fold_l.len();
    let n_stereo = stereo_l.len();

    let (cl, ll) = max_correlation(&fold_l, &stereo_l, 512);
    let (cr, lr) = max_correlation(&fold_r, &stereo_r, 512);

    println!("[FOLDDOWN] L: corr={:.4} lag={}", cl, ll);
    println!("[FOLDDOWN] R: corr={:.4} lag={}", cr, lr);
    println!("[FOLDDOWN] frames: spatial={} stereo={}", n_spatial, n_stereo);

    // ΚΛΕΙΔΩΜΕΝΟ ΑΠΟ ΜΕΤΡΗΣΗ 2026-08-12.
    //
    // fixture test_stereo_dynamic.wav, 10s πραγματικής
    // μουσικής, LUFS -26.7 → shift +12.7 dB → peak
    // +1.50 dBTP έναντι ceiling -1.0.
    // Ο limiter ΔΟΥΛΕΨΕ: [W17-POST-MASTER] tp=-1.1269.
    //
    //   corr = 0.9992, lag = 0, και στα δύο κανάλια
    //
    // Το 0.99 είναι λίγο κάτω από το μετρημένο. Δεν
    // υπάρχει λόγος για χαλαρότερο: το μόνο που
    // μεσολαβεί είναι κβαντισμός και limiting, και
    // μετρήθηκε ότι κοστίζουν 0.0008.
    //
    // ΑΝ ΠΕΣΕΙ ΚΑΤΩ ΑΠΟ 0.99: κάτι άλλαξε στο i24
    // round-trip, στο ADM BWF, ή στο conformance gain.
    // ΜΗΝ χαλαρώσεις το κατώφλι — βρες τι άλλαξε.
    const FOLDDOWN_MIN_CORR: f32 = 0.99;

    assert!(
        cl > FOLDDOWN_MIN_CORR,
        "L fold-down disagrees with stereo master: {cl:.4} \
         (measured 1.0000 on this fixture 2026-08-12)"
    );
    assert!(
        cr > FOLDDOWN_MIN_CORR,
        "R fold-down disagrees with stereo master: {cr:.4} \
         (measured 1.0000 on this fixture 2026-08-12)"
    );

    // ── ΤΟ LAG ΕΙΝΑΙ ΜΕΡΟΣ ΤΗΣ ΑΠΟΔΕΙΞΗΣ ──
    //
    // Υψηλό σκορ σε ΛΑΘΟΣ lag δεν σημαίνει τίποτα.
    // Το unit test sweep_cannot_distinguish_inversion_
    // from_half_period το δείχνει: αντεστραμμένο σήμα
    // δίνει corr=0.9749 σε lag=-224, δηλαδή περνάει
    // κάθε κατώφλι σκορ ενώ το υλικό είναι ΑΚΥΡΩΜΕΝΟ.
    //
    // Υγιές peak βρίσκεται είτε στο 0 (καμία
    // καθυστέρηση) είτε κοντά στο 240 — το lookahead
    // του limiter, 5ms στα 48k (dsp/mod.rs,
    // sample_rate * 0.005).
    //
    // ΜΕΤΡΗΜΕΝΟ σε αυτό το fixture: lag = 0, γιατί το
    // limiter δεν ενεργοποιήθηκε καθόλου.
    //
    // Οτιδήποτε αλλού είναι ΥΠΟΠΤΟ όσο υψηλό κι αν είναι
    // το σκορ.
    const LOOKAHEAD_SAMPLES: i32 = 240;
    const LAG_TOLERANCE: i32 = 32;

    let lag_ok = |lag: i32| {
        lag.abs() <= LAG_TOLERANCE
            || (lag.abs() - LOOKAHEAD_SAMPLES).abs() <= LAG_TOLERANCE
    };

    assert!(
        lag_ok(ll),
        "L peak at implausible lag {ll} — expected near 0 \
         or near ±{LOOKAHEAD_SAMPLES} (limiter lookahead). \
         A high score at an arbitrary lag can be a false \
         match, not agreement."
    );
    assert!(
        lag_ok(lr),
        "R peak at implausible lag {lr} — expected near 0 \
         or near ±{LOOKAHEAD_SAMPLES} (limiter lookahead)."
    );
}

fn test_signal(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32;
            (t * 0.013).sin() * 0.6 + (t * 0.071).sin() * 0.3
        })
        .collect()
}

#[test]
fn correlation_identical_is_one_at_zero_lag() {
    let a = test_signal(4096);
    let (corr, lag) = max_correlation(&a, &a, 512);
    assert!(corr > 0.999, "identical signals: {corr:.6}");
    assert_eq!(lag, 0, "identical signals must align at 0");
}

    // Η Pearson σε lag 0 — χωρίς σάρωση.
    //
    // Αποδεικνύει ότι η ΜΑΘΗΜΑΤΙΚΗ είναι σωστή: ένα
    // σήμα πολλαπλασιασμένο με -1 δίνει -1.0. Αν αυτό
    // σπάσει, η υλοποίηση της συσχέτισης είναι λάθος
    // και κάθε άλλο νούμερο σε αυτό το αρχείο είναι
    // άχρηστο.
    //
    // ΓΙΑΤΙ max_lag = 0: χωρίς σάρωση δεν υπάρχει άλλο
    // lag να διαλέξει η max_correlation.
    #[test]
    fn correlation_inverted_at_zero_lag_is_minus_one() {
        let a = test_signal(4096);
        let b: Vec<f32> = a.iter().map(|s| -s).collect();
        let (corr, lag) = max_correlation(&a, &b, 0);
        assert_eq!(lag, 0, "no sweep requested");
        assert!(
            corr < -0.999,
            "inverted at zero lag must be -1: {corr:.6}"
        );
    }

    // ΤΟ ΟΡΙΟ ΤΗΣ ΜΕΤΡΙΚΗΣ, ΤΕΚΜΗΡΙΩΜΕΝΟ.
    //
    // Με σάρωση, το ΙΔΙΟ αντεστραμμένο σήμα δίνει ~0.97.
    // Η σάρωση βρίσκει lag κοντά στη μισή περίοδο
    // (~242 samples για το test_signal) όπου το
    // αντεστραμμένο ταυτίζεται με το πρωτότυπο.
    //
    // ΔΕΝ ΕΙΝΑΙ ΣΦΑΛΜΑ — είναι ιδιότητα της ολίσθησης
    // πάνω σε περιοδικό υλικό. Γράφεται εδώ ώστε κανείς
    // να μη νομίσει ότι το e2e από πάνω πιάνει
    // αντιστροφή φάσης. ΔΕΝ την πιάνει, όταν το υλικό
    // είναι περιοδικό και η μισή περίοδος χωράει στο
    // παράθυρο.
    //
    // ΜΕΤΡΗΜΕΝΟ 2026-08-12: corr=0.974936
    #[test]
    fn sweep_cannot_distinguish_inversion_from_half_period() {
        let a = test_signal(4096);
        let b: Vec<f32> = a.iter().map(|s| -s).collect();
        let (corr, lag) = max_correlation(&a, &b, 512);

        println!(
            "[LIMIT] inverted with sweep: corr={corr:.4} lag={lag}"
        );

        // Το peak βρίσκεται ΟΧΙ στο 0 — εκεί είναι -1.
        assert_ne!(lag, 0, "sweep must have moved off zero");
        assert!(
            corr > 0.9,
            "documented limit: sweep finds a false match \
             at half period, got {corr:.6}"
        );
    }

#[test]
fn correlation_finds_the_shift() {
    let a = test_signal(4096);
    const SHIFT: usize = 100;
    let b: Vec<f32> = std::iter::repeat(0.0)
        .take(SHIFT)
        .chain(a.iter().copied())
        .collect();
    let (corr, lag) = max_correlation(&a, &b, 512);
    assert!(corr > 0.99, "shifted signal: {corr:.6}");
    assert_eq!(
        lag.unsigned_abs() as usize, SHIFT,
        "must report the shift it found: got {lag}"
    );
}

#[test]
fn correlation_unrelated_is_low() {
    let a = test_signal(4096);
    let b: Vec<f32> = (0..4096)
        .map(|i| {
            let t = i as f32;
            (t * 0.211).sin() * 0.5 + (t * 0.037).cos() * 0.4
        })
        .collect();
    let (corr, lag) = max_correlation(&a, &b, 512);
    println!("[UNRELATED] corr={corr:.4} lag={lag}");
    assert!(
        corr < 0.5,
        "unrelated signals must fall below the e2e floor: {corr:.6}"
    );
}

// ── ΤΙ ΔΕΝ ΚΑΛΥΠΤΕΙ ΑΥΤΟ ΤΟ ΑΡΧΕΙΟ ──
//
// Το e2e παραπάνω χρησιμοποιεί max_lag=512 και
// επέστρεψε corr=1.0000 lag=0. Το lag=0 είναι το
// καθησυχαστικό: το peak βρέθηκε ΧΩΡΙΣ ολίσθηση,
// άρα δεν είναι ψευδής ταύτιση μισής περιόδου.
//
// ΑΛΛΑ αν μια μελλοντική αλλαγή εισάγει αντιστροφή
// φάσης ΚΑΙ το υλικό είναι περιοδικό, το e2e μπορεί
// να δείξει υψηλό σκορ σε μη μηδενικό lag.
// ΓΙ' ΑΥΤΟ ΤΟ e2e ΤΥΠΩΝΕΙ ΤΟ LAG: ένα peak μακριά
// από το 0 ή το 240 (lookahead του limiter) είναι
// ύποπτο, όσο υψηλό κι αν είναι το σκορ.
//
// Πραγματικό, μη περιοδικό υλικό δεν έχει αυτό το
// πρόβλημα. Το fixture είναι synthetic — αυτή είναι
// η αδυναμία, όχι η μετρική.
//
// ΜΕΤΑ ΤΟ ΚΛΕΙΔΩΜΑ: ο έλεγχος του lag είναι αυτός
// που κλείνει το κενό. Το σκορ λέει "μοιάζουν"· το
// lag λέει "και ταιριάζουν εκεί που πρέπει". Χωρίς
// το δεύτερο, μια αντιστροφή φάσης σε περιοδικό
// υλικό θα περνούσε με 0.97.
