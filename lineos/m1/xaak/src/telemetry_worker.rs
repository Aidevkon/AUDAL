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

pub fn spawn<C>(
    mut consumer: C,
    mut raw_consumer: Option<C>,
    sample_rate: u32,
    channels: usize,
    pos_mutex: Arc<Mutex<u64>>,
    stream_ended: Arc<AtomicBool>,
)
where
    C: Consumer<Item = f32> + Observer + Send + 'static,
{
    std::thread::spawn(move || {
        tracing::debug!(sample_rate, channels, "xaak: telemetry worker started");

        let sender = crate::telemetry::UdpTelemetrySender::new();
        let mut analyzer_after  = crate::spectrum::SpectrumAnalyzer::new();
        let mut analyzer_before = crate::spectrum::SpectrumAnalyzer::new();

        let ch = channels.max(1);
        // chunk_size: 4096 frames × channels (8192 samples for stereo). UNCHANGED.
        let chunk_size = 4096 * ch;
        // Fixed stack buffers — zero heap allocation per iteration.
        let mut buffer     = [0.0f32; 8192];
        let mut raw_buffer = [0.0f32; 8192];
        // EOF tail flush state: true after we've zero-padded and sent the tail FFT once.
        let mut already_flushed     = false; // mastered buffer
        let mut already_flushed_raw = false; // raw buffer (independent)

        loop {
            let occupied = consumer.occupied_len();

            // EOF tail flush: if the stream has ended, there are leftover samples
            // (0 < occupied < chunk_size), and we haven't flushed yet — drain them,
            // zero-pad to chunk_size, and let the normal compute path run ONCE.
            let do_eof_flush = occupied > 0
                && occupied < chunk_size
                && stream_ended.load(Ordering::Acquire)
                && !already_flushed;

            if occupied < chunk_size && !do_eof_flush {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }

            // Drain: full chunk_size normally, or just `occupied` for EOF tail.
            let to_read = if do_eof_flush { occupied } else { chunk_size };
            let mut read = 0;
            while read < to_read {
                let n = consumer.pop_slice(&mut buffer[read..to_read]);
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
                      raw_consumer.is_some(), 
                      raw_consumer.as_ref().map(|c| c.occupied_len()).unwrap_or(0), 
                      chunk_size);
            if let Some(ref mut raw_cons) = raw_consumer {
                let raw_occupied = raw_cons.occupied_len();

                let do_eof_flush_raw = raw_occupied > 0
                    && raw_occupied < chunk_size
                    && stream_ended.load(Ordering::Acquire)
                    && !already_flushed_raw;

                if raw_occupied >= chunk_size {
                    // Normal path: full chunk available.
                    let mut raw_read = 0;
                    while raw_read < chunk_size {
                        let n = raw_cons.pop_slice(&mut raw_buffer[raw_read..chunk_size]);
                        if n == 0 { break; }
                        raw_read += n;
                    }
                    spectrum_before = analyzer_before.compute(&raw_buffer[..chunk_size], ch);
                } else if do_eof_flush_raw {
                    // EOF tail: drain leftover, zero-pad, compute ONCE.
                    let mut raw_read = 0;
                    while raw_read < raw_occupied {
                        let n = raw_cons.pop_slice(&mut raw_buffer[raw_read..raw_occupied]);
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

            let position_ms = *pos_mutex.lock().unwrap_or_else(|e| e.into_inner());

            // Temporary [BIN-DUMP] trap
            if position_ms % 2000 < 50 {
                let fmt = |v: &[f32]| -> String {
                    v.iter().take(10).map(|x| format!("{:.1}", x)).collect::<Vec<_>>().join(",")
                };
                eprintln!("[BIN-DUMP] before[0..10]=[{}]", fmt(&spectrum_before));
            }

            let frame = lineos_types::telemetry::RealtimeFrame {
                spectrum_before,
                spectrum_after,
                gonio_path: crate::player::decimate_gonio(&buffer[..chunk_size], ch),
                position_ms,
            };

            sender.send_frame(&frame);

            // Exit the loop once both buffers have been flushed.
            // raw_done is true when there is no raw consumer, or it has already been flushed.
            let raw_done = raw_consumer.is_none() || already_flushed_raw;
            if already_flushed && raw_done {
                break;
            }
        }
    });
}
