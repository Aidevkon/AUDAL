// src/bin/sp314_master.rs

use sp314_dsp::io::{FlacWriter, WavReader, WavWriter};
use sp314_dsp::metering::measure_integrated_lufs;
use sp314_dsp::pipeline::autotune::autotune;
use sp314_dsp::pipeline::engine::Sp314MasteringEngine;
use sp314_dsp::pipeline::presets::MasteringTarget;
use std::env;

fn mastered_path(input: &str, ext: &str) -> String {
    if let Some(pos) = input.rfind(".wav") {
        let (base, _ext) = input.split_at(pos);
        format!("{}_mastered.{}", base, ext)
    } else if let Some(pos) = input.rfind(".WAV") {
        let (base, _ext) = input.split_at(pos);
        format!("{}_mastered.{}", base, ext)
    } else {
        format!("{}_mastered.{}", input, ext)
    }
}

fn print_usage_and_exit() -> ! {
    println!("sp314-dsp v3.0.0 — Offline Mastering");
    println!("Usage: sp314_master <input.wav> [preset] [format]");
    println!("\nAvailable presets:");
    println!("  spotify  (default) - Streaming standard (-14 LUFS)");
    println!("  podcast            - Voice clarity (-16 LUFS)");
    println!("  edm                - Maximum density (-7 LUFS)");
    println!("\nAvailable formats:");
    println!("  flac     (default) - 24-bit integer FLAC (lossless)");
    println!("  wav                - 32-bit float WAV");
    std::process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage_and_exit();
    }

    let input_path = &args[1];
    let preset_arg = if args.len() > 2 {
        args[2].as_str()
    } else {
        "spotify"
    };
    let format_arg = if args.len() > 3 {
        args[3].as_str()
    } else {
        "flac"
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

    let (ext, is_flac) = match format_arg {
        "flac" => ("flac", true),
        "wav" => ("wav", false),
        _ => {
            println!("Error: Unknown format '{}'", format_arg);
            print_usage_and_exit();
        }
    };

    let output_path = mastered_path(input_path, ext);

    let mut decoded = WavReader::read(input_path).unwrap_or_else(|e| {
        println!("Error reading WAV file: {}", e);
        std::process::exit(1);
    });

    let mins = (decoded.duration_secs / 60.0) as u32;
    let secs = (decoded.duration_secs % 60.0) as u32;
    let channels_str = if decoded.num_channels == 1 {
        "mono"
    } else {
        "stereo"
    };

    let format_str = if is_flac {
        "FLAC 24-bit (lossless)"
    } else {
        "WAV 32-bit (float)"
    };

    println!("sp314-dsp v3.0.0 — Offline Mastering");
    println!("─────────────────────────────────────");
    println!(
        "Input:    {} ({} Hz, {}, {}m {:02}s)",
        input_path, decoded.sample_rate, channels_str, mins, secs
    );
    println!("Preset:   {:?}", target);
    println!("Format:   {}", format_str);
    println!("Output:   {}", output_path);
    println!("\nProcessing...");

    let input_lufs = measure_integrated_lufs(&decoded.left, &decoded.right);

    let base_config = target.engine_config(decoded.sample_rate);

    // Autotune (from test_engine.rs)
    let autotune_result = autotune(input_lufs, target.target_lufs().unwrap_or(input_lufs));

    let mut tuned_config = base_config;
    tuned_config.target_makeup_db = autotune_result.pre_gain_db;

    let mut engine = Sp314MasteringEngine::new(tuned_config, decoded.sample_rate).unwrap();

    // Process
    engine.process_offline(&mut decoded.left, &mut decoded.right);

    let output_lufs = measure_integrated_lufs(&decoded.left, &decoded.right);

    let out_peak = decoded
        .left
        .iter()
        .chain(decoded.right.iter())
        .map(|s| s.abs())
        .fold(0.0_f32, f32::max);

    let out_peak_db = if out_peak < 1e-9 {
        -144.0
    } else {
        20.0 * out_peak.log10()
    };

    if is_flac {
        FlacWriter::write(
            &output_path,
            &decoded.left,
            &decoded.right,
            decoded.sample_rate,
        )
        .unwrap_or_else(|e| {
            println!("Error writing FLAC file: {}", e);
            std::process::exit(1);
        });
    } else {
        WavWriter::write(
            &output_path,
            &decoded.left,
            &decoded.right,
            decoded.sample_rate,
        )
        .unwrap_or_else(|e| {
            println!("Error writing WAV file: {}", e);
            std::process::exit(1);
        });
    }

    println!("\nInput LUFS:   {:.1} LUFS", input_lufs);
    println!("Output LUFS:  {:.1} LUFS", output_lufs);
    println!("True peak:    {:.2} dBFS", out_peak_db);
    println!("─────────────────────────────────────");
    println!("Done. Output written to {}", output_path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mastered_path_logic() {
        assert_eq!(mastered_path("song.wav", "flac"), "song_mastered.flac");
        assert_eq!(
            mastered_path("path/to/song.wav", "wav"),
            "path/to/song_mastered.wav"
        );
        assert_eq!(mastered_path("song.WAV", "flac"), "song_mastered.flac");
        assert_eq!(mastered_path("song", "wav"), "song_mastered.wav");
    }
}
