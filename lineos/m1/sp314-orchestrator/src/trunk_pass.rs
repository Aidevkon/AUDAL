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
    pub crest_db: f32,
    pub lra: f32,
    pub noise_floor_dbfs: Option<f32>,
    pub spectral_profile_db: [f32; 8],
    pub transient_density: f32,
    pub global_phase_correlation: f32,
    /// 95th-5th percentile RMS block spread, 50ms blocks (the real broadcast DR metric)
    pub dynamic_range_db: f32,
}

pub struct TrunkReport {
    pub boundaries: Vec<SegmentBoundary>,
    pub metrics: TrunkMetrics,
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

/// Run the trunk pass on a raw PCM dump (interleaved f32, 48 kHz, stereo).
///
/// The dump was written by pass0_decode_to_dump through StandardizedDecoder,
/// which guarantees 48k/2ch by construction [standardized_decoder.rs:70].
pub fn run_trunk_metrics(dump_path: &Path) -> Result<TrunkMetrics, String> {
    run_trunk_internal(dump_path, false).map(|r| r.metrics)
}

pub fn run_trunk_pass(dump_path: &Path) -> Result<TrunkReport, String> {
    run_trunk_internal(dump_path, true)
}

fn run_trunk_internal(dump_path: &Path, do_segmentation: bool) -> Result<TrunkReport, String> {
    // 2 channels: trunk dump is stereo f32 LE interleaved from StandardizedDecoder.
    let mut source = crate::raw_pcm_source::RawPcmFileSource::new(dump_path, 2)?;
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

    // === Transient density ===
    let mut transient_det = StreamingTransientDetector::new();

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

    // === Scratch buffers ===
    let mut interleaved = vec![0f32; CHUNK_FRAMES * 2];
    let mut left_chunk = vec![0f32; CHUNK_FRAMES];
    let mut right_chunk = vec![0f32; CHUNK_FRAMES];
    let mut mono_chunk = vec![0f32; CHUNK_FRAMES];

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
            transient_det.feed(s.abs());
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
    let (rms_db, crest_db, dyn_range) = dynamics.finish();
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

    // === Finish transient density ===
    let transient_density = transient_det.finish();

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

    Ok(TrunkReport {
        boundaries,
        metrics: TrunkMetrics {
            integrated_lufs,
            rms_db,
            crest_db,
            lra,
            noise_floor_dbfs: min_nondead_dbfs,
            spectral_profile_db,
            transient_density,
            global_phase_correlation,
            dynamic_range_db: dyn_range,
        },
    })
}
