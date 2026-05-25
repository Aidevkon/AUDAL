// src/realtime/engine_thread.rs
// Engine processing loop — runs on a dedicated thread.
// Reads from input ring buffer, processes blocks, writes to output ring buffer.

use crate::pipeline::engine::Sp314MasteringEngine;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub const BLOCK_SIZE: usize = 512;

/// Spawn the engine processing thread.
/// Returns a JoinHandle and a stop signal sender.
pub fn spawn_engine_thread(
    mut engine: Sp314MasteringEngine,
    mut input_consumer: ringbuf::HeapConsumer<f32>,
    mut output_producer: ringbuf::HeapProducer<f32>,
) -> (thread::JoinHandle<()>, Arc<AtomicBool>) {
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_signal_clone = stop_signal.clone();

    let handle = thread::spawn(move || {
        let mut left = vec![0.0_f32; BLOCK_SIZE];
        let mut right = vec![0.0_f32; BLOCK_SIZE];
        let mut interleaved_in = vec![0.0_f32; BLOCK_SIZE * 2];
        let mut interleaved_out = vec![0.0_f32; BLOCK_SIZE * 2];

        while !stop_signal_clone.load(Ordering::SeqCst) {
            // 2. Try to read BLOCK_SIZE * 2 samples
            if input_consumer.len() >= BLOCK_SIZE * 2 {
                input_consumer.pop_slice(&mut interleaved_in);

                // 4. Deinterleave
                for i in 0..BLOCK_SIZE {
                    left[i] = interleaved_in[i * 2];
                    right[i] = interleaved_in[i * 2 + 1];
                }

                // 5. Call engine.process_block
                engine.process_block(&mut left, &mut right);

                // 6. Interleave output and write
                for i in 0..BLOCK_SIZE {
                    interleaved_out[i * 2] = left[i];
                    interleaved_out[i * 2 + 1] = right[i];
                }

                // Write to output ring buffer. Wait if full.
                while output_producer.free_len() < BLOCK_SIZE * 2 {
                    if stop_signal_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(std::time::Duration::from_millis(1));
                }
                
                output_producer.push_slice(&interleaved_out);
            } else {
                // 3. Not enough samples yet — sleep 1ms and retry
                thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    });

    (handle, stop_signal)
}
