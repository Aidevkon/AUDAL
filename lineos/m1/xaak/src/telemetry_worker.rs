//! telemetry_worker.rs — Off-thread FFT + UDP telemetry for dual-spectrum delta.
//!
//! Consumes mastered PCM (telem_cons) and raw PCM (telem_cons_raw) from
//! lock-free ring buffers filled by the cpal callback, computes two 64-band
//! spectra per chunk (spectrum_after = mastered, spectrum_before = raw),
//! and ships a RealtimeFrame via UDP to the Tauri telemetry listener.
//!
//! This thread MUST NOT be joined or awaited from the audio thread.

use crate::crossover::CrossoverLR4;
use ringbuf::traits::*;
use ringbuf::wrap::caching::Caching;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const KEPLER_CROSSOVER_HZ: [f32; 4] = [112.0, 332.0, 1500.0, 6777.0];

pub type TelemCons = Caching<Arc<ringbuf::SharedRb<ringbuf::storage::Heap<f32>>>, false, true>;

pub enum TelemetryCommand {
    StartStream {
        mastered_cons: TelemCons,
        raw_cons: Option<TelemCons>,
        sample_rate: u32,
        channels: usize,
        position_ms: Arc<Mutex<u64>>,
        stream_ended: Arc<AtomicBool>,
    },
    Shutdown,
}

pub fn spawn(rx: Receiver<TelemetryCommand>) {
    std::thread::spawn(move || {
        tracing::debug!("xaak: telemetry actor thread started");

        let sender = crate::telemetry::UdpTelemetrySender::new();
        let mut analyzer_after = crate::spectrum::SpectrumAnalyzer::new();
        let mut analyzer_before = crate::spectrum::SpectrumAnalyzer::new();

        let mut current_sr = 0;
        let mut xover_l: Option<[CrossoverLR4; 4]> = None;
        let mut xover_r: Option<[CrossoverLR4; 4]> = None;

        let mut active_stream: Option<TelemetryCommand> = None;
        let mut already_flushed = false;
        let mut already_flushed_raw = false;

        let mut buffer = [0.0f32; 8192];
        let mut raw_buffer = [0.0f32; 8192];

        loop {
            // Fast path check: instantly check if a new stream arrived without adding latency.
            // Trade-off evaluation: If we used recv_timeout(2ms) here, we would inject a
            // constant +2ms latency to EVERY telemetry frame, even when the buffer is full.
            // By using try_recv() here, the fast path is 0ms. We only use recv_timeout(2ms)
            // when we actually need to wait for more audio data (in the 'not enough data' branch).
            if let Some(TelemetryCommand::StartStream { .. }) = active_stream {
                match rx.try_recv() {
                    Ok(cmd @ TelemetryCommand::StartStream { .. }) => {
                        eprintln!("[ACTOR-REPLACE] dropping old stream mid-playback!");
                        active_stream = Some(cmd);
                        already_flushed = false;
                        already_flushed_raw = false;
                        continue;
                    }
                    Ok(TelemetryCommand::Shutdown) | Err(TryRecvError::Disconnected) => break,
                    Err(TryRecvError::Empty) => {} // normal execution, buffer has data
                }
            }

            if let Some(TelemetryCommand::StartStream {
                mut mastered_cons,
                mut raw_cons,
                sample_rate,
                channels,
                position_ms,
                stream_ended,
            }) = active_stream.take()
            {
                if current_sr != sample_rate || xover_l.is_none() {
                    let make_xovers = || -> [CrossoverLR4; 4] {
                        [
                            CrossoverLR4::new(KEPLER_CROSSOVER_HZ[0], sample_rate),
                            CrossoverLR4::new(KEPLER_CROSSOVER_HZ[1], sample_rate),
                            CrossoverLR4::new(KEPLER_CROSSOVER_HZ[2], sample_rate),
                            CrossoverLR4::new(KEPLER_CROSSOVER_HZ[3], sample_rate),
                        ]
                    };
                    xover_l = Some(make_xovers());
                    xover_r = Some(make_xovers());
                    current_sr = sample_rate;
                }

                let ch = channels.max(1);
                let chunk_size = 4096 * ch;
                let hop_size = 2048 * ch;

                let occupied = mastered_cons.occupied_len();

                // EOF tail flush: if the stream has ended, there are leftover samples
                let do_eof_flush = occupied > 0
                    && occupied < chunk_size
                    && stream_ended.load(Ordering::Acquire)
                    && !already_flushed;

                if stream_ended.load(Ordering::Acquire) && occupied == 0 {
                    // Mastered stream ended and buffer is completely dry.
                    // Transition to IDLE state (active_stream remains None).
                    continue;
                }

                if occupied < chunk_size && !do_eof_flush {
                    // Not enough data yet. Instead of thread::sleep, we wait on the channel
                    // to instantly catch rapid play commands while sleeping.
                    match rx.recv_timeout(Duration::from_millis(2)) {
                        Ok(cmd @ TelemetryCommand::StartStream { .. }) => {
                            eprintln!("[ACTOR-REPLACE] dropping old stream mid-playback!");
                            active_stream = Some(cmd);
                            already_flushed = false;
                            already_flushed_raw = false;
                            continue;
                        }
                        Ok(TelemetryCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                            break
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            // Put stream back and continue to re-evaluate occupied
                            active_stream = Some(TelemetryCommand::StartStream {
                                mastered_cons,
                                raw_cons,
                                sample_rate,
                                channels,
                                position_ms,
                                stream_ended,
                            });
                            continue;
                        }
                    }
                }

                if !do_eof_flush {
                    // OLA steady-state: peek 4096 samples, then skip hop size.
                    let (left, right) = mastered_cons.as_slices();
                    let left_len = left.len().min(chunk_size);
                    buffer[..left_len].copy_from_slice(&left[..left_len]);
                    if left_len < chunk_size {
                        let right_len = chunk_size - left_len;
                        buffer[left_len..chunk_size].copy_from_slice(&right[..right_len]);
                    }
                    mastered_cons.skip(hop_size);
                } else {
                    // EOF flush tail: pop whatever is left (no overlap possible) and zero-pad.
                    let mut read = 0;
                    while read < occupied {
                        let n = mastered_cons.pop_slice(&mut buffer[read..occupied]);
                        if n == 0 {
                            break;
                        }
                        read += n;
                    }
                    for s in &mut buffer[read..chunk_size] {
                        *s = 0.0;
                    }
                    already_flushed = true;
                }

                // Compute spectrum_before from raw (pre-mastering) PCM if available.
                let mut spectrum_before = [-120.0f32; 64];
                #[cfg(feature = "debug-telem")]
                eprintln!(
                    "[RAW-TELEM] computing before? cons_some={} occupied={} chunk_size={}",
                    raw_cons.is_some(),
                    raw_cons.as_ref().map(|c| c.occupied_len()).unwrap_or(0),
                    chunk_size
                );
                if let Some(ref mut raw_c) = raw_cons {
                    let raw_occupied = raw_c.occupied_len();

                    let do_eof_flush_raw = raw_occupied > 0
                        && raw_occupied < chunk_size
                        && stream_ended.load(Ordering::Acquire)
                        && !already_flushed_raw;

                    if stream_ended.load(Ordering::Acquire) && raw_occupied == 0 {
                        // Raw stream ended and buffer is completely dry.
                        // Mark as flushed so the normal exit condition can trigger.
                        already_flushed_raw = true;
                    }

                    if raw_occupied >= chunk_size {
                        let (left, right) = raw_c.as_slices();
                        let left_len = left.len().min(chunk_size);
                        raw_buffer[..left_len].copy_from_slice(&left[..left_len]);
                        if left_len < chunk_size {
                            let right_len = chunk_size - left_len;
                            raw_buffer[left_len..chunk_size].copy_from_slice(&right[..right_len]);
                        }
                        raw_c.skip(hop_size);
                        spectrum_before = analyzer_before.compute(&raw_buffer[..chunk_size], ch);
                    } else if do_eof_flush_raw {
                        let mut raw_read = 0;
                        while raw_read < raw_occupied {
                            let n = raw_c.pop_slice(&mut raw_buffer[raw_read..raw_occupied]);
                            if n == 0 {
                                break;
                            }
                            raw_read += n;
                        }
                        for s in &mut raw_buffer[raw_read..chunk_size] {
                            *s = 0.0;
                        }
                        spectrum_before = analyzer_before.compute(&raw_buffer[..chunk_size], ch);
                        already_flushed_raw = true;
                    }
                }

                // Compute spectrum_after from mastered PCM.
                let spectrum_after = analyzer_after.compute(&buffer[..chunk_size], ch);

                let pos_val = *position_ms.lock().unwrap_or_else(|e| e.into_inner());

                // Mid/Side energy (RMS, dBFS) for the spatial "Kepler" visualizer.
                // Same M=(L+R)*0.5, S=(L-R)*0.5 definition proven correct by the null-test
                // in sp314-dsp/tests/midside_contract.rs (test_midside_matrix_null_roundtrip,
                // commit 7b9f6a4). Computed inline here (not via MidSideMatrix::encode())
                // to avoid two heap allocations per telemetry frame on this hot path —
                // we only need the RMS reduction, not the full M/S sample arrays.
                let (energy_mid, energy_side, band_mid_db, band_side_db, band_pan) = if ch == 2 {
                    let xl = xover_l.as_mut().unwrap();
                    let xr = xover_r.as_mut().unwrap();

                    let hop_slice = &buffer[hop_size..chunk_size];
                    let frames = hop_slice.len() / 2;

                    let mut band_sum_m = [0.0_f32; 5];
                    let mut band_sum_s = [0.0_f32; 5];
                    let mut band_sum_l = [0.0_f32; 5];
                    let mut band_sum_r = [0.0_f32; 5];

                    // energy_mid/energy_side remain full-spectrum RMS (unchanged from this morning's commit),
                    // separate from the new per-band split — NOT band_mid_db[0], which would silently narrow
                    // the existing metric to only the Low band.
                    let mut sum_m_total = 0.0_f32;
                    let mut sum_s_total = 0.0_f32;

                    for frame in hop_slice.chunks_exact(2) {
                        let l = frame[0];
                        let r = frame[1];

                        let m_full = (l + r) * 0.5;
                        let s_full = (l - r) * 0.5;
                        sum_m_total += m_full * m_full;
                        sum_s_total += s_full * s_full;

                        let (l_low, l_rem1) = xl[0].process(l);
                        let (l_low_mid, l_rem2) = xl[1].process(l_rem1);
                        let (l_mid, l_rem3) = xl[2].process(l_rem2);
                        let (l_high_mid, l_high) = xl[3].process(l_rem3);
                        let bands_l = [l_low, l_low_mid, l_mid, l_high_mid, l_high];

                        let (r_low, r_rem1) = xr[0].process(r);
                        let (r_low_mid, r_rem2) = xr[1].process(r_rem1);
                        let (r_mid, r_rem3) = xr[2].process(r_rem2);
                        let (r_high_mid, r_high) = xr[3].process(r_rem3);
                        let bands_r = [r_low, r_low_mid, r_mid, r_high_mid, r_high];

                        for i in 0..5 {
                            let b_l = bands_l[i];
                            let b_r = bands_r[i];
                            band_sum_l[i] += b_l * b_l;
                            band_sum_r[i] += b_r * b_r;

                            let m = (b_l + b_r) * 0.5;
                            let s = (b_l - b_r) * 0.5;
                            band_sum_m[i] += m * m;
                            band_sum_s[i] += s * s;
                        }
                    }

                    let to_db = |amp: f32| {
                        if amp > 1e-6 {
                            20.0 * amp.log10()
                        } else {
                            -120.0
                        }
                    };

                    let mut final_m_db = [0.0_f32; 5];
                    let mut final_s_db = [0.0_f32; 5];
                    let mut final_pan = [0.0_f32; 5];

                    let inv_frames = 1.0 / frames as f32;
                    for i in 0..5 {
                        let rms_m = (band_sum_m[i] * inv_frames).sqrt();
                        let rms_s = (band_sum_s[i] * inv_frames).sqrt();

                        final_m_db[i] = to_db(rms_m);
                        final_s_db[i] = to_db(rms_s);

                        let e_l = band_sum_l[i];
                        let e_r = band_sum_r[i];
                        final_pan[i] = (e_r - e_l) / (e_r + e_l + 1e-9);
                    }

                    let total_m_db = to_db((sum_m_total * inv_frames).sqrt());
                    let total_s_db = to_db((sum_s_total * inv_frames).sqrt());

                    (total_m_db, total_s_db, final_m_db, final_s_db, final_pan)
                } else {
                    (-120.0, -120.0, [-120.0; 5], [-120.0; 5], [0.0; 5]) // Mono: no spatial information, Side is silent by definition.
                };

                // Temporary [BIN-DUMP] trap
                #[cfg(feature = "debug-telem")]
                {
                    if pos_val % 2000 < 50 {
                        let fmt = |v: &[f32]| -> String {
                            v.iter()
                                .take(10)
                                .map(|x| format!("{:.1}", x))
                                .collect::<Vec<_>>()
                                .join(",")
                        };
                        eprintln!("[BIN-DUMP] before[0..10]=[{}]", fmt(&spectrum_before));
                    }
                }

                let frame = lineos_types::telemetry::RealtimeFrame {
                    spectrum_before,
                    spectrum_after,
                    energy_mid,
                    energy_side,
                    band_mid_db,
                    band_side_db,
                    band_pan,
                    gonio_path: crate::player::decimate_gonio(&buffer[..chunk_size], ch),
                    position_ms: pos_val,
                };

                sender.send_frame(&frame);

                // Exit active state once both buffers have been flushed.
                let raw_done = raw_cons.is_none() || already_flushed_raw;
                if already_flushed && raw_done {
                    // ALL DONE -> Transition to IDLE (active_stream remains None)
                    continue;
                }

                // Put stream back for next iteration
                active_stream = Some(TelemetryCommand::StartStream {
                    mastered_cons,
                    raw_cons,
                    sample_rate,
                    channels,
                    position_ms,
                    stream_ended,
                });
            } else {
                // IDLE STATE
                eprintln!("[ACTOR-IDLE] waiting for command");
                match rx.recv() {
                    Ok(cmd @ TelemetryCommand::StartStream { .. }) => {
                        active_stream = Some(cmd);
                        already_flushed = false;
                        already_flushed_raw = false;
                    }
                    Ok(TelemetryCommand::Shutdown) | Err(_) => break,
                }
            }
        }
    });
}
