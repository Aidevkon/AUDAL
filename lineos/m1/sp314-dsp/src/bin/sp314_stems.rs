// src/bin/sp314_stems.rs
// E14 Five-Stem Separator CLI
// Usage: sp314_stems <input.wav>
// Output: <name>_bass.wav, <name>_harmonics.wav,
//         <name>_drums.wav, <name>_ambience.wav, <name>_voice.wav

use sp314_dsp::io::{WavReader, WavWriter};
use sp314_dsp::stft::stem_renderer::FiveStemRenderer;
use std::env;

fn stem_path(input: &str, stem: &str) -> String {
    let base = if let Some(pos) = input.rfind('.') {
        &input[..pos]
    } else {
        input
    };
    format!("{}_{}.wav", base, stem)
}

fn print_usage_and_exit() -> ! {
    println!("sp314-dsp E14 — Five-Stem Separator");
    println!("Usage: sp314_stems <input.wav>");
    println!();
    println!("Output files:");
    println!("  <name>_bass.wav");
    println!("  <name>_harmonics.wav");
    println!("  <name>_drums.wav");
    println!("  <name>_ambience.wav");
    println!("  <name>_voice.wav");
    std::process::exit(1);
}

fn write_wav(path: &str, samples: &[f32], sample_rate: u32) {
    // WavWriter::write takes left and right channels for stereo. We pass the mono stem to both.
    WavWriter::write(path, samples, samples, sample_rate).expect("Failed to write WAV file");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage_and_exit();
    }
    let input_path = &args[1];

    // Read input
    println!("=== E14 Five-Stem Separator ===");
    println!("Input: {}", input_path);

    let reader = WavReader::read(input_path).expect("Failed to open input WAV");

    let sample_rate = reader.sample_rate;
    let num_channels = reader.num_channels;

    println!("  Sample rate: {} Hz", sample_rate);
    println!("  Channels:    {}", num_channels);

    // Mix down to mono if stereo
    let mono: Vec<f32> = if num_channels == 2 {
        reader
            .left
            .iter()
            .zip(reader.right.iter())
            .map(|(l, r)| (*l + *r) * 0.5_f32)
            .collect()
    } else {
        reader.left
    };

    let duration = mono.len() as f32 / sample_rate as f32;
    println!("  Duration:    {:.1}s ({} samples)", duration, mono.len());

    // Run 5-stem separation
    println!("\nRunning stem separation...");
    println!("  STFT → HPSS → NMF (100 iter)");
    let t0 = std::time::Instant::now();

    let mut renderer = FiveStemRenderer::new();
    let stems = renderer.render(&mono);

    let elapsed = t0.elapsed().as_millis();
    println!("  Done in {}ms", elapsed);

    // Write output stems
    let bass_path = stem_path(input_path, "bass");
    let harmonics_path = stem_path(input_path, "harmonics");
    let drums_path = stem_path(input_path, "drums");
    let ambience_path = stem_path(input_path, "ambience");
    let voice_path = stem_path(input_path, "voice");

    println!("\nWriting stems:");
    write_wav(&bass_path, &stems.bass, sample_rate);
    println!("  ✓ {}", bass_path);
    write_wav(&harmonics_path, &stems.harmonics, sample_rate);
    println!("  ✓ {}", harmonics_path);
    write_wav(&drums_path, &stems.drums, sample_rate);
    println!("  ✓ {}", drums_path);
    write_wav(&ambience_path, &stems.ambience, sample_rate);
    println!("  ✓ {}", ambience_path);
    write_wav(&voice_path, &stems.voice, sample_rate);
    println!("  ✓ {}", voice_path);

    println!("\n=== Done ===");
}
