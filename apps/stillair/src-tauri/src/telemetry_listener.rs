//! UDP listener for real-time telemetry from xaak (m0-daemon).
//! Authority: telemetry-bridge-spec-v1_3.md TB-P4
//!
//! Listens on 127.0.0.1:9000.
//! Decodes RealtimeFrame (bincode, 520 bytes).
//! Always overwrites latest frame — consumer gets freshest data.
//!
//! INV-TB-3: always overwrites latest frame
//! INV-TB-4: listener thread never blocks audio thread

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use lineos_types::RealtimeFrame;

/// Shared state holding the latest RealtimeFrame from xaak.
/// None if xaak has not sent any frame yet.
pub type LatestFrame = Arc<Mutex<Option<RealtimeFrame>>>;

/// Spawn background UDP listener thread on 127.0.0.1:9000.
/// Non-blocking — runs independently of Tauri event loop.
/// Overwrites latest frame on every received UDP packet.
pub fn spawn_udp_listener(latest: LatestFrame) {
    std::thread::spawn(move || {
        let socket = match UdpSocket::bind("127.0.0.1:9000") {
            Ok(s)  => s,
            Err(e) => {
                tracing::error!("TB-P4: cannot bind UDP :9000 — {e}");
                return;
            }
        };

        // Blocking recv — thread sleeps until packet arrives
        socket.set_nonblocking(false).ok();

        let mut buf = [0u8; 1024]; // larger than max frame (520 bytes)

        loop {
            match socket.recv_from(&mut buf) {
                Err(e) => {
                    tracing::warn!("TB-P4: UDP recv error: {e}");
                    continue;
                }
                Ok((len, _addr)) => {
                    // Decode RealtimeFrame from bincode bytes
                    match bincode::decode_from_slice::<RealtimeFrame, _>(
                        &buf[..len],
                        bincode::config::standard(),
                    ) {
                        Err(e) => {
                            tracing::warn!("TB-P4: bincode decode error: {e}");
                        }
                        Ok((frame, _)) => {
                            // Overwrite latest — INV-TB-3
                            if let Ok(mut lock) = latest.lock() {
                                *lock = Some(frame);
                            }
                        }
                    }
                }
            }
        }
    });
}
