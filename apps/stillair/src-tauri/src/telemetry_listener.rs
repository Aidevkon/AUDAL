//! UDP listener + local RealtimeFrame for src-tauri.
//! Authority: telemetry-bridge-spec-v1_3.md TB-P4
//!
//! Local RealtimeFrame duplicates xaak struct — same binary layout.
//! Uses bincode::Decode + serde::Serialize for Tauri IPC.
//! Fully decoupled from lineos-types.

use std::net::UdpSocket;
use std::sync::{Arc, Mutex};

/// Local copy of RealtimeFrame — same binary layout as xaak's.
/// bincode::Decode for UDP receive.
/// serde::Serialize for Tauri IPC to Dioxus.
#[derive(Debug, Clone, Copy, serde::Serialize, bincode::Decode)]
pub struct RealtimeFrame {
    #[serde(with = "serde_arrays")]
    pub spectrum: [f32; 64],
    pub gonio_path: [(f32, f32); 32],
    pub position_ms: u64,
}

pub type LatestFrame = Arc<Mutex<Option<RealtimeFrame>>>;

pub fn spawn_udp_listener(latest: LatestFrame) {
    std::thread::spawn(move || {
        let socket = match UdpSocket::bind("127.0.0.1:9000") {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("TB-P4: cannot bind UDP :9000 — {e}");
                return;
            }
        };
        socket.set_nonblocking(false).ok();

        let mut buf = [0u8; 1024];

        loop {
            match socket.recv_from(&mut buf) {
                Err(e) => {
                    tracing::warn!("TB-P4: UDP recv error: {e}");
                }
                Ok((len, _)) => {
                    if let Ok((frame, _)) = bincode::decode_from_slice::<RealtimeFrame, _>(
                        &buf[..len],
                        bincode::config::standard(),
                    ) {
                        if let Ok(mut lock) = latest.lock() {
                            *lock = Some(frame);
                        }
                    }
                }
            }
        }
    });
}
