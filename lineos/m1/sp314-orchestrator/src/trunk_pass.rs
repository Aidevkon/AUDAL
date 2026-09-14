//! Y1 Trunk Pass — single-pass segmentation + metering over the raw PCM dump.
//!
//! Replaces the old two-step sequence of
//! `build_timeline_map` (WholeBufferProvider → full-file RAM decode → scan_file)
//! and separate metering passes with ONE chunked read of the dump file
//! (written by P0-a), producing boundaries + LUFS + crest + LRA + noise
//! floor + spectral profile + transient density in a single linear scan.
//!
//! STRANGLER FIG: The measurement logic is IDENTICAL to scan_file's —
//! same SegmentScout instance, same 5s windows, same 1s hops, same
//! sequential order, same partial-window drop. Only the I/O layer changed
//! (dump-backed ChunkSource instead of full-file slice).

use lineos_corpus::scout::{compute_scout_decision, smooth_and_segment, SegmentBoundary};
use sp314_dsp::analysis::dynamics::StreamingDynamicsAnalyzer;
use sp314_dsp::analysis::pre_analysis::{butter_hp2, butter_lp2, Biquad, BAND_EDGES};
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_dsp::metering::lra::StreamingLraMeter;
use sp314_dsp::metering::LufsMeter;
use sp314_dsp::stft::sliding_overlap_reader::ChunkSource;
use std::path::Path;

/// Metrics produced by the trunk pass.
pub struct TrunkMetrics {
    pub integrated_lufs: Option<f32>,
    /// Whole-file RMS in dBFS, unweighted — plain average level, not LUFS.
    /// Needed for ACX's RMS-window requirement (-23..-18 dB), which is a
    /// different measurement than LUFS: LUFS is K-weighted and gates out
    /// silence, RMS is not. Do not substitute one for the other.
    pub rms_db: f32,
    /// 5th percentile of 50ms-block RMS across the whole file, from the same
    /// StreamingDynamicsAnalyzer pass. NOT gated to non-speech regions — it is
    /// the quietest-5%-of-blocks number, not "the room tone between phrases".
    /// In continuous speech with few pauses this may read as the softest
    /// syllable rather than the ambient floor ACX means. Unverified as an ACX
    /// noise-floor proxy; do not surface it to a user as a pass/fail number
    /// without checking it against real accepted/rejected ACX files first.
    ///
    /// ⚠ ΔΙΟΡΘΩΣΗ 2026-08-25: Field name is misleading. NOT the AcxCheckAnalyzer
    /// algorithm (which uses LS window after HP8 Butterworth @10Hz, step 100ms).
    /// This IS p5 block RMS. Name kept to avoid breaking callers; the calculation
    /// is what matters. Also: this metric is computed but DISCARDED in to_pre_analysis().
    pub acx_noise_floor_proxy_db: f32,
    pub crest_db: f32,
    pub lra: f32,
    /// Το πιο ήσυχο παράθυρο 1 s ΠΟΥ ΔΕΝ ΕΙΝΑΙ σιωπή — ελάχιστο RMS πάνω από
    /// το DEAD_AIR_GATE_DBFS, ΧΩΡΙΣ φιλτράρισμα. ΔΕΝ είναι πάτωμα θορύβου:
    /// σε καθαρή αφήγηση αυτό που μετριέται είναι η ΠΙΟ ΗΣΥΧΗ ΟΜΙΛΙΑ.
    /// Το πραγματικό πάτωμα είναι το AcxCheckReport::noise_floor_db
    /// (HP8 @10 Hz, ελάχιστο κυλιόμενο 500 ms) — άλλο μέγεθος [F-097].
    /// ΗΤΑΝ `noise_floor_dbfs`· η ομωνυμία γέννησε το F-096.
    pub quietest_active_window_dbfs: Option<f32>,
    pub spectral_profile_db: [f32; 8],
    pub transient_density: f32,
    /// Zero-crossing rate mean across 1024-sample frames (hop 512) on the 48kHz mono downmix.
    /// ZCR is normalized by (frame_size - 1), i.e., counts / 1023.0.
    pub zcr_mean: f32,
    /// Zero-crossing rate standard deviation across the same 1024-sample frames.
    pub zcr_std: f32,
    /// BPM estimate derived from 100Hz onset envelope autocorrelation (40-200 BPM range).
    pub bpm_estimate: f32,
    /// Confidence of the BPM estimate (0.0 to 1.0), derived from normalized autocorrelation peak.
    pub bpm_confidence: f32,
    pub global_phase_correlation: f32,
    /// 95th-5th percentile RMS block spread, 50ms blocks (the real broadcast DR metric)
    pub dynamic_range_db: f32,
    /// Present only when the caller asked for the ACX delivery check
    /// (run_trunk_metrics_with_acx). None means "not measured", never
    /// "passed" — consumers must not treat absence as compliance.
    pub acx: Option<sp314_dsp::analysis::acx_check::AcxCheckReport>,
    pub cv_ioi_sequence: Vec<f32>,
    pub cepstral_flux_sequence: Vec<f32>,
    /// VAD ratio: percentage of frames where posterior > threshold
    pub voice_ratio: Option<f32>,
    pub voice_posterior_mean: Option<f32>,
    pub voice_posterior_std: Option<f32>,
    /// Longest continuous speech segment, using the Ducker VAD hysteresis (s)
    pub voice_longest_run_s: Option<f32>,
    /// Mean of the 13 MFCC coefficients across all 1024-frame windows
    pub mfcc_means: [f32; 13],
    /// Standard deviation of the 13 MFCC coefficients across all 1024-frame windows
    pub mfcc_stds: [f32; 13],
}

pub struct TrunkReport {
    pub boundaries: Vec<SegmentBoundary>,
    pub metrics: TrunkMetrics,
    /// (floor_db, start_frame) του interior ελαχίστου. None αν:
    ///   · ο analyzer δεν έτρεξε (with_acx = false)
    ///   · δεν δόθηκε edge_sec
    ///   · το αρχείο είναι πολύ κοντό για interior
    ///     (το ίδιο MIN_SUB_BLOCKS του analyzer, όχι νέο όριο)
    /// None σημαίνει «δεν μετρήθηκε» — ίδια σημασιολογία με το
    /// max_noise_floor_db, ΜΗΔΕΝ default.
    ///
    /// Ίδιο μοτίβο με το `acx_noise_floor_proxy_db` (:693 → :816): μετριέται
    /// στο ίδιο ενιαίο πέρασμα και ταξιδεύει μαζί με τα υπόλοιπα. ΔΙΑΦΟΡΑ:
    /// εκείνο πετιέται στο `to_pre_analysis`· αυτό ζει στο TrunkReport, που
    /// ΔΕΝ σειριοποιείται και δεν φτάνει σε cert.
    pub acx_interior_noise_floor: Option<(f32, usize)>,
    /// Η τομή Otsu της κατανομής RMS/100 ms ΤΟΥ ΙΔΙΟΥ ΤΟΥ ΑΡΧΕΙΟΥ, σε dBFS.
    ///
    /// None σημαίνει «η κατανομή δεν είναι διμερής» — δεν υπάρχει δωμάτιο
    /// ξεχωριστό από τη φωνή, άρα δεν υπάρχει σημείο να μπει κατώφλι. ΜΗΔΕΝ
    /// default, ΜΗΔΕΝ fallback: ο expander απλώς δεν τρέχει.
    ///
    /// Μετριέται στο ΙΔΙΟ πέρασμα με όλα τα υπόλοιπα (ιστόγραμμα στον βρόχο
    /// του mono), όχι σε δεύτερο.
    pub quiet_window_split_dbfs: Option<f32>,
}

impl std::ops::Deref for TrunkReport {
    type Target = TrunkMetrics;
    fn deref(&self) -> &Self::Target {
        &self.metrics
    }
}

impl TrunkMetrics {
    /// Constructs PreAnalysisData from TrunkMetrics, closing the drift
    /// between the executor site and the dsp_pipeline sites (where the
    /// executor site previously hardcoded correlation to 1.0).
    pub fn to_pre_analysis(
        &self,
        true_peak_dbtp: f32,
        bpm: f32,
        beats_ms: Vec<u32>,
        downbeats_ms: Vec<u32>,
        transients_ms: Vec<u32>,
    ) -> lineos_types::pre_analysis::PreAnalysisData {
        let zone_flags = sp314_dsp::analysis::pre_analysis::compute_zone_flags(
            &self.spectral_profile_db,
            self.crest_db,
            self.lra,
            self.global_phase_correlation,
            &[], // resonant peaks: unmeasured — empty = no resonance
        );

        lineos_types::pre_analysis::PreAnalysisData {
            integrated_lufs: self.integrated_lufs.unwrap_or(-144.0),
            true_peak_dbtp,
            loudness_range: self.lra,
            dynamic_range_db: self.dynamic_range_db,
            global_crest_factor_db: self.crest_db,
            spectral_profile_db: self.spectral_profile_db,
            transient_density: self.transient_density,
            global_phase_correlation: self.global_phase_correlation,
            zone_flags,
            bpm,
            beats_ms,
            downbeats_ms,
            transients_ms,
        }
    }
}

// Mirror scan_file's constants exactly [scout_scanner.rs:6-7].
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SAMPLE_RATE: u32 = 48_000;
const CHUNK_FRAMES: usize = 4096;

// Mirror signal_health.rs:92 — windows below this are dead air.
const DEAD_AIR_GATE_DBFS: f32 = -60.0;
const NOISE_FLOOR_WINDOW: usize = 48_000; // 1s at 48kHz

// ── Κατανομή ήσυχων παραθύρων ────────────────────────────────────────────────
// Ιστόγραμμα RMS ανά 100 ms, κάδοι του 1 dB. Ίδιο παράθυρο και ίδιοι κάδοι με
// το harness που μέτρησε τη διμερή κατανομή στις 14/09 — ό,τι αλλάξει εδώ
// παύει να είναι συγκρίσιμο με εκείνη τη μέτρηση.
// MEASURED: n=9 2026-09-14 — 8/9 αφηγήσεις διμερείς, λόγος κοιλάδας 0.058–0.324.
const QW_WINDOW: usize = 4_800; // 100 ms at 48kHz
const QW_LO_DB: i32 = -100;
const QW_HI_DB: i32 = 0;
const QW_NBINS: usize = (QW_HI_DB - QW_LO_DB) as usize;

/// Otsu: η τομή που μεγιστοποιεί τη διακύμανση ΑΝΑΜΕΣΑ στις δύο κλάσεις ενός
/// 1-D ιστογράμματος. Επιστρέφει τον δείκτη κάδου της τομής.
///
/// SOURCE: N. Otsu, "A Threshold Selection Method from Gray-Level Histograms",
///   IEEE Transactions on Systems, Man and Cybernetics, vol. 9, pp. 62-66, 1979,
///   DOI 10.1109/TSMC.1979.4310076
/// RETRIEVED: 2026-09-14 (Semantic Scholar Graph API, εγγραφή DOI)
///
/// ⚠ ΟΡΙΑ ΤΟΥ ΑΛΓΟΡΙΘΜΟΥ — ΔΙΑΒΑΣΕ ΠΡΙΝ ΤΟΝ ΕΜΠΙΣΤΕΥΤΕΙΣ:
/// ΥΠΟΘΕΤΕΙ δύο πληθυσμούς. ΔΕΝ μπορεί να ανιχνεύσει ότι υπάρχει ένας. Σε
/// μονοκόρυφη κατανομή επιστρέφει πάντα μια τομή — έγκυρη αριθμητικά, χωρίς
/// νόημα φυσικά. Ο ΕΛΕΓΧΟΣ ΔΙΜΕΡΕΙΑΣ ΕΙΝΑΙ ΕΥΘΥΝΗ ΤΟΥ ΚΑΛΟΥΝΤΟΣ.
/// Μετρημένο παράδειγμα: janeeyre_01_bronte ανεβαίνει μονότονα από -67 ως -40
/// dB· ο Otsu έδωσε -38.5 και ο λόγος κοιλάδας βγήκε 1.0000 [14/09].
fn otsu_split_bin(hist: &[u32; QW_NBINS]) -> Option<usize> {
    let total: f64 = hist.iter().map(|&c| c as f64).sum();
    if total == 0.0 {
        return None;
    }
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut w0, mut sum0) = (0.0_f64, 0.0_f64);
    let (mut best_var, mut best_t) = (-1.0_f64, 0usize);
    for t in 0..QW_NBINS {
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

/// Ο ΚΑΛΩΝ: τρέχει τον Otsu ΚΑΙ ελέγχει ότι η κατανομή δικαιολογεί το
/// αποτέλεσμά του. Επιστρέφει το dBFS της τομής ΜΟΝΟ αν υπάρχουν δύο κορυφές
/// εκατέρωθεν και κοιλάδα ανάμεσά τους.
///
/// ΤΟ ΚΡΙΤΗΡΙΟ ΕΙΝΑΙ ΤΟΠΟΛΟΓΙΚΟ, ΟΧΙ ΜΕΓΕΘΟΥΣ: αρκεί να υπάρχει κορυφή κάτω
/// από την τομή, κορυφή πάνω, και ένας κάδος ανάμεσα γνησίως χαμηλότερος και
/// από τις δύο. ΚΑΝΕΝΑ κατώφλι στον λόγο κοιλάδας — ο λόγος μετρήθηκε
/// (0.058–0.324 στα οκτώ, 1.0000 στο ένα) αλλά ΔΕΝ κρίνει εδώ.
///
/// None σημαίνει «η κατανομή δεν λέει πού είναι το δωμάτιο». ΜΗΔΕΝ fallback:
/// ο καλών δεν βάζει άλλο νούμερο στη θέση του.
fn quiet_window_split_db(hist: &[u32; QW_NBINS]) -> Option<f32> {
    let t = otsu_split_bin(hist)?;
    if t == 0 || t >= QW_NBINS {
        return None;
    }
    // Κορυφή = ο πιο γεμάτος κάδος σε κάθε πλευρά της τομής.
    let lo = (0..t).max_by_key(|&i| hist[i])?;
    let hi = (t..QW_NBINS).max_by_key(|&i| hist[i])?;
    if hist[lo] == 0 || hist[hi] == 0 || lo >= hi {
        return None;
    }
    // Κοιλάδα = ο πιο άδειος κάδος ΑΝΑΜΕΣΑ στις δύο κορυφές. Πρέπει να είναι
    // γνησίως χαμηλότερος και από τις δύο, αλλιώς δεν υπάρχει διαχωρισμός.
    let valley = (lo..=hi).map(|i| hist[i]).min()?;
    if valley >= hist[lo] || valley >= hist[hi] {
        return None;
    }
    Some(QW_LO_DB as f32 + t as f32 + 0.5)
}

// ── Transient density constants ──────────────────────────────────────────────
// Mirror compute_transient_density [pre_analysis.rs:548-550] exactly:
//   fast_win = 0.010 * sr = 480 samples (10ms at 48kHz)
//   slow_win = 0.100 * sr = 4800 samples (100ms at 48kHz)
//   threshold_linear = 10^(6/20) ≈ 1.9953 (+6dB)
const TD_FAST_WIN: usize = 480;
const TD_SLOW_WIN: usize = 4800;

/// 4th-order Butterworth bandpass filter (2×HP + 2×LP cascade).
/// Same topology as `bandpass_filter` in pre_analysis.rs:399-413.
struct BandpassFilter {
    hp1: Biquad,
    hp2: Biquad,
    lp1: Biquad,
    lp2: Biquad,
}

impl BandpassFilter {
    fn new(lo: f32, hi: f32, sr: f32) -> Self {
        Self {
            hp1: butter_hp2(lo, sr),
            hp2: butter_hp2(lo, sr),
            lp1: butter_lp2(hi, sr),
            lp2: butter_lp2(hi, sr),
        }
    }

    /// Process one sample through the 4th-order bandpass cascade.
    /// Mirrors bandpass_filter's per-sample chain exactly:
    ///   y = hp1(x) → hp2 → lp1 → lp2  [pre_analysis.rs:407-410]
    fn process(&mut self, x: f32) -> f32 {
        let y = self.hp1.process(x);
        let y = self.hp2.process(y);
        let y = self.lp1.process(y);
        self.lp2.process(y)
    }
}

/// Streaming dual-moving-average transient detector.
/// Mirrors `compute_transient_density` [pre_analysis.rs:543-585] line-for-line:
///   - Rectified signal: |mono[i]|
///   - fast MA: 10ms (480 samples) running average of rectified signal
///   - slow MA: 100ms (4800 samples) running average of rectified signal
///   - Onset: fast > slow * threshold (+6dB)
///   - Count: false→true leading edges (after slow_win warm-up)
///   - Density: count / ((total_samples - slow_win) / sr)
struct StreamingTransientDetector {
    fast_ring: Vec<f32>,
    fast_pos: usize,
    fast_sum: f32,
    fast_filled: usize,
    slow_ring: Vec<f32>,
    slow_pos: usize,
    slow_sum: f32,
    slow_filled: usize,
    threshold_linear: f32,
    was_above: bool,
    count: u32,
    total_samples: usize,
    pub onset_envelope: Vec<f32>,
    samples_since_env: usize,
    current_env_val: f32,
}

impl StreamingTransientDetector {
    fn new() -> Self {
        // threshold_linear = 10^(6/20) ≈ 1.9953
        // Mirrors compute_transient_density [pre_analysis.rs:550]:
        //   let threshold_linear: f32 = libm::powf(10.0, 6.0 / 20.0);
        // Pre-computed because libm is not a dep of sp314-orchestrator.
        let threshold_linear: f32 = 1.995_262_3; // 10^0.3 exact to 7 sig figs
        debug_assert!((threshold_linear - 10.0_f32.powf(6.0 / 20.0)).abs() < 1e-6);
        Self {
            fast_ring: vec![0.0; TD_FAST_WIN],
            fast_pos: 0,
            fast_sum: 0.0,
            fast_filled: 0,
            slow_ring: vec![0.0; TD_SLOW_WIN],
            slow_pos: 0,
            slow_sum: 0.0,
            slow_filled: 0,
            threshold_linear,
            was_above: false,
            count: 0,
            total_samples: 0,
            onset_envelope: Vec::new(),
            samples_since_env: 0,
            current_env_val: 0.0,
        }
    }

    /// Feed one rectified mono sample.
    /// Mirrors the per-sample loop in compute_transient_density
    /// [pre_analysis.rs:571-578]:
    ///   for i in slow_win..n {
    ///       let fast = ma(i, fast_win);
    ///       let slow = ma(i, slow_win);
    ///       let is_above = fast > slow * threshold_linear;
    ///       if is_above && !was_above { count += 1; }
    ///       was_above = is_above;
    ///   }
    fn feed(&mut self, rect_sample: f32) {
        self.total_samples += 1;

        // Update fast ring (10ms MA)
        self.fast_sum -= self.fast_ring[self.fast_pos];
        self.fast_ring[self.fast_pos] = rect_sample;
        self.fast_sum += rect_sample;
        self.fast_pos = (self.fast_pos + 1) % TD_FAST_WIN;
        if self.fast_filled < TD_FAST_WIN {
            self.fast_filled += 1;
        }

        // Update slow ring (100ms MA)
        self.slow_sum -= self.slow_ring[self.slow_pos];
        self.slow_ring[self.slow_pos] = rect_sample;
        self.slow_sum += rect_sample;
        self.slow_pos = (self.slow_pos + 1) % TD_SLOW_WIN;
        if self.slow_filled < TD_SLOW_WIN {
            self.slow_filled += 1;
        }

        // Only start detection after slow_win samples are full,
        // mirroring `for i in slow_win..n` [pre_analysis.rs:571].
        if self.slow_filled < TD_SLOW_WIN {
            return;
        }

        let fast_ma = self.fast_sum / self.fast_filled as f32;
        let slow_ma = self.slow_sum / TD_SLOW_WIN as f32;

        let is_above = fast_ma > slow_ma * self.threshold_linear;
        
        let diff = fast_ma - slow_ma * self.threshold_linear;
        if diff > self.current_env_val {
            self.current_env_val = diff;
        }
        
        self.samples_since_env += 1;
        if self.samples_since_env >= TD_FAST_WIN {
            self.onset_envelope.push(self.current_env_val.max(0.0));
            self.current_env_val = 0.0;
            self.samples_since_env = 0;
        }

        if is_above && !self.was_above {
            self.count += 1;
        }
        self.was_above = is_above;
    }

    /// Mirrors compute_transient_density [pre_analysis.rs:580-584]:
    ///   let duration = (n - slow_win) as f32 / sample_rate as f32;
    ///   count as f32 / duration
    fn finish(&self) -> f32 {
        let active_samples = self.total_samples.saturating_sub(TD_SLOW_WIN);
        if active_samples == 0 {
            return 0.0;
        }
        let duration = active_samples as f32 / SAMPLE_RATE as f32;
        self.count as f32 / duration
    }
}

/// Computes ZCR on a mono stream using 1024-sample frames and 512-sample hop.
/// Input must be the 48kHz mono downmix (L+R)/2.
/// ZCR is defined as the number of sign changes divided by (frame_size - 1).
struct StreamingZcrMeter {
    frame_buf: Vec<f32>,
    zcr_sum: f64,
    zcr_sq_sum: f64,
    count: usize,
}

impl StreamingZcrMeter {
    fn new() -> Self {
        Self {
            frame_buf: Vec::with_capacity(1024),
            zcr_sum: 0.0,
            zcr_sq_sum: 0.0,
            count: 0,
        }
    }

    fn feed_chunk(&mut self, chunk: &[f32]) {
        for &sample in chunk {
            self.frame_buf.push(sample);
            if self.frame_buf.len() == 1024 {
                let mut crossings = 0;
                for i in 1..1024 {
                    if (self.frame_buf[i] >= 0.0 && self.frame_buf[i - 1] < 0.0)
                        || (self.frame_buf[i] < 0.0 && self.frame_buf[i - 1] >= 0.0)
                    {
                        crossings += 1;
                    }
                }
                let zcr = crossings as f64 / 1023.0;
                self.zcr_sum += zcr;
                self.zcr_sq_sum += zcr * zcr;
                self.count += 1;

                self.frame_buf.drain(0..512); // Hop 512
            }
        }
    }

    fn finish(&self) -> (f32, f32) {
        if self.count == 0 {
            return (0.0, 0.0);
        }
        let mean = self.zcr_sum / self.count as f64;
        let var = (self.zcr_sq_sum / self.count as f64 - mean * mean).max(0.0);
        (mean as f32, var.sqrt() as f32)
    }
}

/// Run the trunk pass on a raw PCM dump (interleaved f32, 48 kHz, stereo).
///
/// The dump was written by pass0_decode_to_dump through StandardizedDecoder,
/// which guarantees 48k/2ch by construction [standardized_decoder.rs:70].
pub fn run_trunk_metrics(dump_path: &Path) -> Result<TrunkMetrics, String> {
    run_trunk_internal(dump_path, false, false, false, None).map(|r| r.metrics)
}

/// Same single pass, plus the ACX delivery check (sample peak, DC-removed
/// RMS, quietest-500ms noise floor — see sp314_dsp::analysis::acx_check).
/// Costs one extra HP cascade + window min per mono sample; callers that
/// aren't delivering to ACX use run_trunk_metrics and pay nothing.
pub fn run_trunk_metrics_with_acx(dump_path: &Path) -> Result<TrunkMetrics, String> {
    run_trunk_internal(dump_path, false, true, false, None).map(|r| r.metrics)
}

/// Ίδιο με το run_trunk_pass, αλλά με τον AcxCheckAnalyzer ενεργό.
///
/// Ο συνδυασμός segmentation + analyzer ΔΕΝ εκτίθεται από καμία άλλη
/// δημόσια είσοδο (μετρήθηκε 2026-09-12). Η streaming χρειάζεται
/// boundaries — άρα run_trunk_pass — αλλά χρειάζεται και το interior
/// ελάχιστο, που μόνο ο analyzer το δίνει. Απουσία εισόδου, όχι
/// απόφαση κόστους.
///
/// Το edge_sec έρχεται ως ΟΡΙΣΜΑ: η πολιτική ζει στον orchestrator,
/// ποτέ στον πυρήνα DSP — ίδιο δόγμα με το −45 fallback του gate.
///
/// ΚΟΣΤΟΣ, δηλωμένο: one extra HP cascade + window min per mono sample
/// (trunk_pass.rs, σχόλιο του with_acx).
pub fn run_trunk_pass_with_acx(
    dump_path: &Path,
    enable_vad: bool,
    edge_sec: f32,
) -> Result<TrunkReport, String> {
    run_trunk_internal(dump_path, true, true, enable_vad, Some(edge_sec))
}

pub fn run_trunk_pass(dump_path: &Path, enable_vad: bool) -> Result<TrunkReport, String> {
    run_trunk_internal(dump_path, true, false, enable_vad, None)
}

fn run_trunk_internal(
    dump_path: &Path,
    do_segmentation: bool,
    with_acx: bool,
    enable_vad: bool,
    // None ⇒ ο analyzer δεν υπολογίζει interior· δεν έχει νόημα χωρίς edge.
    edge_sec: Option<f32>,
) -> Result<TrunkReport, String> {
    // 2 channels: trunk dump is stereo f32 LE interleaved from StandardizedDecoder.
    let mut source = sp314_dsp::stft::raw_pcm_source::RawPcmFileSource::new(dump_path, 2)?;
    let srf = SAMPLE_RATE as f32;
    let nyq = srf / 2.0;

    // === Meters ===
    let mut lufs_meter = LufsMeter::new();
    let mut dynamics = StreamingDynamicsAnalyzer::new(SAMPLE_RATE);
    let mut lra_meter = StreamingLraMeter::new(SAMPLE_RATE);

    // === Noise floor: 1s energy windows ===
    let mut nf_sum_sq: f32 = 0.0;
    let mut nf_count: usize = 0;
    let mut min_nondead_dbfs: Option<f32> = None;
    // Ιστόγραμμα RMS/100 ms — ΤΡΕΦΕΤΑΙ ΣΤΟΝ ΙΔΙΟ ΒΡΟΧΟ. Κανένα δεύτερο πέρασμα
    // πάνω στο dump: ο expander παίρνει το κατώφλι του από τη μέτρηση που
    // γίνεται ούτως ή άλλως.
    let mut qw_hist = [0u32; QW_NBINS];
    let mut qw_sum_sq: f32 = 0.0;
    let mut qw_count: usize = 0;

    // === 8-band spectral profile ===
    // Mirrors spectral_profile_8band [pre_analysis.rs:469-499]:
    //   Same BAND_EDGES, same 4th-order Butterworth bandpass (2×HP + 2×LP),
    //   same RMS formula: sqrt(sum(l²+r²) / (2*N)).
    //   Stateful filters carry state across chunks → bit-identical to
    //   whole-buffer processing of the same signal.
    struct BandState {
        filter_l: BandpassFilter,
        filter_r: BandpassFilter,
        sum_sq: f64, // f64 accumulator to avoid precision loss on long files
        active: bool,
    }
    let mut bands: Vec<BandState> = (0..8)
        .map(|i| {
            let lo = BAND_EDGES[i].max(1.0);
            let hi = BAND_EDGES[i + 1].min(nyq - 1.0);
            let active = lo < hi;
            BandState {
                filter_l: BandpassFilter::new(lo, hi, srf),
                filter_r: BandpassFilter::new(lo, hi, srf),
                sum_sq: 0.0,
                active,
            }
        })
        .collect();
    let mut total_frames: usize = 0;

    // === Transient density & BPM ===
    let mut transient_det = StreamingTransientDetector::new();
    let mut zcr_meter = StreamingZcrMeter::new();

    // === VAD state ===
    let mut vad_extractor = if enable_vad { Some(sp314_dsp::analysis::vad_features::VadFeatureExtractor::new()) } else { None };
    let mut vad_classifier = if enable_vad { Some(sp314_dsp::analysis::vad_model::VadClassifier::new(
        sp314_dsp::analysis::vad_model::FixedPriors,
    )) } else { None };
    let mut vad_posteriors = Vec::new();
    let mut vad_is_speech_count = 0;
    let mut vad_current_run = 0;
    let mut vad_longest_run = 0;

    // === Phase Correlation ===
    let mut corr_cross: f32 = 0.0;
    let mut corr_sum_l: f32 = 0.0;
    let mut corr_sum_r: f32 = 0.0;

    // === Scout windowing state ===
    // Mirrors scan_file's sequential loop [scout_scanner.rs:38-56]:
    //   - One SegmentScout reused across all windows (flux/mfcc state carries).
    //   - 5s window, 1s hop, partial final window dropped.
    //   - Constructor: SegmentScout::new() [scout_scanner.rs:38].
    let win_samples = (WINDOW_SECS * SAMPLE_RATE as f32) as usize; // 240_000
    let hop_samples = (HOP_SECS * SAMPLE_RATE as f32) as usize; // 48_000
    let mut scout = SegmentScout::new();
    let mut decisions = Vec::new();
    let dump_bytes = std::fs::metadata(dump_path).map(|m| m.len()).unwrap_or(0);
    let frames_hint = dump_bytes / 8; // 2ch f32 = 8 bytes/frame
    let expected_windows = if frames_hint > win_samples as u64 {
        ((frames_hint - win_samples as u64) / hop_samples as u64 + 1) as usize
    } else {
        0
    };
    let mut cv_ioi_sequence = Vec::with_capacity(expected_windows);
    let mut cepstral_flux_sequence = Vec::with_capacity(expected_windows);

    // History buffers — hold enough for one full window + chunk slack.
    // Periodically drained so memory stays bounded at ~win_samples + CHUNK_FRAMES.
    let mut hist_left: Vec<f32> = Vec::with_capacity(win_samples + CHUNK_FRAMES);
    let mut hist_right: Vec<f32> = Vec::with_capacity(win_samples + CHUNK_FRAMES);
    let mut hist_mono: Vec<f32> = Vec::with_capacity(win_samples + CHUNK_FRAMES);
    // Global sample offset of hist[0] in the full file.
    let mut hist_base: usize = 0;
    // Next window start as a global sample offset.
    let mut next_window_start: usize = 0;

    // === Scout MFCC Tracking State ===
    let mut mfcc_analyzer = lineos_corpus::mfcc::MfccAnalyzer::new();
    let mut mfcc_prev: Option<[f32; 13]> = None;
    let mut flux_distances = std::collections::VecDeque::<f32>::with_capacity(467);
    let mut flux_running_sum: f64 = 0.0;
    let mut next_mfcc_frame_start: usize = 0;
    let mut mfcc_sums = [0.0f64; 13];
    let mut mfcc_sq_sums = [0.0f64; 13];
    let mut mfcc_count: usize = 0;

    // === Scratch buffers ===
    let mut interleaved = vec![0f32; CHUNK_FRAMES * 2];
    let mut left_chunk = vec![0f32; CHUNK_FRAMES];
    let mut right_chunk = vec![0f32; CHUNK_FRAMES];
    let mut mono_chunk = vec![0f32; CHUNK_FRAMES];
    // The dump is 48k/2ch by construction [standardized_decoder.rs:70];
    // measured on real narration: resampling 44.1k->48k moves the three
    // ACX numbers by <=0.05 dB near the -60 limit (0.31 dB at -94).
    let mut acx_analyzer = if with_acx {
        Some(sp314_dsp::analysis::acx_check::AcxCheckAnalyzer::new(
            48_000,
        ))
    } else {
        None
    };

    loop {
        let frames = source.fill_buffer(&mut interleaved)?;
        if frames == 0 {
            break;
        }
        total_frames += frames;

        // De-interleave + mono — formula mirrors scan_file's exactly:
        //   mono[i] = (l + r) * 0.5   [scout_scanner.rs:30]
        for i in 0..frames {
            left_chunk[i] = interleaved[i * 2];
            right_chunk[i] = interleaved[i * 2 + 1];
            mono_chunk[i] = (left_chunk[i] + right_chunk[i]) * 0.5;
        }
        let l = &left_chunk[..frames];
        let r = &right_chunk[..frames];
        let m = &mono_chunk[..frames];

        // --- Feed full-file meters ---
        lufs_meter.process_chunk(l, r);
        dynamics.feed_chunk(m);
        if let Some(acx) = acx_analyzer.as_mut() {
            acx.feed_chunk(m);
        }
        lra_meter.process_chunk(l, r);

        // --- 8-band spectral profile: filter + accumulate ---
        // Mirrors spectral_profile_8band's per-sample chain:
        //   let fl = bandpass_filter(left, lo, hi, srf);
        //   let fr = bandpass_filter(right, lo, hi, srf);
        //   sum_sq += fl[i]*fl[i] + fr[i]*fr[i]   [pre_analysis.rs:490]
        for i in 0..frames {
            corr_cross += l[i] * r[i];
            corr_sum_l += l[i] * l[i];
            corr_sum_r += r[i] * r[i];

            for band in bands.iter_mut() {
                if !band.active {
                    continue;
                }
                let fl = band.filter_l.process(l[i]);
                let fr = band.filter_r.process(r[i]);
                band.sum_sq += (fl * fl + fr * fr) as f64;
            }
        }

        // --- Transient density: feed rectified mono ---
        // Mirrors compute_transient_density [pre_analysis.rs:553]:
        //   let rect: Vec<f32> = mono.iter().map(|&s| libm::fabsf(s)).collect();
        for &s in m.iter() {
            transient_det.feed(f32::abs(s));
        }

        // --- ZCR: feed mono downmix ---
        zcr_meter.feed_chunk(m);

        // --- VAD: Voice as a feature ---
        if enable_vad {
            if let (Some(ext), Some(clf)) = (vad_extractor.as_mut(), vad_classifier.as_mut()) {
                let vad_features = ext.process_chunk(m, l, r);
                // Χρησιμοποιούμε το streaming floor, όχι το τελικό — 
                // τα posteriors του CLI μπορεί να αποκλίνουν ελαφρά από του render pass στα πρώτα δευτερόλεπτα.
                let current_noise_floor = min_nondead_dbfs.unwrap_or(-144.0);

                for f in vad_features {
                    let decision = clf.process(&f, current_noise_floor);
                    vad_posteriors.push(decision.posterior);
                    if decision.is_speech {
                        vad_is_speech_count += 1;
                        vad_current_run += 1;
                        vad_longest_run = vad_longest_run.max(vad_current_run);
                    } else {
                        vad_current_run = 0;
                    }
                }
            }
        }

        // --- Κατανομή ήσυχων παραθύρων: 100 ms RMS -> κάδοι του 1 dB ---
        // ΙΔΙΟΣ τύπος με το harness της 14/09: RMS πάνω σε mono downmix,
        // παράθυρα 100 ms, μερικό τελευταίο παράθυρο ΠΕΤΙΕΤΑΙ.
        for &s in m.iter() {
            qw_sum_sq += s * s;
            qw_count += 1;
            if qw_count == QW_WINDOW {
                let e = (qw_sum_sq / QW_WINDOW as f32) as f64;
                if e > 0.0 {
                    let db = 10.0 * e.log10();
                    let bin = (db - QW_LO_DB as f64).floor();
                    if bin >= 0.0 && (bin as usize) < QW_NBINS {
                        qw_hist[bin as usize] += 1;
                    }
                }
                qw_sum_sq = 0.0;
                qw_count = 0;
            }
        }

        // --- Noise floor: 1s energy windows ---
        for &s in m.iter() {
            nf_sum_sq += s * s;
            nf_count += 1;
            if nf_count == NOISE_FLOOR_WINDOW {
                let rms = (nf_sum_sq / NOISE_FLOOR_WINDOW as f32).sqrt();
                let dbfs = if rms < 1e-10 {
                    -144.0
                } else {
                    20.0 * rms.log10()
                };
                // Gate: only non-dead-air windows contribute to noise floor.
                // Mirrors DEAD_AIR_WINDOW_DBFS = -60.0 [signal_health.rs:92].
                if dbfs >= DEAD_AIR_GATE_DBFS {
                    min_nondead_dbfs = Some(match min_nondead_dbfs {
                        Some(prev) => prev.min(dbfs),
                        None => dbfs,
                    });
                }
                nf_sum_sq = 0.0;
                nf_count = 0;
            }
        }

        // --- Append to scout history ---
        // F-051: metrics-only mode must not accumulate history
        // — unbounded growth (C-switch regression, caught by the episode heap oracle).
        if do_segmentation {
            hist_left.extend_from_slice(l);
            hist_right.extend_from_slice(r);
            hist_mono.extend_from_slice(m);
        }

        // --- Serve scout windows ---
        if do_segmentation {
            let hist_end = hist_base + hist_mono.len();

            // --- Compute O(1) MFCC distance ring ---
            while next_mfcc_frame_start + 1024 <= hist_end {
                let local_start = next_mfcc_frame_start - hist_base;
                let mfcc_curr = mfcc_analyzer.compute(&hist_mono[local_start..local_start + 1024]);

                for k in 0..13 {
                    let val = mfcc_curr[k] as f64;
                    mfcc_sums[k] += val;
                    mfcc_sq_sums[k] += val * val;
                }
                mfcc_count += 1;

                if let Some(prev) = mfcc_prev {
                    let dist = lineos_corpus::scout::mfcc_euclidean_distance(&prev, &mfcc_curr);
                    flux_distances.push_back(dist);
                    flux_running_sum += dist as f64;

                    // 5 seconds = 467 frames -> 466 distances max.
                    if flux_distances.len() > 466 {
                        let oldest = flux_distances.pop_front().unwrap();
                        flux_running_sum -= oldest as f64;
                    }
                }
                mfcc_prev = Some(mfcc_curr);
                next_mfcc_frame_start += 512;
            }

            while next_window_start + win_samples <= hist_end {
                let local_start = next_window_start - hist_base;
                let local_end = local_start + win_samples;

                let start_sec = next_window_start as f32 / SAMPLE_RATE as f32;
                let cepstral_flux = if flux_distances.is_empty() {
                    0.0
                } else {
                    (flux_running_sum / flux_distances.len() as f64) as f32
                };
                let meas = scout.measure(
                    &hist_mono[local_start..local_end],
                    cepstral_flux,
                    SAMPLE_RATE,
                );
                cv_ioi_sequence.push(meas.cv_ioi);
                cepstral_flux_sequence.push(meas.cepstral_flux);
                decisions.push((start_sec, compute_scout_decision(&meas)));
                next_window_start += hop_samples;
            }

            // --- Drain consumed history ---
            let mut keep_from = if next_window_start >= win_samples {
                next_window_start - win_samples + hop_samples
            } else {
                0
            };
            keep_from = keep_from.min(next_mfcc_frame_start);
            if keep_from > hist_base {
                let drain_count = keep_from - hist_base;
                if drain_count > 0 && drain_count <= hist_mono.len() {
                    hist_left.drain(..drain_count);
                    hist_right.drain(..drain_count);
                    hist_mono.drain(..drain_count);
                    hist_base = keep_from;
                }
            }
        }
    }

    // === Finish meters ===
    let integrated_lufs = lufs_meter.finish();
    let dyn_result = dynamics.finish();
    let (rms_db, crest_db, dyn_range) = (
        dyn_result.rms_db,
        dyn_result.crest_db,
        dyn_result.dyn_range_db,
    );
    let acx_noise_floor_proxy_db = dyn_result.p5_block_rms_db;
    // ⚠ Η ΣΕΙΡΑ ΕΙΝΑΙ ΥΠΟΧΡΕΩΤΙΚΗ: το finish() ΚΑΤΑΝΑΛΩΝΕΙ τον analyzer, ενώ
    // το interior_noise_floor_db θέλει &self. Μετά τη γραμμή του finish() το
    // interior ΔΕΝ ανακτάται κατάντη — το AcxCheckReport δεν το φέρει, και
    // δεν μπορεί να το αποκτήσει (σχήμα παγωμένο 2026-08-23).
    let acx_interior = acx_analyzer
        .as_ref()
        .and_then(|a| edge_sec.and_then(|e| a.interior_noise_floor_db(e)));
    let acx = acx_analyzer.map(|a| a.finish());
    let lra = lra_meter.finish();

    // === Finish spectral profile ===
    // Mirrors spectral_profile_8band [pre_analysis.rs:490-494]:
    //   let rms = sqrtf(sum_sq / (2.0 * left.len() as f32));
    //   if rms > 1e-20 { profile[i] = 20.0 * log10f(rms); }
    let mut spectral_profile_db = [-144.0_f32; 8];
    if total_frames > 0 {
        for (i, band) in bands.iter().enumerate() {
            if !band.active {
                continue;
            }
            let rms = (band.sum_sq / (2.0 * total_frames as f64)).sqrt() as f32;
            if rms > 1e-20 {
                spectral_profile_db[i] = 20.0 * rms.log10();
            }
        }
    }

    // === Finish transient density & BPM ===
    let transient_density = transient_det.finish();
    
    let mut bpm_estimate = 0.0;
    let mut bpm_confidence = 0.0;
    if !transient_det.onset_envelope.is_empty() {
        let env = &transient_det.onset_envelope;
        let mean = env.iter().sum::<f32>() / env.len() as f32;
        let mut zero_mean_env: Vec<f32> = env.iter().map(|&x| x - mean).collect();
        
        let mut r = vec![0.0; 151];
        let n = zero_mean_env.len();
        
        // R[0]
        for i in 0..n {
            r[0] += zero_mean_env[i] * zero_mean_env[i];
        }
        
        if r[0] > 1e-10 {
            let mut peak_lag = 30;
            let mut peak_val = -1.0;
            
            for lag in 30..=150 {
                let mut sum = 0.0;
                let end = n.saturating_sub(lag);
                for i in 0..end {
                    sum += zero_mean_env[i] * zero_mean_env[i + lag];
                }
                r[lag] = sum;
                if sum > peak_val {
                    peak_val = sum;
                    peak_lag = lag;
                }
            }
            
            bpm_estimate = 6000.0 / peak_lag as f32;
            bpm_confidence = (peak_val / r[0]).clamp(0.0, 1.0);
        }
    }

    let (zcr_mean, zcr_std) = zcr_meter.finish();

    // === Finish VAD metrics ===
    let (voice_ratio, voice_posterior_mean, voice_posterior_std, voice_longest_run_s) = if enable_vad {
        let ratio = if !vad_posteriors.is_empty() {
            vad_is_speech_count as f32 / vad_posteriors.len() as f32
        } else {
            0.0
        };
        
        let (mean, std) = if !vad_posteriors.is_empty() {
            let m = vad_posteriors.iter().sum::<f32>() / vad_posteriors.len() as f32;
            let mut var = 0.0;
            for &p in &vad_posteriors {
                var += (p - m) * (p - m);
            }
            var /= vad_posteriors.len() as f32;
            (m, var.sqrt())
        } else {
            (0.0, 0.0)
        };
        // VAD frames are 10ms each (FRAME_SAMPLES=480 in 48kHz)
        let longest_run_s = vad_longest_run as f32 * 0.01;
        (Some(ratio), Some(mean), Some(std), Some(longest_run_s))
    } else {
        (None, None, None, None)
    };

    // === Finish phase correlation ===
    let denom = (corr_sum_l * corr_sum_r).sqrt();
    let global_phase_correlation = if denom < 1e-10 {
        1.0
    } else {
        (corr_cross / denom).clamp(-1.0, 1.0)
    };

    // === Segmentation ===
    let boundaries = if do_segmentation {
        smooth_and_segment(&decisions)
    } else {
        Vec::new()
    };

    // === Compute MFCC statistics ===
    let mut mfcc_means = [0.0f32; 13];
    let mut mfcc_stds = [0.0f32; 13];
    if mfcc_count > 0 {
        let n = mfcc_count as f64;
        for k in 0..13 {
            let mean = mfcc_sums[k] / n;
            // Guard against floating point cancellation producing negative variance
            let var = (mfcc_sq_sums[k] / n - mean * mean).max(0.0);
            mfcc_means[k] = mean as f32;
            mfcc_stds[k] = var.sqrt() as f32;
        }
    }

    Ok(TrunkReport {
        boundaries,
        acx_interior_noise_floor: acx_interior,
        quiet_window_split_dbfs: quiet_window_split_db(&qw_hist),
        metrics: TrunkMetrics {
            integrated_lufs,
            rms_db,
            acx_noise_floor_proxy_db,
            acx,
            crest_db,
            lra,
            quietest_active_window_dbfs: min_nondead_dbfs,
            spectral_profile_db,
            transient_density,
            zcr_mean,
            zcr_std,
            bpm_estimate,
            bpm_confidence,
            global_phase_correlation,
            dynamic_range_db: dyn_range,
            cv_ioi_sequence,
            cepstral_flux_sequence,
            voice_ratio,
            voice_posterior_mean,
            voice_posterior_std,
            voice_longest_run_s,
            mfcc_means,
            mfcc_stds,
        },
    })
}
