use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ringbuf::traits::*;

pub fn spawn<C>(
    mut consumer: C,
    sample_rate: u32,
    channels: usize,
    pos_mutex: Arc<Mutex<u64>>,
) where
    C: Consumer<Item = f32> + Observer + Send + 'static,
{
    std::thread::spawn(move || {
        eprintln!("[WORKER] Thread spawned. CH: {}, SR: {}", channels, sample_rate);

        let udp_tx = match UdpSocket::bind("127.0.0.1:0") {
            Ok(s) => {
                s.set_nonblocking(true).unwrap_or_default();
                eprintln!("[WORKER] UDP bound to {:?}", s.local_addr());
                Some(s)
            }
            Err(e) => {
                eprintln!("[WORKER] Failed to bind UDP: {}", e);
                None
            }
        };

        let mut analyzer = crate::spectrum::SpectrumAnalyzer::new();
        let ch = channels.max(1);
        let chunk_size = 1024 * ch;
        let mut buffer = Vec::with_capacity(chunk_size);
        let mut iter_count = 0;

        loop {
            let available = consumer.occupied_len();
            if available >= chunk_size {
                buffer.clear();
                for _ in 0..chunk_size {
                    if let Some(s) = consumer.try_pop() {
                        buffer.push(s);
                    }
                }
                
                iter_count += 1;
                if iter_count % 50 == 0 {
                    eprintln!("[WORKER] Processed 50 chunks ({} frames)", chunk_size);
                }

                let position_ms = *pos_mutex.lock().unwrap_or_else(|e| e.into_inner());
                
                let frame = lineos_types::RealtimeFrame {
                    spectrum:    analyzer.compute(&buffer, ch),
                    gonio_path:  crate::player::decimate_gonio(&buffer, ch),
                    position_ms,
                };

                if let Some(ref sock) = udp_tx {
                    match bincode::encode_to_vec(&frame, bincode::config::standard()) {
                        Ok(bytes) => {
                            if let Err(e) = sock.send_to(&bytes, "127.0.0.1:9000") {
                                if e.kind() != std::io::ErrorKind::WouldBlock {
                                    eprintln!("[WORKER] send_to error: {}", e);
                                }
                            }
                        }
                        Err(e) => eprintln!("[WORKER] Bincode error: {}", e),
                    }
                }
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    });
}
