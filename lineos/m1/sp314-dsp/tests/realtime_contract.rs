#![cfg(feature = "cli")]
// tests/realtime_contract.rs

use ringbuf::HeapRb;
use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;
use sp314_dsp::realtime::engine_thread::spawn_engine_thread;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[test]
fn process_block_matches_process_offline() {
    let target = MasteringTarget::SpotifyV3;
    let config1 = target.engine_config(48000);
    let config2 = target.engine_config(48000);

    let mut engine_block = Sp314MasteringEngine::new(config1, 48000).unwrap();
    let mut engine_offline = Sp314MasteringEngine::new(config2, 48000).unwrap();

    let len = 512;
    use sp314_dsp::limiter::LOOKAHEAD_SAMPLES;
    let block_len = len + LOOKAHEAD_SAMPLES;
    let mut left_block = vec![0.0_f32; block_len];
    let mut right_block = vec![0.0_f32; block_len];
    let mut left_offline = vec![0.0_f32; len];
    let mut right_offline = vec![0.0_f32; len];

    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.1;
        left_block[i] = s;
        right_block[i] = -s;
        left_offline[i] = s;
        right_offline[i] = -s;
    }

    engine_block.process_block(&mut left_block, &mut right_block);
    let _ = engine_offline.process_offline(&mut left_offline, &mut right_offline);

    // In real-time mode, latency is kept in the buffer. In offline mode, the latency is shifted out.
    // Also, process_offline flushes the limiter with raw zeros (bypassing EQ/Comp),
    // whereas process_block processes the trailing zeros through the entire chain.
    // Therefore, they are only bit-identical for the frames where they process the exact same inputs
    // and have the exact same lookahead context.
    let valid_len = len - sp314_dsp::limiter::LOOKAHEAD_SAMPLES;
    for i in 0..valid_len {
        assert_eq!(
            left_block[i + sp314_dsp::limiter::LOOKAHEAD_SAMPLES],
            left_offline[i],
            "Left channel mismatch at frame {}",
            i
        );
        assert_eq!(
            right_block[i + sp314_dsp::limiter::LOOKAHEAD_SAMPLES],
            right_offline[i],
            "Right channel mismatch at frame {}",
            i
        );
    }
}

#[test]
fn engine_thread_processes_without_glitch() {
    let target = MasteringTarget::SpotifyV3;
    let config = target.engine_config(48000);
    let engine = Sp314MasteringEngine::new(config, 48000).unwrap();

    let rb_in = HeapRb::<f32>::new(4096 * 2);
    let (mut input_prod, input_cons) = rb_in.split();

    let rb_out = HeapRb::<f32>::new(4096 * 2);
    let (output_prod, output_cons) = rb_out.split();

    let (handle, stop_signal) = spawn_engine_thread(engine, input_cons, output_prod);

    // Write 4 blocks (2048 stereo frames = 4096 samples)
    let len = 2048;
    let mut interleaved_in = vec![0.0_f32; len * 2];
    for i in 0..len {
        let t = i as f32 / 48000.0;
        let s = (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.5;
        interleaved_in[i * 2] = s;
        interleaved_in[i * 2 + 1] = -s;
    }
    input_prod.push_slice(&interleaved_in);

    // Wait 100ms for processing
    std::thread::sleep(Duration::from_millis(100));

    // Assert: output buffer contains >= 2048 stereo frames
    assert!(
        output_cons.len() >= len * 2,
        "Engine did not keep up: expected >= {}, got {}",
        len * 2,
        output_cons.len()
    );

    // Stop and join
    stop_signal.store(true, Ordering::SeqCst);
    handle.join().unwrap();
}
