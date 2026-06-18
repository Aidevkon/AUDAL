use ringbuf::traits::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub fn spawn<C>(mut consumer: C, mut raw_consumer: Option<C>, sample_rate: u32, channels: usize, pos_mutex: Arc<Mutex<u64>>)
where
    C: Consumer<Item = f32> + Observer + Send + 'static,
{
    std::thread::spawn(move || {
        eprintln!(
            "[WORKER] Thread spawned. CH: {}, SR: {}",
            channels, sample_rate
        );

        let sender = crate::telemetry::UdpTelemetrySender::new();

        let mut analyzer_after = crate::spectrum::SpectrumAnalyzer::new();
        let mut analyzer_before = crate::spectrum::SpectrumAnalyzer::new();
        
        let ch = channels.max(1);
        let chunk_size = 1024 * ch;
        // Use a fixed stack array to strictly eliminate heap allocations
        // 1024 frames * 2 channels max = 2048
        let mut buffer = [0.0f32; 2048];
        let mut raw_buffer = [0.0f32; 2048];
        let mut iter_count = 0;

        loop {
            let available = consumer.occupied_len();
            if available >= chunk_size {
                let slice = &mut buffer[..chunk_size];
                let _filled = consumer.pop_slice(slice);
                
                let mut spectrum_before = [-120.0f32; 64];
                if let Some(ref mut raw_cons) = raw_consumer {
                    let raw_available = raw_cons.occupied_len();
                    if raw_available >= chunk_size {
                        let raw_slice = &mut raw_buffer[..chunk_size];
                        let _raw_filled = raw_cons.pop_slice(raw_slice);
                        spectrum_before = analyzer_before.compute(raw_slice, ch);
                    }
                }

                let spectrum_after = analyzer_after.compute(&buffer[..chunk_size], ch);

                iter_count += 1;
                if iter_count <= 100 {
                    eprintln!("[WORKER] before[0..3]={:?} after[0..3]={:?}", &spectrum_before[0..3], &spectrum_after[0..3]);
                } else if iter_count % 50 == 0 {
                    eprintln!("[WORKER] Processed 50 chunks. before[0]={:.1}, after[0]={:.1}", spectrum_before[0], spectrum_after[0]);
                }

                let position_ms = *pos_mutex.lock().unwrap_or_else(|e| e.into_inner());

                let frame = lineos_types::telemetry::RealtimeFrame {
                    spectrum_before,
                    spectrum_after,
                    gonio_path: crate::player::decimate_gonio(&buffer[..chunk_size], ch),
                    position_ms,
                };

                sender.send_frame(&frame);
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    });
}
