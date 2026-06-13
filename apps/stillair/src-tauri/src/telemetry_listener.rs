use lineos_types::telemetry::RealtimeFrame;
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;

pub struct LatestFrame(pub Arc<Mutex<Option<RealtimeFrame>>>);

pub fn spawn_udp_listener(latest: Arc<Mutex<Option<RealtimeFrame>>>) {
    thread::spawn(move || {
        let socket = match UdpSocket::bind("127.0.0.1:9000") {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to bind UDP listener to 127.0.0.1:9000: {}", e);
                return;
            }
        };

        let mut buffer = [0u8; 1024];
        let config = bincode::config::standard();

        loop {
            match socket.recv(&mut buffer) {
                Ok(size) => {
                    // On success: bincode::decode_from_slice -> overwrite Mutex (INV-TB-3)
                    if let Ok((frame, _)) = bincode::decode_from_slice::<RealtimeFrame, _>(&buffer[..size], config) {
                        if let Ok(mut lock) = latest.lock() {
                            *lock = Some(frame);
                        }
                    }
                }
                Err(_) => {
                    // Silently continue (INV-TB-4)
                    continue;
                }
            }
        }
    });
}
