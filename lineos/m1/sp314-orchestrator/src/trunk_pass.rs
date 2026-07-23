//! Y1 Trunk Pass — single-pass segmentation + metering over the raw PCM dump.
//!
//! Replaces the old two-step sequence of
//! `build_timeline_map` (WholeBufferProvider → full-file RAM decode → scan_file)
//! and separate metering passes with ONE chunked read of the dump file
//! (written by P0-a), producing boundaries + LUFS + crest + LRA + noise
//! floor in a single linear scan.
//!
//! STRANGLER FIG: The measurement logic is IDENTICAL to scan_file's —
//! same SegmentScout instance, same 5s windows, same 1s hops, same
//! sequential order, same partial-window drop. Only the I/O layer changed
//! (dump-backed ChunkSource instead of full-file slice).

use lineos_corpus::scout::{compute_scout_decision, smooth_and_segment, SegmentBoundary};
use sp314_dsp::analysis::dynamics::StreamingDynamicsAnalyzer;
use sp314_dsp::analysis::scout::SegmentScout;
use sp314_dsp::metering::lra::StreamingLraMeter;
use sp314_dsp::metering::LufsMeter;
use sp314_dsp::stft::sliding_overlap_reader::ChunkSource;
use std::path::Path;

/// Metrics produced by the trunk pass.
pub struct TrunkReport {
    pub boundaries: Vec<SegmentBoundary>,
    pub integrated_lufs: Option<f32>,
    pub crest_db: f32,
    pub lra: f32,
    pub noise_floor_dbfs: Option<f32>,
}

// Mirror scan_file's constants exactly [scout_scanner.rs:6-7].
const WINDOW_SECS: f32 = 5.0;
const HOP_SECS: f32 = 1.0;
const SAMPLE_RATE: u32 = 48_000;
const CHUNK_FRAMES: usize = 4096;

// Mirror signal_health.rs:92 — windows below this are dead air.
const DEAD_AIR_GATE_DBFS: f32 = -60.0;
const NOISE_FLOOR_WINDOW: usize = 48_000; // 1s at 48kHz

/// Run the trunk pass on a raw PCM dump (interleaved f32, 48 kHz, stereo).
///
/// The dump was written by pass0_decode_to_dump through StandardizedDecoder,
/// which guarantees 48k/2ch by construction [standardized_decoder.rs:70].
pub fn run_trunk_pass(dump_path: &Path) -> Result<TrunkReport, String> {
    let mut source = crate::raw_pcm_source::RawPcmFileSource::new(dump_path)?;

    // === Meters ===
    let mut lufs_meter = LufsMeter::new();
    let mut dynamics = StreamingDynamicsAnalyzer::new(SAMPLE_RATE);
    let mut lra_meter = StreamingLraMeter::new(SAMPLE_RATE);

    // === Noise floor: 1s energy windows ===
    let mut nf_sum_sq: f32 = 0.0;
    let mut nf_count: usize = 0;
    let mut min_nondead_dbfs: Option<f32> = None;

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
        hist_left.extend_from_slice(l);
        hist_right.extend_from_slice(r);
        hist_mono.extend_from_slice(m);

        // --- Serve scout windows ---
        // WINDOW-PARITY GUARANTEE: scan_file's loop condition is
        //   `while start + win_samples <= mono_full.len()`  [scout_scanner.rs:43]
        // Our equivalent: we only measure when the history contains a
        // FULL window starting at next_window_start, and next_window_start
        // advances by hop_samples (48_000) each time. The global offsets
        // match scan_file's `start` variable exactly:
        //   scan_file: start = 0, hop, 2*hop, ... while start + win <= N
        //   trunk:     next_window_start = 0, hop, 2*hop, ... same condition
        // The last partial window (< win_samples) is NOT measured, matching
        // scan_file's behavior exactly.
        let hist_end = hist_base + hist_mono.len();
        while next_window_start + win_samples <= hist_end {
            let local_start = next_window_start - hist_base;
            let local_end = local_start + win_samples;

            let start_sec = next_window_start as f32 / SAMPLE_RATE as f32;
            let meas = scout.measure(
                &hist_mono[local_start..local_end],
                &hist_left[local_start..local_end],
                &hist_right[local_start..local_end],
                SAMPLE_RATE,
            );
            decisions.push((start_sec, compute_scout_decision(&meas)));
            next_window_start += hop_samples;
        }

        // --- Drain consumed history ---
        // Everything before next_window_start - (win_samples - hop_samples)
        // can never be needed again (the next window starts at
        // next_window_start and looks back win_samples from there).
        let keep_from = if next_window_start >= win_samples {
            next_window_start - win_samples + hop_samples
        } else {
            0
        };
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

    // === Finish meters ===
    let integrated_lufs = lufs_meter.finish();
    let (_rms_db, crest_db, _dyn_range) = dynamics.finish();
    let lra = lra_meter.finish();

    // === Segmentation ===
    let boundaries = smooth_and_segment(&decisions);

    Ok(TrunkReport {
        boundaries,
        integrated_lufs,
        crest_db,
        lra,
        noise_floor_dbfs: min_nondead_dbfs,
    })
}
