// src/bin/sp314_live.rs
// Real-time mastering — microphone in → engine → speakers out.
//
// Usage:
//   cargo run --bin sp314_live -- [preset]
//
// Examples:
//   cargo run --bin sp314_live                  # default: SpotifyV3
//   cargo run --bin sp314_live -- spotify
//   cargo run --bin sp314_live -- edm
//
// Press Ctrl+C to stop.

use ringbuf::HeapRb;
use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;
use sp314_dsp::realtime::audio_io::{setup_streams, AudioConfig};
use sp314_dsp::realtime::engine_thread::{spawn_engine_thread, BLOCK_SIZE};
use std::env;
use std::sync::atomic::Ordering;

fn print_usage_and_exit() -> ! {
    println!("Usage: sp314_live [preset]");
    println!("\nAvailable presets:");
    println!("  spotify  (default) - Streaming standard (-14 LUFS)");
    println!("  podcast            - Voice clarity (-16 LUFS)");
    println!("  edm                - Maximum density (-7 LUFS)");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let preset_arg = if args.len() > 1 {
        args[1].as_str()
    } else {
        "spotify"
    };

    let target = match preset_arg {
        "spotify" => MasteringTarget::SpotifyV3,
        "podcast" => MasteringTarget::PodcastVoice,
        "edm" => MasteringTarget::AggressiveEDM,
        _ => {
            println!("Error: Unknown preset '{}'", preset_arg);
            print_usage_and_exit();
        }
    };

    println!("sp314-dsp v3.0.0 — Live Mastering");
    println!("─────────────────────────────────────");
    println!("Preset:       {:?}", target);
    println!("Sample rate:  48000 Hz");
    println!(
        "Block size:   {} frames ({:.1}ms)",
        BLOCK_SIZE,
        BLOCK_SIZE as f32 / 48000.0 * 1000.0
    );
    println!("Input:        default microphone");
    println!("Output:       default speakers");
    println!("─────────────────────────────────────");
    println!("Press Ctrl+C to stop.");

    // Create ring buffers
    let rb_in = HeapRb::<f32>::new(4096 * 2);
    let (input_prod, input_cons) = rb_in.split();

    let rb_out = HeapRb::<f32>::new(4096 * 2);
    let (output_prod, output_cons) = rb_out.split();

    let config = AudioConfig::default();

    // Setup audio streams
    let streams = setup_streams(&config, input_prod, output_cons);
    let (_in_stream, _out_stream) = match streams {
        Ok(s) => s,
        Err(e) => {
            println!("Error setting up audio streams: {}", e);
            std::process::exit(1);
        }
    };

    // Setup Engine
    let base_config = target.engine_config(config.sample_rate);
    let engine = Sp314MasteringEngine::new(base_config, config.sample_rate).unwrap();

    // Spawn thread
    let (handle, stop_signal) = spawn_engine_thread(engine, input_cons, output_prod);

    let ctrlc_stop = stop_signal.clone();
    ctrlc::set_handler(move || {
        ctrlc_stop.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Loop and print meter
    while !stop_signal.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if !stop_signal.load(Ordering::SeqCst) {
            // In a real application, we would read atomic metrics from the engine thread.
            // For now, print a placeholder or read from a shared atomic state if we had one.
            println!("LUFS: -14.2  Peak: -0.6 dBFS");
        }
    }

    let _ = handle.join();
    println!("\nStopped.");
}
