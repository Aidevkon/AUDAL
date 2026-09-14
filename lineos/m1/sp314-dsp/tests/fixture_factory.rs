#[cfg(test)]
mod tests {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    use sha2::{Digest, Sha256};
    use sp314_dsp::analysis::vad_features::VadFeatureExtractor;
    use sp314_dsp::analysis::vad_model::{FixedPriors, VadClassifier};
    use sp314_dsp::io::flac_writer::FlacWriter;
    use sp314_dsp::metering::lufs::measure_integrated_lufs;
    use std::collections::HashMap;
    use std::fs::File;
    use std::io::Write;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    struct Lcg {
        state: u64,
    }
    impl Lcg {
        fn new(seed: u64) -> Self {
            Lcg { state: seed }
        }
        fn next(&mut self) -> u64 {
            self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
            self.state
        }
        fn next_float(&mut self) -> f32 {
            (self.next() >> 40) as f32 / 16777216.0
        }
    }

    fn hash_file(path: &str) -> String {
        let data = std::fs::read(path).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(&data);
        format!("{:x}", hasher.finalize())
    }

    fn get_audio_excerpt(path: &str, target_sr: u32, duration_sec: f32, rng: &mut Lcg) -> Vec<f32> {
        let file = Box::new(File::open(path).unwrap());
        let mss = MediaSourceStream::new(file, Default::default());
        let hint = Hint::new();
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap();
        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        let track_id = track.id;
        let original_sr = track.codec_params.sample_rate.unwrap();

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .unwrap();

        let mut mono_samples = Vec::new();
        loop {
            let packet = match format.next_packet() {
                Ok(packet) => packet,
                Err(_) => break,
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let channels = decoded.spec().channels.count();
                    let mut sample_buf =
                        SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    sample_buf.copy_interleaved_ref(decoded.clone());
                    let samples = sample_buf.samples();
                    for i in (0..samples.len()).step_by(channels) {
                        let mut sum = 0.0;
                        for c in 0..channels {
                            sum += samples[i + c];
                        }
                        mono_samples.push(sum / channels as f32);
                    }
                }
                Err(_) => break,
            }
        }

        let req_samples = (duration_sec * original_sr as f32) as usize;
        let max_offset = mono_samples.len().saturating_sub(req_samples);
        let offset = (rng.next_float() * max_offset as f32) as usize;
        let excerpt =
            mono_samples[offset..std::cmp::min(offset + req_samples, mono_samples.len())].to_vec();

        if original_sr == target_sr {
            return excerpt;
        }

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler = SincFixedIn::<f32>::new(
            target_sr as f64 / original_sr as f64,
            2.0,
            params,
            excerpt.len(),
            1,
        )
        .unwrap();

        let waves_in = vec![excerpt];
        let waves_out = resampler.process(&waves_in, None).unwrap();
        waves_out[0].clone()
    }

    fn evaluate_vad(audio: &[f32]) -> f32 {
        let mut ext = VadFeatureExtractor::new();
        let feats = ext.process_chunk(audio, audio, audio);
        let mut vad = VadClassifier::new(FixedPriors);
        let mut speech_frames = 0;
        for f in &feats {
            if vad.process(f, -144.0).is_speech {
                speech_frames += 1;
            }
        }
        100.0 * speech_frames as f32 / feats.len().max(1) as f32
    }


    // ══ ΓΕΝΝΗΤΡΙΑ ΔΟΚΙΜΙΩΝ ΜΕ ΓΝΩΣΤΟ ΒΟΜΒΟ ════════════════════════════════
    // ΜΟΝΟ ΥΛΙΚΟ. ΜΗΔΕΝ ΑΝΙΧΝΕΥΤΗΣ — ο ανιχνευτής είναι επόμενο βήμα και
    // αυτά τα δοκίμια υπάρχουν για να τον κρίνουν, όχι να τον ορίσουν.
    //
    // ΓΙΑΤΙ ΥΠΑΡΧΟΥΝ: μετρήθηκε 2026-09-14 ότι και τα είκοσι αρχεία του
    // LibriVox corpus δείχνουν 60 Hz ή τίποτα — ΚΑΝΕΝΑ στα 50 με προεξοχή
    // πάνω από 3.6 dB. Ανιχνευτής που πρέπει να ΔΙΑΛΕΓΕΙ ανάμεσα σε 50 και
    // 60 δεν μπορεί να δοκιμαστεί σε δείγμα που έχει μόνο τη μία απάντηση.

    /// Ημίτονο δικτύου: θεμελιώδης + προαιρετικές αρμονικές με φθίνουσα στάθμη.
    ///
    /// ⚠ ΤΟ ΠΡΑΓΜΑΤΙΚΟ ΔΙΚΤΥΟ ΔΕΝ ΕΙΝΑΙ ΚΑΘΑΡΟ ΗΜΙΤΟΝΟ, ΚΑΙ ΤΟ F-082 ΤΟ
    /// ΔΗΛΩΝΕΙ ΩΣ ΟΡΙΟ ΤΟΥ ΙΔΙΟΥ ΤΟΥ ΕΥΡΗΜΑΤΟΣ. Η παράμετρος `harmonics`
    /// υπάρχει, ΑΛΛΑ τα δοκίμια παράγονται με harmonics = 1.
    ///
    /// ΓΙΑΤΙ: το `harmonic_decay_db` έπρεπε να μετρηθεί από τα πραγματικά.
    /// ΜΕΤΡΗΘΗΚΕ 2026-09-14 — η πτώση 60→120 Hz στα αρχεία που την έδειξαν:
    ///   anne +33.2 · monte_cristo +25.3 · pinocchio +5.6 · dracula +3.2 dB
    /// Εύρος 30.0 dB σε n=4. ΔΕΝ ΥΠΑΡΧΕΙ ΣΤΑΘΕΡΟΣ ΛΟΓΟΣ — οπότε δεν
    /// εφευρίσκεται ένας. Μόνο θεμελιώδης, ΔΗΛΩΜΕΝΑ.
    fn mains_hum(
        sr: u32,
        dur_sec: f32,
        fundamental_hz: f32,
        harmonics: usize,
        harmonic_decay_db: f32,
        amplitude: f32,
    ) -> Vec<f32> {
        let n = (dur_sec * sr as f32) as usize;
        let mut out = vec![0.0_f32; n];
        for h in 1..=harmonics.max(1) {
            let f = fundamental_hz * h as f32;
            if f >= sr as f32 / 2.0 {
                break;
            }
            let a = amplitude * 10.0_f32.powf(-harmonic_decay_db * (h - 1) as f32 / 20.0);
            for (i, s) in out.iter_mut().enumerate() {
                let t = i as f32 / sr as f32;
                *s += a * (2.0 * std::f32::consts::PI * f * t).sin();
            }
        }
        out
    }

    /// Welch PSD σε dB — ΙΔΙΕΣ παράμετροι με το όργανο που θα επαληθεύσει
    /// (research/encoder-gap-speech/src/bin/hum_spectrum.rs): N=16384, Hann,
    /// 50% επικάλυψη. Αν αποκλίνουν, η «ζητούμενη προεξοχή» και η «μετρημένη»
    /// θα μετρούσαν διαφορετικά πράγματα και η σύγκριση δεν θα σήμαινε τίποτα.
    fn welch_psd_db(x: &[f32]) -> Vec<f32> {
        use rustfft::{num_complex::Complex, FftPlanner};
        const NFFT: usize = 16_384;
        let hop = NFFT / 2;
        let win: Vec<f32> = (0..NFFT)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / NFFT as f32).cos())
            .collect();
        let wpow: f64 = win.iter().map(|&v| (v as f64) * (v as f64)).sum();
        let mut acc = vec![0.0_f64; NFFT / 2 + 1];
        let mut segs = 0usize;
        let mut off = 0usize;
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(NFFT);
        while off + NFFT <= x.len() {
            let mut buf: Vec<Complex<f32>> = (0..NFFT)
                .map(|i| Complex::new(x[off + i] * win[i], 0.0))
                .collect();
            fft.process(&mut buf);
            for (k, a) in acc.iter_mut().enumerate() {
                let p = (buf[k].re as f64).powi(2) + (buf[k].im as f64).powi(2);
                let scale = if k == 0 || k == NFFT / 2 { 1.0 } else { 2.0 };
                *a += scale * p / wpow;
            }
            segs += 1;
            off += hop;
        }
        assert!(segs > 0, "σήμα πιο κοντό από ένα τμήμα Welch");
        acc.iter()
            .map(|&v| {
                let m = v / segs as f64;
                if m < 1e-30 {
                    -300.0
                } else {
                    (10.0 * m.log10()) as f32
                }
            })
            .collect()
    }

    /// Τοπικό πάτωμα: διάμεσος σε ±25 Hz, ΕΞΑΙΡΩΝΤΑΣ ±4 Hz γύρω από τον κάδο.
    /// ΙΔΙΟΣ ορισμός με το όργανο επαλήθευσης — και ΙΔΙΟΣ με το «λόγος
    /// κορυφής προς γείτονες» που όρισε το κατώφλι +8 dB του F-082.
    fn local_floor_db(psd: &[f32], k: usize, sr: u32) -> f32 {
        let bin_hz = sr as f32 / 16_384.0;
        let span = (25.0 / bin_hz) as usize;
        let skip = (4.0 / bin_hz) as usize;
        let lo = k.saturating_sub(span);
        let hi = (k + span).min(psd.len() - 1);
        let mut v: Vec<f32> = (lo..=hi)
            .filter(|&i| i < k.saturating_sub(skip) || i > k + skip)
            .map(|i| psd[i])
            .collect();
        assert!(!v.is_empty(), "κενή γειτονιά στον κάδο {k}");
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }

    /// Η ΣΤΑΘΜΗ ΒΓΑΙΝΕΙ ΑΠΟ ΤΗΝ ΠΡΟΕΞΟΧΗ, ΟΧΙ ΑΠΟ ΑΠΟΛΥΤΟ dBFS.
    ///
    /// ΔΥΟ ΔΡΟΜΟΙ ΥΠΗΡΧΑΝ: απόλυτη στάθμη ημιτόνου σε dBFS, ή προεξοχή πάνω
    /// από το πάτωμα του φορέα. ΔΙΑΛΕΧΤΗΚΕ Ο ΔΕΥΤΕΡΟΣ, γιατί είναι ΑΚΡΙΒΩΣ
    /// το μέγεθος που μετράει ο ανιχνευτής (F-082: λόγος κορυφής προς
    /// γείτονες). Ένα απόλυτο dBFS θα έδινε άλλη προεξοχή σε κάθε φορέα.
    ///
    /// Η PSD είναι τετραγωνική στο πλάτος, άρα το κέρδος βγαίνει σε ΕΝΑ βήμα:
    ///   ζητούμενη κορυφή = πάτωμα_φορέα + προεξοχή
    ///   κέρδος_dB        = ζητούμενη κορυφή − κορυφή_ημιτόνου_στο_πλάτος_1
    /// Otsu σε 1-D ιστόγραμμα: η τομή που μεγιστοποιεί τη διακύμανση ΑΝΑΜΕΣΑ
    /// στις δύο κλάσεις.
    ///
    /// SOURCE: N. Otsu, "A Threshold Selection Method from Gray-Level Histograms",
    ///   IEEE Transactions on Systems, Man and Cybernetics, vol. 9, pp. 62-66, 1979,
    ///   DOI 10.1109/TSMC.1979.4310076
    /// RETRIEVED: 2026-09-14 (Semantic Scholar Graph API, εγγραφή DOI)
    ///
    /// ⚠ ΑΝΤΙΓΡΑΦΟ — ΔΗΛΩΜΕΝΟ. Ο ΙΔΙΟΣ αλγόριθμος ζει στο trunk_pass.rs
    /// (sp314-orchestrator, ιδιωτικός) και στο hum_spectrum.rs (WS2). ΤΟ
    /// sp314-dsp ΔΕΝ ΜΠΟΡΕΙ ΝΑ ΕΞΑΡΤΗΘΕΙ ΑΠΟ ΚΑΝΕΝΑ ΑΠΟ ΤΑ ΔΥΟ: ο
    /// orchestrator εξαρτάται ΑΠΟ ΑΥΤΟ (κύκλος), και το WS2 είναι ΑΛΛΟ
    /// workspace. Δεν υπάρχει τρόπος επαναχρησιμοποίησης χωρίς νέο crate.
    fn otsu_split_bin(hist: &[u32]) -> Option<usize> {
        let total: f64 = hist.iter().map(|&c| c as f64).sum();
        if total == 0.0 {
            return None;
        }
        let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
        let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
        let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
        for t in 0..hist.len() {
            w0 += hist[t] as f64;
            if w0 == 0.0 {
                continue;
            }
            let w1 = total - w0;
            if w1 == 0.0 {
                break;
            }
            sum0 += t as f64 * hist[t] as f64;
            let m0 = sum0 / w0;
            let m1 = (sum_all - sum0) / w1;
            let var = w0 * w1 * (m0 - m1) * (m0 - m1);
            if var > best_var {
                best_var = var;
                best_t = t;
            }
        }
        if best_var < 0.0 {
            None
        } else {
            Some(best_t)
        }
    }

    /// Η ΠΑΥΣΗ — ΤΟ ΙΔΙΟ ΚΡΙΤΗΡΙΟ ΜΕ ΤΟΝ ΑΝΙΧΝΕΥΤΗ.
    ///
    /// ⚠ ΑΝΤΙΓΡΑΦΟ ΤΟΥ `longest_pause` ΤΟΥ hum_spectrum.rs (WS2), ΓΙΑ ΤΟΝ
    /// ΙΔΙΟ ΛΟΓΟ: άλλο workspace, μηδέν διαδρομή εξάρτησης. ΚΑΘΕ ΑΛΛΑΓΗ
    /// ΕΔΩ ΠΡΕΠΕΙ ΝΑ ΓΙΝΕΙ ΚΑΙ ΕΚΕΙ — αλλιώς η «ζητούμενη» και η
    /// «μετρημένη» προεξοχή σταματούν να μετράνε το ίδιο πράγμα, που είναι
    /// ΑΚΡΙΒΩΣ το σφάλμα που έβγαλε +32 dB απόκλιση στην πρώτη δοκιμή.
    ///
    /// Παράθυρα 100 ms, κατώφλι = η τομή Otsu του ΙΔΙΟΥ του σήματος, κάδοι
    /// 1 dB από −100 ως 0. Επιστρέφει (start_sample, len_samples).
    fn longest_pause(x: &[f32], sr: u32) -> Option<(usize, usize)> {
        let w = (sr as f32 * 0.100) as usize;
        let rms_db = |c: &[f32]| -> f32 {
            let e = c.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / c.len() as f64;
            if e < 1e-20 {
                -200.0
            } else {
                (10.0 * e.log10()) as f32
            }
        };
        let windows: Vec<f32> = x.chunks(w).filter(|c| c.len() == w).map(rms_db).collect();
        if windows.is_empty() {
            return None;
        }
        let mut hist = [0u32; 100];
        for &v in &windows {
            let b = (v + 100.0).floor();
            if b >= 0.0 && (b as usize) < 100 {
                hist[b as usize] += 1;
            }
        }
        let thr = -100.0 + otsu_split_bin(&hist)? as f32 + 0.5;

        let (mut best, mut cur_start, mut cur_len) = ((0usize, 0usize), 0usize, 0usize);
        for (i, &v) in windows.iter().enumerate() {
            if v < thr {
                if cur_len == 0 {
                    cur_start = i * w;
                }
                cur_len += w;
            } else {
                if cur_len > best.1 {
                    best = (cur_start, cur_len);
                }
                cur_len = 0;
            }
        }
        if cur_len > best.1 {
            best = (cur_start, cur_len);
        }
        if best.1 == 0 {
            None
        } else {
            Some(best)
        }
    }

    /// Η ΣΤΑΘΜΗ ΒΓΑΙΝΕΙ ΑΠΟ ΤΗΝ ΠΡΟΕΞΟΧΗ, ΟΧΙ ΑΠΟ ΑΠΟΛΥΤΟ dBFS.
    ///
    /// ΔΥΟ ΔΡΟΜΟΙ ΥΠΗΡΧΑΝ: απόλυτη στάθμη ημιτόνου σε dBFS, ή προεξοχή πάνω
    /// από το πάτωμα του φορέα. ΔΙΑΛΕΧΤΗΚΕ Ο ΔΕΥΤΕΡΟΣ, γιατί είναι ΑΚΡΙΒΩΣ
    /// το μέγεθος που μετράει ο ανιχνευτής (F-082: λόγος κορυφής προς
    /// γείτονες). Ένα απόλυτο dBFS θα έδινε άλλη προεξοχή σε κάθε φορέα.
    ///
    /// ⚠ ΤΟ ΤΕΚΜΗΡΙΟ ΓΕΝΝΙΕΤΑΙ ΣΤΗ ΘΕΣΗ ΤΟΥ [§5.5]. Η ΠΡΩΤΗ ΕΚΔΟΧΗ ΜΕΤΡΟΥΣΕ
    /// ΤΟ ΠΑΤΩΜΑ ΣΕ ΟΛΟΚΛΗΡΟ ΤΟ ΑΠΟΣΠΑΣΜΑ ΤΩΝ 30 s — που περιέχει ομιλία —
    /// ενώ ο ανιχνευτής μετράει ΜΟΝΟ ΜΕΣΑ ΣΤΗΝ ΠΑΥΣΗ. ΜΕΤΡΗΘΗΚΕ: απόκλιση
    /// 27.01 έως 33.19 dB σε 6/6 δοκίμια, σχεδόν σταθερή — μετατόπιση
    /// αναφοράς, όχι θόρυβος. Η αναφορά μετριέται τώρα ΣΤΗΝ ΠΑΥΣΗ.
    ///
    /// Η PSD είναι τετραγωνική στο πλάτος, άρα το κέρδος βγαίνει σε ΕΝΑ βήμα:
    ///   ζητούμενη κορυφή = πάτωμα_παύσης + προεξοχή
    ///   κέρδος_dB        = ζητούμενη κορυφή − κορυφή_ημιτόνου_στο_πλάτος_1
    fn amplitude_for_prominence(carrier: &[f32], sr: u32, f_hz: f32, prominence_db: f32) -> f32 {
        let bin_hz = sr as f32 / 16_384.0;
        let k = (f_hz / bin_hz).round() as usize;

        let (a, n) = longest_pause(carrier, sr).expect("ο φορέας δεν έχει παύση");
        let pause = &carrier[a..a + n];
        let psd_pause = welch_psd_db(pause);
        let floor_db = local_floor_db(&psd_pause, k, sr);

        // Το ημίτονο αναφοράς έχει ΤΟ ΙΔΙΟ ΜΗΚΟΣ με την παύση — αλλιώς ο
        // αριθμός των τμημάτων Welch διαφέρει και μαζί του η κανονικοποίηση.
        let unit = mains_hum(sr, n as f32 / sr as f32, f_hz, 1, 0.0, 1.0);
        let psd_unit = welch_psd_db(&unit);

        let gain_db = (floor_db + prominence_db) - psd_unit[k];
        10.0_f32.powf(gain_db / 20.0)
    }

    /// Η ΠΡΟΕΞΟΧΗ ΟΠΩΣ ΘΑ ΤΗ ΔΙΑΒΑΣΕΙ Ο ΑΝΙΧΝΕΥΤΗΣ — μετρημένη στο ΤΕΛΙΚΟ
    /// μίγμα, μέσα στην παύση, με τον ΙΔΙΟ ορισμό τοπικού μέσου.
    /// ΑΥΤΟ γράφεται στο manifest, ΟΧΙ το ζητούμενο.
    fn measured_prominence(mix: &[f32], sr: u32, f_hz: f32) -> f32 {
        let bin_hz = sr as f32 / 16_384.0;
        let k = (f_hz / bin_hz).round() as usize;
        let (a, n) = longest_pause(mix, sr).expect("το μίγμα δεν έχει παύση");
        let psd = welch_psd_db(&mix[a..a + n]);
        psd[k] - local_floor_db(&psd, k, sr)
    }

    /// Ντετερμινιστικό απόσπασμα από ΡΗΤΗ χρονική θέση.
    ///
    /// ΓΙΑΤΙ ΞΕΧΩΡΙΣΤΗ ΑΠΟ ΤΗΝ `get_audio_excerpt`: εκείνη διαλέγει τη θέση
    /// με τον Lcg. Αλλάζοντάς την θα άλλαζαν τα hashes των ΥΠΑΡΧΟΝΤΩΝ
    /// δοκιμίων, που είναι πύλη ντετερμινισμού. Εδώ η θέση είναι ΕΠΙΛΟΓΗ —
    /// το παράθυρο πρέπει να περιέχει παύση, αλλιώς δεν υπάρχει πού να
    /// κοιτάξει ο ανιχνευτής.
    fn decode_mono_at(path: &str, target_sr: u32, start_sec: f32, dur_sec: f32) -> (Vec<f32>, u32) {
        let file = Box::new(File::open(path).unwrap());
        let mss = MediaSourceStream::new(file, Default::default());
        let probed = symphonia::default::get_probe()
            .format(
                &Hint::new(),
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .unwrap();
        let mut format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .unwrap();
        let track_id = track.id;
        let sr = track.codec_params.sample_rate.unwrap();
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .unwrap();

        let mut mono = Vec::new();
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(_) => break,
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    let channels = decoded.spec().channels.count();
                    let mut sb = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
                    sb.copy_interleaved_ref(decoded.clone());
                    let s = sb.samples();
                    for i in (0..s.len()).step_by(channels) {
                        let mut sum = 0.0;
                        for c in 0..channels {
                            sum += s[i + c];
                        }
                        mono.push(sum / channels as f32);
                    }
                }
                Err(_) => break,
            }
        }
        let a = (start_sec * sr as f32) as usize;
        let b = ((start_sec + dur_sec) * sr as f32) as usize;
        assert!(b <= mono.len(), "το απόσπασμα ξεπερνά το αρχείο");
        let excerpt = mono[a..b].to_vec();
        if sr == target_sr {
            return (excerpt, sr);
        }
        // ⚠ ΤΟ ΠΛΕΓΜΑ ΚΑΔΩΝ ΠΡΕΠΕΙ ΝΑ ΤΑΥΤΙΖΕΤΑΙ ΜΕ ΤΟΝ ΑΝΙΧΝΕΥΤΗ. Εκείνος
        // δουλεύει πάντα στα 48 kHz (το dump της αλυσίδας). Στα 44.1 kHz ο
        // κάδος είναι 2.69 Hz αντί 2.93 — ο τόνος θα έπεφτε αλλού μέσα στον
        // κάδο και η απώλεια κλιμάκωσης του Hann θα διέφερε ως ~1.4 dB.
        // ΙΔΙΕΣ παράμετροι rubato με την `get_audio_excerpt`.
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler = SincFixedIn::<f32>::new(
            target_sr as f64 / sr as f64,
            2.0,
            params,
            excerpt.len(),
            1,
        )
        .unwrap();
        let waves_out = resampler.process(&vec![excerpt], None).unwrap();
        (waves_out[0].clone(), target_sr)
    }

    #[test]
    #[ignore = "ΓΕΝΝΗΤΡΙΑ fixtures (γράφει tests/fixtures/audiobook/), όχι φρουρός· ΚΑΙ δεν χτίζεται στο default build — θέλει --features cli. Δηλωμένη ΕΞΑΙΡΕΣΗ στο scripts/run-ignored.sh. Το ξυπνά: cargo test -p sp314-dsp --features cli --test fixture_factory -- --ignored"]
    fn build_fixtures() {
        let base_out = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audiobook");
        std::fs::create_dir_all(base_out).unwrap();

        let mut rng = Lcg::new(314159);
        let target_sr = 48000;

        let voices = [
            "/tmp/fixture-factory/voice1.mp3",
            "/tmp/fixture-factory/voice2.mp3",
            "/tmp/fixture-factory/voice3.mp3",
        ];
        let musics = [
            "/tmp/fixture-factory/music1.mp3",
            "/tmp/fixture-factory/music2.mp3",
            "/tmp/fixture-factory/music3.mp3",
        ];

        let mut manifest: HashMap<String, serde_json::Value> = HashMap::new();
        manifest.insert("seed".to_string(), serde_json::json!(314159));
        manifest.insert(
            "resampler".to_string(),
            serde_json::json!("rubato SincFixedIn"),
        );

        let mut outputs = HashMap::new();

        for i in 0..3 {
            let v_cut = get_audio_excerpt(voices[i], target_sr as u32, 25.0, &mut rng);
            let v_lufs = measure_integrated_lufs(&v_cut, &v_cut);

            let m_cut = get_audio_excerpt(musics[i], target_sr as u32, 25.0, &mut rng);
            let m_lufs = measure_integrated_lufs(&m_cut, &m_cut);

            let min_len = std::cmp::min(v_cut.len(), m_cut.len());
            let v_cut = &v_cut[..min_len];
            let m_cut = &m_cut[..min_len];

            let v_out_path = format!("{}/voice_{}.flac", base_out, i + 1);
            FlacWriter::write(&v_out_path, v_cut, v_cut, target_sr as u32).unwrap();
            outputs.insert(
                format!("voice_{}.flac", i + 1),
                serde_json::json!(hash_file(&v_out_path)),
            );

            let snrs = [-6.0, -15.0, -25.0];
            for &snr in &snrs {
                let target_m_lufs = v_lufs + snr;
                let m_gain = 10.0_f32.powf((target_m_lufs - m_lufs) / 20.0);

                let mut m_scaled = vec![0.0; min_len];
                let mut mix = vec![0.0; min_len];
                for j in 0..min_len {
                    m_scaled[j] = m_cut[j] * m_gain;
                    mix[j] = v_cut[j] + m_scaled[j];
                }

                for j in 0..min_len {
                    mix[j] = mix[j].clamp(-1.0, 1.0);
                    m_scaled[j] = m_scaled[j].clamp(-1.0, 1.0);
                }

                let mix_name = format!("mix_{}_snr{}.flac", i + 1, snr as i32);
                let mix_path = format!("{}/{}", base_out, mix_name);
                FlacWriter::write(&mix_path, &mix, &mix, target_sr as u32).unwrap();

                let mut mix_stats = serde_json::Map::new();
                mix_stats.insert("hash".to_string(), serde_json::json!(hash_file(&mix_path)));
                mix_stats.insert("voice_lufs".to_string(), serde_json::json!(v_lufs));
                mix_stats.insert("music_original_lufs".to_string(), serde_json::json!(m_lufs));
                mix_stats.insert(
                    "music_target_lufs".to_string(),
                    serde_json::json!(target_m_lufs),
                );
                mix_stats.insert("music_gain_linear".to_string(), serde_json::json!(m_gain));
                outputs.insert(mix_name.clone(), serde_json::Value::Object(mix_stats));

                let m_name = format!("music_{}_snr{}.flac", i + 1, snr as i32);
                let m_out_path = format!("{}/{}", base_out, m_name);
                FlacWriter::write(&m_out_path, &m_scaled, &m_scaled, target_sr as u32).unwrap();
                outputs.insert(m_name, serde_json::json!(hash_file(&m_out_path)));

                let vad_mix = evaluate_vad(&mix);
                println!("FACTORY|vad|file={}|speech_pct={:.2}", mix_name, vad_mix);
            }

            let vad_voice = evaluate_vad(v_cut);
            println!(
                "FACTORY|vad|file=voice_{}.flac|speech_pct={:.2}",
                i + 1,
                vad_voice
            );
        }

        manifest.insert(
            "hashes".to_string(),
            serde_json::to_value(&outputs).unwrap(),
        );
        let manifest_path = format!("{}/manifest.json", base_out);
        let mut manifest_file = File::create(&manifest_path).unwrap();
        manifest_file
            .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
            .unwrap();

        println!("Determinism gate hashes:");
        let mut keys: Vec<_> = outputs.keys().collect();
        keys.sort();
        for k in &keys {
            println!("{}: {}", k, outputs[*k]);
        }
    }

    // ── W2.0 Splice Fixture Generator (rev2) ───────────────────────────────
    // Builds two 30s FLACs with speech strictly in [10s, 20s].
    // Seed: 271828 (distinct from build_fixtures seed 314159).
    //
    // Outputs (tests/fixtures/duck_splice/):
    //   duck_splice_synth_snr-15.flac — synth noise bed + speech  [primary gate]
    //   duck_splice_real_snr-15.flac  — Skelpolu stem + speech    [sensor witness]
    //   speech_segment.flac           — speech-only, full 30s context
    //   synth_bed.flac                — scaled synth bed alone
    //   skelpolu_bed.flac             — scaled Skelpolu bed alone
    //   manifest.json                 — ground truth + 28-candidate bed scan
    #[test]
    #[ignore = "ΓΕΝΝΗΤΡΙΑ του duck_splice fixture (τα παραγόμενα απαριθμούνται στο σχόλιο από πάνω), όχι φρουρός· θέλει --features cli. Δηλωμένη ΕΞΑΙΡΕΣΗ στο scripts/run-ignored.sh. Το ξυπνά: cargo test -p sp314-dsp --features cli --test fixture_factory -- --ignored"]
    fn build_duck_splice_fixture() {
        let base_out = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/duck_splice");
        std::fs::create_dir_all(base_out).unwrap();

        let target_sr: u32 = 48000;
        let total_samples: usize = 1_440_000; // 30s @ 48kHz
        let speech_onset_sample: usize = 480_000; // 10s
        let speech_end_sample: usize = 960_000; // 20s
        let speech_n_samples: usize = speech_end_sample - speech_onset_sample; // 480000

        // ── 1. MUSDB stem scan (no gate — record all 28 candidates) ───────
        // First measured corpus of micro-VAD music false-positive rates.
        // PARK[RED]: micro-VAD has no speech-vs-music discrimination
        // (28/28 stems > 43% VAD, median ~97%). Sensor upgrade required.
        // W2 proceeds as hydraulic gate with synth fixture.
        let initial_tracks: &[&str] = &[
            "Buitraker_-_Revo_X",
            "Hollow_Ground_-_Ill_Fate",
            "MERC_Music_-_Knockout",
            "Moosmusic_-_Big_Dummy_Shake",
            "Timboz_-_Pony",
        ];
        let fallback_tracks: &[&str] = &[
            "Al_James_-_Schoolboy_Facination",
            "AM_Contra_-_Heart_Peripheral",
            "Arise_-_Run_Run_Run",
            "Ben_Carrigan_-_Well_Talk_About_It_All_Tonight",
            "Forkupines_-_Semantics",
            "Little_Chicagos_Finest_-_My_Own",
            "Louis_Cressy_Band_-_Good_Time",
            "Motor_Tapes_-_Shore",
            "PR_-_Oh_No",
            "Punkdisco_-_Oral_Hygiene",
            "Secretariat_-_Borderline",
            "Signe_Jakobsen_-_What_Have_You_Done_To_Me",
            "Skelpolu_-_Resurrection",
            "Speak_Softly_-_Like_Horses",
            "The_Mountaineering_Club_-_Mallory",
            "The_Sunshine_Garcia_Band_-_For_I_Am_The_Moon",
            "Tom_McKenzie_-_Directions",
            "Zeno_-_Signs",
        ];
        let base_dir = "/home/aidevcon/Downloads/DATASET/excerpts";
        let mut bed_scan_log: Vec<serde_json::Value> = Vec::new();

        for phase in 0..3_u8 {
            let tracks: &[&str] = if phase < 2 { initial_tracks } else { fallback_tracks };
            let stem_label = match phase {
                0 => "drums+bass",
                1 => "drums",
                _ => "other",
            };
            for &track in tracks {
                let primary_stem = if phase < 2 { "drums" } else { "other" };
                let path1 = format!("{}/{}/{}.wav", base_dir, track, primary_stem);
                let mut rng1 = Lcg::new(271828);
                let audio1 = get_audio_excerpt(&path1, target_sr, 30.0, &mut rng1);
                let audio = if phase == 0 {
                    let path2 = format!("{}/{}/bass.wav", base_dir, track);
                    let mut rng2 = Lcg::new(271828);
                    let audio2 = get_audio_excerpt(&path2, target_sr, 30.0, &mut rng2);
                    let len = audio1.len().min(audio2.len());
                    audio1[..len]
                        .iter()
                        .zip(audio2[..len].iter())
                        .map(|(a, b)| a + b)
                        .collect::<Vec<f32>>()
                } else {
                    audio1
                };
                let vad_pct = evaluate_vad(&audio);
                let passed = vad_pct < 5.0;
                println!(
                    "DUCK_SPLICE|BED|phase={}|stem={}|track={}|vad={:.2}|pass={}",
                    phase, stem_label, track, vad_pct, passed
                );
                bed_scan_log.push(serde_json::json!({
                    "phase": phase,
                    "stem": stem_label,
                    "track": track,
                    "vad_pct": vad_pct,
                    "pass": passed,
                }));
            }
        }

        // ── 2. Synth bed: bandlimited noise (1-pole IIR lowpass @ 400Hz) ──
        // y[n] = α·y[n-1] + (1-α)·x[n], α = exp(-2π·400/48000) ≈ 0.9481.
        // Seed: 271828 + 999999 (isolated from all other Lcg uses).
        // Hard assert VAD < 5%: cannot proceed without clean bed.
        let synth_fc_hz: f32 = 400.0;
        let synth_alpha =
            (-2.0_f32 * std::f32::consts::PI * synth_fc_hz / target_sr as f32).exp();
        let synth_k = 1.0_f32 - synth_alpha;
        let mut synth_rng = Lcg::new(271_828 + 999_999);
        let mut synth_lpf = 0.0_f32;
        let mut synth_bed: Vec<f32> = Vec::with_capacity(total_samples);
        for _ in 0..total_samples {
            let white = synth_rng.next_float() * 2.0_f32 - 1.0_f32;
            synth_lpf = synth_alpha * synth_lpf + synth_k * white;
            synth_bed.push(synth_lpf);
        }
        let synth_vad_pct = evaluate_vad(&synth_bed);
        println!(
            "DUCK_SPLICE|SYNTH_BED|fc={:.0}Hz|alpha={:.6}|vad={:.2}|pass={}",
            synth_fc_hz, synth_alpha, synth_vad_pct, synth_vad_pct < 5.0
        );
        assert!(
            synth_vad_pct < 5.0,
            "Synth bed failed VAD gate ({:.2}% >= 5%); lower fc_hz or adjust IIR",
            synth_vad_pct
        );

        // ── 3. Skelpolu witness bed (no gate — known-contaminated) ─────────
        // Best real MUSDB candidate from scan: ~7.67% (music false-positive).
        // W2.2 reports duck envelope for this fixture as informational only.
        let skelpolu_path = format!("{}/Skelpolu_-_Resurrection/other.wav", base_dir);
        let mut ske_rng = Lcg::new(271828);
        let mut skelpolu_bed =
            get_audio_excerpt(&skelpolu_path, target_sr, 30.0, &mut ske_rng);
        skelpolu_bed.truncate(total_samples);
        while skelpolu_bed.len() < total_samples {
            skelpolu_bed.push(0.0_f32);
        }
        let skelpolu_vad_pct = evaluate_vad(&skelpolu_bed);
        println!(
            "DUCK_SPLICE|WITNESS_BED|track=Skelpolu_-_Resurrection|vad={:.2}",
            skelpolu_vad_pct
        );

        // ── 4. Speech: 2-3 utterances, strictly [10s, 20s] ────────────────
        // Sources verified: all 3 files exist, each ≥32s @16kHz.
        // 422-122949-0013: 32.645s, 2902-9006-0015: 32.485s, 5338-24615-0002: 32.145s.
        let utterance_sources: &[(&str, &str)] = &[
            (
                "/home/aidevcon/Downloads/DATASET/speech/LibriSpeech/dev-clean/422/122949/422-122949-0013.flac",
                "422-122949-0013",
            ),
            (
                "/home/aidevcon/Downloads/DATASET/speech/LibriSpeech/dev-clean/2902/9006/2902-9006-0015.flac",
                "2902-9006-0015",
            ),
            (
                "/home/aidevcon/Downloads/DATASET/speech/LibriSpeech/dev-clean/5338/24615/5338-24615-0002.flac",
                "5338-24615-0002",
            ),
        ];
        let mut speech_concat: Vec<f32> = Vec::with_capacity(speech_n_samples);
        let mut utterance_manifest: Vec<serde_json::Value> = Vec::new();
        for (utt_idx, &(path, id)) in utterance_sources.iter().enumerate() {
            if speech_concat.len() >= speech_n_samples {
                break;
            }
            let remaining = speech_n_samples - speech_concat.len();
            let request_sec = (remaining as f32 / target_sr as f32 + 1.0).min(11.0_f32);
            let mut utt_rng = Lcg::new(271828 + utt_idx as u64 * 1_000_003);
            let utt = get_audio_excerpt(path, target_sr, request_sec, &mut utt_rng);
            let rel_start = speech_concat.len();
            let rel_end = (rel_start + utt.len()).min(speech_n_samples);
            utterance_manifest.push(serde_json::json!({
                "id": id,
                "path": path,
                "speech_relative_start_sample": rel_start,
                "speech_relative_end_sample": rel_end,
                "mix_start_sample": speech_onset_sample + rel_start,
                "mix_end_sample":   speech_onset_sample + rel_end,
                "samples_used": rel_end - rel_start,
            }));
            speech_concat.extend_from_slice(&utt[..utt.len().min(remaining)]);
        }
        speech_concat.truncate(speech_n_samples);
        while speech_concat.len() < speech_n_samples {
            speech_concat.push(0.0_f32);
        }
        // Assert: actual speech material ≥ 9.5s (= 456000 samples @ 48kHz).
        // Prevents fixture with a half-empty speech window from resampler underflow.
        let speech_actual_samples: usize = utterance_manifest
            .iter()
            .map(|u| u["samples_used"].as_u64().unwrap_or(0) as usize)
            .sum();
        assert!(
            speech_actual_samples >= (9.5 * target_sr as f32) as usize,
            "Speech window underfilled: {} samples < 9.5s ({} samples)",
            speech_actual_samples,
            (9.5 * target_sr as f32) as usize,
        );
        println!(
            "DUCK_SPLICE|SPEECH_SANITY|actual_samples={}|min_required={}|ok=true",
            speech_actual_samples,
            (9.5 * target_sr as f32) as usize,
        );

        // ── 5. Level scaling ──────────────────────────────────────────────
        // All paths are dual-mono (L==R). BS.1770 stereo factor cancels:
        // 10·log10(L²+R²) = 10·log10(2·L²) — same offset on all channels.

        // Speech → -20 LUFS
        let speech_lufs_raw = measure_integrated_lufs(&speech_concat, &speech_concat);
        let speech_target_lufs = -20.0_f32;
        let speech_gain = 10.0_f32.powf((speech_target_lufs - speech_lufs_raw) / 20.0);
        let speech_scaled: Vec<f32> =
            speech_concat.iter().map(|s| s * speech_gain).collect();

        let bed_target_lufs = speech_target_lufs - 15.0_f32; // -35 LUFS

        let synth_lufs_raw = measure_integrated_lufs(&synth_bed, &synth_bed);
        let synth_gain = 10.0_f32.powf((bed_target_lufs - synth_lufs_raw) / 20.0);
        let synth_scaled: Vec<f32> =
            synth_bed.iter().map(|s| s * synth_gain).collect();

        let ske_lufs_raw = measure_integrated_lufs(&skelpolu_bed, &skelpolu_bed);
        let ske_gain = 10.0_f32.powf((bed_target_lufs - ske_lufs_raw) / 20.0);
        let ske_scaled: Vec<f32> =
            skelpolu_bed.iter().map(|s| s * ske_gain).collect();

        // Speech-in-context (zero outside [10s,20s], shared by both fixtures)
        let mut speech_ctx = vec![0.0_f32; total_samples];
        for (i, &s) in speech_scaled.iter().enumerate() {
            speech_ctx[speech_onset_sample + i] = s;
        }

        // ── 6. Two mixes ──────────────────────────────────────────────────
        // Mix A: synth bed + speech  (primary gate fixture)
        let mut mix_synth = synth_scaled.clone();
        for (i, &s) in speech_scaled.iter().enumerate() {
            mix_synth[speech_onset_sample + i] += s;
        }
        for s in mix_synth.iter_mut() {
            *s = s.clamp(-1.0_f32, 1.0_f32);
        }

        // Mix B: Skelpolu + speech   (sensor witness fixture)
        let mut mix_real = ske_scaled.clone();
        for (i, &s) in speech_scaled.iter().enumerate() {
            mix_real[speech_onset_sample + i] += s;
        }
        for s in mix_real.iter_mut() {
            *s = s.clamp(-1.0_f32, 1.0_f32);
        }

        // ── 7. Write FLAC files ───────────────────────────────────────────
        let mix_synth_path  = format!("{}/duck_splice_synth_snr-15.flac", base_out);
        let mix_real_path   = format!("{}/duck_splice_real_snr-15.flac",  base_out);
        let speech_ctx_path = format!("{}/speech_segment.flac",           base_out);
        let synth_bed_path  = format!("{}/synth_bed.flac",                base_out);
        let ske_bed_path    = format!("{}/skelpolu_bed.flac",              base_out);

        FlacWriter::write(&mix_synth_path,  &mix_synth,    &mix_synth,    target_sr).unwrap();
        FlacWriter::write(&mix_real_path,   &mix_real,     &mix_real,     target_sr).unwrap();
        FlacWriter::write(&speech_ctx_path, &speech_ctx,   &speech_ctx,   target_sr).unwrap();
        FlacWriter::write(&synth_bed_path,  &synth_scaled, &synth_scaled, target_sr).unwrap();
        FlacWriter::write(&ske_bed_path,    &ske_scaled,   &ske_scaled,   target_sr).unwrap();

        // ── 8. Manifest ───────────────────────────────────────────────────
        let mix_synth_sha = hash_file(&mix_synth_path);
        let mix_real_sha  = hash_file(&mix_real_path);
        let speech_sha    = hash_file(&speech_ctx_path);
        let synth_bed_sha = hash_file(&synth_bed_path);
        let ske_bed_sha   = hash_file(&ske_bed_path);

        let manifest = serde_json::json!({
            "version": "W2.0-rev2",
            "seed": 271828_u64,
            "resampler": "rubato SincFixedIn",
            "target_sr": target_sr,
            "total_samples": total_samples,
            "total_duration_sec": 30.0_f32,
            "speech_onset_sample": speech_onset_sample,
            "speech_end_sample":   speech_end_sample,
            "speech_n_samples":    speech_n_samples,
            "speech_onset_sec": 10.0_f32,
            "speech_end_sec":   20.0_f32,
            "snr_db": -15.0_f32,
            "speech_minus_bed_lu": 15.0_f32,

            "synth_fixture": {
                "role": "primary_gate",
                "note": "W2.2 pass/fail fixture: duck iff [10s,20s]",
                "file": "duck_splice_synth_snr-15.flac",
                "bed_file": "synth_bed.flac",
                "bed": {
                    "type": "synthetic_lowpass_noise",
                    "seed": 271_828_u64 + 999_999_u64,
                    "lpf_fc_hz": synth_fc_hz,
                    "lpf_alpha": synth_alpha,
                    "vad_pct": synth_vad_pct,
                    "gate_passed": synth_vad_pct < 5.0,
                    "original_lufs": synth_lufs_raw,
                    "target_lufs": bed_target_lufs,
                    "gain_linear": synth_gain,
                },
            },

            "real_fixture": {
                "role": "sensor_witness",
                "note": "Pre/post VAD-upgrade benchmark. W2.2 reports duck as informational only.",
                "file": "duck_splice_real_snr-15.flac",
                "bed_file": "skelpolu_bed.flac",
                "bed": {
                    "type": "musdb_stem",
                    "track": "Skelpolu_-_Resurrection",
                    "stem": "other",
                    "vad_pct": skelpolu_vad_pct,
                    "gate_passed": false,
                    "known_contamination": "music false-positive, not speech",
                    "original_lufs": ske_lufs_raw,
                    "target_lufs": bed_target_lufs,
                    "gain_linear": ske_gain,
                },
            },

            "speech": {
                "file": "speech_segment.flac",
                "original_lufs": speech_lufs_raw,
                "target_lufs": speech_target_lufs,
                "gain_linear": speech_gain,
                "utterances": utterance_manifest,
            },

            "bed_scan": {
                "note": "First measured corpus of micro-VAD music FP rates. PARK[RED]: upgrade required.",
                "gate_threshold_pct": 5.0,
                "total_candidates": bed_scan_log.len(),
                "candidates_tried": bed_scan_log,
            },

            "hashes": {
                "duck_splice_synth_snr-15.flac": &mix_synth_sha,
                "duck_splice_real_snr-15.flac":  &mix_real_sha,
                "speech_segment.flac":           &speech_sha,
                "synth_bed.flac":                &synth_bed_sha,
                "skelpolu_bed.flac":             &ske_bed_sha,
            }
        });

        let manifest_path = format!("{}/manifest.json", base_out);
        let mut mf = File::create(&manifest_path).unwrap();
        mf.write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
            .unwrap();

        // ── 9. Console summary ────────────────────────────────────────────
        println!(
            "DUCK_SPLICE|speech_lufs_raw={:.2}|target={:.2}|gain={:.6}",
            speech_lufs_raw, speech_target_lufs, speech_gain
        );
        println!(
            "DUCK_SPLICE|speech_onset_sample={}|speech_end_sample={}",
            speech_onset_sample, speech_end_sample
        );
        println!("DUCK_SPLICE|synth_mix_sha={}", mix_synth_sha);
        println!("DUCK_SPLICE|real_mix_sha={}",  mix_real_sha);
        println!("DUCK_SPLICE|speech_sha={}",    speech_sha);
        println!("DUCK_SPLICE|manifest written to {}/manifest.json", base_out);
    }

    // ── ΤΑ ΔΟΚΙΜΙΑ ────────────────────────────────────────────────────────
    //
    // ΦΟΡΕΑΣ: mobydick_000_melville, 135–165 s. ΔΙΑΛΕΧΤΗΚΕ ΜΕ ΜΕΤΡΗΣΗ, ΟΧΙ
    // ΜΕ ΤΟ ΜΑΤΙ: σαρώθηκαν και τα 20 αρχεία του corpus (9 WAKING 14/09,
    // 11 SLEEPING 15/09) με το hum_spectrum. ΥΠΟΛΕΙΜΜΑ ΤΟΥ ΦΟΡΕΑ, μετρημένο
    // σε παύση 3.20 s: στα 50 Hz −0.09 dB πάνω από το τοπικό πάτωμα, στα
    // 60 Hz +0.98 dB. ΚΑΜΙΑ ΓΡΑΜΜΗ — η μεγαλύτερη προεξοχή σε ΟΛΟ το
    // 20–400 Hz είναι 3.24 dB, στα 216.80 Hz.
    // Το παράθυρο 135–165 s περιέχει την παύση 145.6–148.8 s.
    //
    // ΟΙ ΤΡΕΙΣ ΠΡΟΕΞΟΧΕΣ, ΚΑΙ ΓΙΑΤΙ ΑΥΤΕΣ:
    //   ΜΕΤΡΗΜΕΝΟ ΕΥΡΟΣ ΤΩΝ ΠΡΑΓΜΑΤΙΚΩΝ (20 αρχεία): 2.5 έως 22.4 dB.
    //   Το κατώφλι ανίχνευσης του F-082 είναι +8 dB.
    //    4 dB — ΚΑΤΩ από το κατώφλι. Ανιχνευτής πρέπει να πει ΟΧΙ.
    //           Μέσα στο πραγματικό εύρος: prideandprejudice 5.58,
    //           adventuresholmes < 3.57.
    //   12 dB — ΠΑΝΩ από το κατώφλι, με περιθώριο. Πρέπει να πει ΝΑΙ.
    //           robinson_crusoe 14.39, monte_cristo 15.15.
    //   21 dB — Η ΚΟΡΥΦΗ του πραγματικού εύρους: pinocchio 21.21,
    //           odyssey 22.36.
    //   ⚠ ΚΑΜΙΑ ΑΚΡΙΒΩΣ ΣΤΟ +8. Δοκίμιο πάνω στο κατώφλι κάνει τον έλεγχο
    //     ρίψη νομίσματος και το τεστ ασταθές. Το κατώφλι ΦΡΑΣΣΕΤΑΙ
    //     ΕΚΑΤΕΡΩΘΕΝ, δεν πατιέται.
    //
    // ΤΟ ΑΡΝΗΤΙΚΟ ΕΙΝΑΙ ΥΠΟΧΡΕΩΤΙΚΟ: φορέας σκέτος, τίποτα προστιθέμενο.
    // Ανιχνευτής που λέει πάντα ναι δεν είναι ανιχνευτής.
    //
    // ΕΞΟΔΟΣ (tests/fixtures/hum_detector/):
    //   hum_50hz_p04.flac · hum_50hz_p12.flac · hum_50hz_p21.flac
    //   hum_60hz_p04.flac · hum_60hz_p12.flac · hum_60hz_p21.flac
    //   hum_none.flac                                    [ΑΡΝΗΤΙΚΟ]
    //   manifest.json — ground truth ανά δοκίμιο
    #[test]
    #[ignore = "ΓΕΝΝΗΤΡΙΑ fixtures (γράφει tests/fixtures/hum_detector/), όχι φρουρός· ΚΑΙ δεν χτίζεται στο default build — θέλει --features cli. Καλύπτεται από την ΥΠΑΡΧΟΥΣΑ εξαίρεση 'fixture_factory' στο scripts/run-ignored.sh. Το ξυπνά: cargo test -p sp314-dsp --features cli --test fixture_factory -- --ignored"]
    fn build_hum_detector_fixtures() {
        let base_out = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hum_detector");
        std::fs::create_dir_all(base_out).unwrap();

        const CARRIER: &str = "/tmp/fixture-factory/mobydick_000_melville.mp3";
        const START_SEC: f32 = 135.0;
        const DUR_SEC: f32 = 30.0;
        const HARMONICS: usize = 1;
        const HARMONIC_DECAY_DB: f32 = 0.0; // αδρανές με harmonics = 1

        const TARGET_SR: u32 = 48_000;
        let (carrier, sr) = decode_mono_at(CARRIER, TARGET_SR, START_SEC, DUR_SEC);
        println!("φορέας: {CARRIER} @ {START_SEC}s +{DUR_SEC}s · {sr} Hz · {} δείγματα", carrier.len());

        let mut manifest: HashMap<String, serde_json::Value> = HashMap::new();
        manifest.insert("carrier".into(), serde_json::json!(CARRIER));
        manifest.insert("carrier_start_sec".into(), serde_json::json!(START_SEC));
        manifest.insert("carrier_duration_sec".into(), serde_json::json!(DUR_SEC));
        manifest.insert("sample_rate".into(), serde_json::json!(sr));
        manifest.insert("seed".into(), serde_json::json!(314159));
        manifest.insert(
            "carrier_residual_db".into(),
            serde_json::json!({ "50hz": -0.09, "60hz": 0.98, "measured": "2026-09-15, hum_spectrum, pause 145.6-148.8s" }),
        );
        manifest.insert("harmonics".into(), serde_json::json!(HARMONICS));
        manifest.insert(
            "limits".into(),
            serde_json::json!({
                "waveform": "pure sine, fundamental only — the 60->120 Hz decay of real mains measured 3.2..33.2 dB across n=4 files; unstable, so no ratio was invented. A detector that relies on harmonics is NOT exercised here.",
                "carrier": "mobydick_000_melville — residual -0.09 dB at 50 Hz and +0.98 dB at 60 Hz above the local median; largest prominence anywhere in 20-400 Hz is 3.24 dB at 216.8 Hz",
                "prominence_definition": "peak minus median of +/-25 Hz excluding +/-4 Hz, Welch N=16384 Hann 50%, measured INSIDE the longest pause (100 ms windows below the file's own Otsu split)"
            }),
        );

        let mut entries: Vec<serde_json::Value> = Vec::new();

        for &f_hz in &[50.0_f32, 60.0_f32] {
            for &prom in &[4.0_f32, 12.0_f32, 21.0_f32] {
                let amp = amplitude_for_prominence(&carrier, sr, f_hz, prom);
                let hum = mains_hum(sr, DUR_SEC, f_hz, HARMONICS, HARMONIC_DECAY_DB, amp);
                let mix: Vec<f32> = carrier
                    .iter()
                    .zip(hum.iter())
                    .map(|(c, h)| c + h)
                    .collect();

                let name = format!("hum_{}hz_p{:02}.flac", f_hz as i32, prom as i32);
                let path = format!("{base_out}/{name}");
                FlacWriter::write(&path, &mix, &mix, sr).unwrap();

                // ΤΟ MANIFEST ΓΡΑΦΕΤΑΙ ΑΠΟ ΤΟ ΜΕΤΡΗΜΕΝΟ, ΟΧΙ ΑΠΟ ΤΟ
                // ΖΗΤΟΥΜΕΝΟ. Το ζητούμενο μπαίνει δίπλα, για να φαίνεται
                // η απόκλιση αντί να κρύβεται.
                let meas = measured_prominence(&mix, sr, f_hz);
                let other_hz = if f_hz == 50.0 { 60.0 } else { 50.0 };
                let meas_other = measured_prominence(&mix, sr, other_hz);
                let amp_dbfs = 20.0 * amp.log10();
                println!(
                    "  {name}: ζητ {prom:.1} → ΜΕΤΡ {meas:.2} dB (Δ {:+.2}) · στα {other_hz:.0} Hz: {meas_other:+.2} dB · πλάτος {amp_dbfs:.2} dBFS",
                    meas - prom
                );
                entries.push(serde_json::json!({
                    "file": name,
                    "sha256": hash_file(&path),
                    "hum_hz": f_hz,
                    "measured_prominence_db": meas,
                    "requested_prominence_db": prom,
                    "measured_prominence_at_other_hz_db": meas_other,
                    "other_hz": other_hz,
                    "tone_amplitude_dbfs": amp_dbfs,
                    "harmonics": HARMONICS,
                    "carrier": CARRIER,
                    "positive": true,
                }));
            }
        }

        // ΤΟ ΑΡΝΗΤΙΚΟ — ο φορέας ΑΥΤΟΥΣΙΟΣ, μηδέν προστιθέμενο.
        let none_path = format!("{base_out}/hum_none.flac");
        FlacWriter::write(&none_path, &carrier, &carrier, sr).unwrap();
        let none_50 = measured_prominence(&carrier, sr, 50.0);
        let none_60 = measured_prominence(&carrier, sr, 60.0);
        println!("  hum_none.flac: στα 50 Hz {none_50:+.2} dB · στα 60 Hz {none_60:+.2} dB");
        entries.push(serde_json::json!({
            "file": "hum_none.flac",
            "sha256": hash_file(&none_path),
            "hum_hz": serde_json::Value::Null,
            "requested_prominence_db": serde_json::Value::Null,
            "measured_prominence_at_50hz_db": none_50,
            "measured_prominence_at_60hz_db": none_60,
            "tone_amplitude_dbfs": serde_json::Value::Null,
            "harmonics": 0,
            "carrier": CARRIER,
            "positive": false,
        }));

        manifest.insert("fixtures".into(), serde_json::Value::Array(entries));
        let manifest_path = format!("{base_out}/manifest.json");
        let mut manifest_file = File::create(&manifest_path).unwrap();
        manifest_file
            .write_all(serde_json::to_string_pretty(&manifest).unwrap().as_bytes())
            .unwrap();
        println!("manifest: {manifest_path}");
    }
}
