//! telemetry.rs — UDP Telemetry Sender (Phase 9: TB-P3)
//! Authority: telemetry-bridge-spec-v1_3.md
//!
//! INV-TB-1: UDP fire-and-forget — sender never blocks audio thread
//! INV-TB-2: bincode — zero heap allocation per frame
//! INV-TB-5: RealtimeFrame is Copy — 520 bytes, no heap

use lineos_types::telemetry::RealtimeFrame;
use std::net::UdpSocket;

pub struct UdpTelemetrySender {
    socket: UdpSocket,
}

impl Default for UdpTelemetrySender {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpTelemetrySender {
    pub fn new() -> Self {
        // Bind to an ephemeral port locally
        let socket = UdpSocket::bind("127.0.0.1:0").expect("Failed to bind UDP telemetry socket");

        // Connect to the receiver so we can just use send()
        socket
            .connect("127.0.0.1:9000")
            .expect("Failed to connect UDP socket to telemetry listener");

        // Strictly set to non-blocking (INV-TB-1)
        socket
            .set_nonblocking(true)
            .expect("Failed to set non-blocking on telemetry UDP socket");

        Self { socket }
    }

    /// Sends a real-time frame via UDP.
    /// INV-TB-1: Fire-and-forget, ignores WouldBlock.
    /// INV-TB-2: Zero heap allocation.
    pub fn send_frame(&self, frame: &RealtimeFrame) {
        // INV-TB-2: zero heap allocation per frame (stack buffer)
        let mut buffer = [0u8; 1024];
        let config = bincode::config::standard();

        // Encode into the stack-allocated buffer (INV-TB-2)
        if let Ok(size) = bincode::encode_into_slice(frame, &mut buffer, config) {
            // Send the buffer via self.socket.send().
            // Silently ignore WouldBlock or other errors (INV-TB-1).
            let _ = self.socket.send(&buffer[..size]);
        }
    }
}
