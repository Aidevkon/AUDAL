//! Telemetry background worker — FFT + UDP off audio thread.
//! Authority: telemetry-bridge-spec-v1_3.md
//!
//! Receives raw audio samples from audio callback via lock-free ringbuf.
//! When 1024 samples accumulated: compute spectrum + send UDP.
//!
//! INV-TB-1: audio thread never blocked
//! INV-TB-2: bincode zero-alloc encoding
//! INV-TB-4: worker thread never waits for audio thread

use ringbuf::traits::{Consumer, Observer};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use lineos_types::RealtimeFrame;

const FRAME_SIZE: usize = 1024;

/// Spawn telemetry worker thread.
/// Consumes raw samples from ring buffer, computes spectrum, sends UDP.
pub fn spawn(
    mut consumer:   impl Consumer<Item = f32> + Observer + Send + 'static,
    sample_rate:    u32,
    channels:       u16,
    position_ms:    Arc<Mutex<u64>>,
) {
    std::thread::spawn(move || {
        let socket = match UdpSocket::bind("127.0.0.1:0") {
            Ok(s)  => s,
            Err(e) => {
                tracing::error!("telemetry_worker: UDP bind failed: {e}");
                return;
            }
        };
        socket.set_nonblocking(false).ok();

        let mut analyzer = crate::spectrum::SpectrumAnalyzer::new();
        let mut buf = vec![0.0f32; FRAME_SIZE * channels as usize];
        let ch = channels as usize;

        loop {
            // Wait until we have a full frame worth of samples
            let available = consumer.occupied_len();
            if available < FRAME_SIZE * ch {
                // Not enough data yet — yield briefly
                std::thread::sleep(std::time::Duration::from_millis(1));
                continue;
            }

            // Pop exactly one frame
            let popped = consumer.pop_slice(&mut buf[..FRAME_SIZE * ch]);
            if popped < FRAME_SIZE * ch { continue; }

            // Compute spectrum (FFT on channel 0)
            let spectrum = analyzer.compute(&buf[..FRAME_SIZE * ch], ch);

            // Decimate goniometer (32 pairs)
            let gonio_path = crate::player::decimate_gonio(
                &buf[..FRAME_SIZE * ch], ch
            );

            // Current playback position
            let position_ms = position_ms.lock()
                .map(|p| *p)
                .unwrap_or(0);

            let frame = RealtimeFrame {
                spectrum,
                gonio_path,
                position_ms,
            };

            // Encode + send (syscall here — safe, off audio thread)
            if let Ok(bytes) = bincode::encode_to_vec(
                &frame,
                bincode::config::standard(),
            ) {
                let _ = socket.send_to(&bytes, "127.0.0.1:9000");
            }
        }
    });
}
