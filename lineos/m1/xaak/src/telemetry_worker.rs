//! telemetry_worker.rs — Off-thread FFT + UDP telemetry for dual-spectrum delta.
//!
//! Consumes mastered PCM (telem_cons) and raw PCM (telem_cons_raw) from
//! lock-free ring buffers filled by the cpal callback, computes two 64-band
//! spectra per chunk (spectrum_after = mastered, spectrum_before = raw),
//! and ships a RealtimeFrame via UDP to the Tauri telemetry listener.
//!
//! This thread MUST NOT be joined or awaited from the audio thread.

use ringbuf::traits::*;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError};
use ringbuf::wrap::caching::Caching;

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
        let mut analyzer_after  = crate::spectrum::SpectrumAnalyzer::new();
        let mut analyzer_before = crate::spectrum::SpectrumAnalyzer::new();

        let mut active_stream: Option<TelemetryCommand> = None;
        let mut already_flushed     = false;
        let mut already_flushed_raw = false;
        
        let mut buffer     = [0.0f32; 8192];
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
            }) = active_stream.take() {
                let ch = channels.max(1);
                let chunk_size = 4096 * ch;

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
                        Ok(TelemetryCommand::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {
                            // Put stream back and continue to re-evaluate occupied
                            active_stream = Some(TelemetryCommand::StartStream {
                                mastered_cons, raw_cons, sample_rate, channels, position_ms, stream_ended
                            });
                            continue;
                        }
                    }
                }

                // Drain: full chunk_size normally, or just `occupied` for EOF tail.
                let to_read = if do_eof_flush { occupied } else { chunk_size };
                let mut read = 0;
                while read < to_read {
                    let n = mastered_cons.pop_slice(&mut buffer[read..to_read]);
                    if n == 0 { break; }
                    read += n;
                }
                if do_eof_flush {
                    // Zero-pad remainder to chunk_size so FFT receives a full window.
                    for s in &mut buffer[read..chunk_size] { *s = 0.0; }
                    already_flushed = true;
                }

                // Compute spectrum_before from raw (pre-mastering) PCM if available.
                let mut spectrum_before = [-120.0f32; 64];
                eprintln!("[RAW-TELEM] computing before? cons_some={} occupied={} chunk_size={}", 
                          raw_cons.is_some(), 
                          raw_cons.as_ref().map(|c| c.occupied_len()).unwrap_or(0), 
                          chunk_size);
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
                        let mut raw_read = 0;
                        while raw_read < chunk_size {
                            let n = raw_c.pop_slice(&mut raw_buffer[raw_read..chunk_size]);
                            if n == 0 { break; }
                            raw_read += n;
                        }
                        spectrum_before = analyzer_before.compute(&raw_buffer[..chunk_size], ch);
                    } else if do_eof_flush_raw {
                        let mut raw_read = 0;
                        while raw_read < raw_occupied {
                            let n = raw_c.pop_slice(&mut raw_buffer[raw_read..raw_occupied]);
                            if n == 0 { break; }
                            raw_read += n;
                        }
                        for s in &mut raw_buffer[raw_read..chunk_size] { *s = 0.0; }
                        spectrum_before = analyzer_before.compute(&raw_buffer[..chunk_size], ch);
                        already_flushed_raw = true;
                    }
                }

                // Compute spectrum_after from mastered PCM.
                let spectrum_after = analyzer_after.compute(&buffer[..chunk_size], ch);

                let pos_val = *position_ms.lock().unwrap_or_else(|e| e.into_inner());

                // Temporary [BIN-DUMP] trap
                if pos_val % 2000 < 50 {
                    let fmt = |v: &[f32]| -> String {
                        v.iter().take(10).map(|x| format!("{:.1}", x)).collect::<Vec<_>>().join(",")
                    };
                    eprintln!("[BIN-DUMP] before[0..10]=[{}]", fmt(&spectrum_before));
                }

                let frame = lineos_types::telemetry::RealtimeFrame {
                    spectrum_before,
                    spectrum_after,
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
                    mastered_cons, raw_cons, sample_rate, channels, position_ms, stream_ended
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
