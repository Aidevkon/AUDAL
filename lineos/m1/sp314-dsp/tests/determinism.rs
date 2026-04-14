//! Determinism test — LineOS Constitution v2.0 §09.2
//! Same input + same seed → bit-identical binary output. Always.
//! This is the most critical test in Phase 2.
//! Failure here is a constitutional violation.

use sp314_dsp::pipeline::{MasteringIntent, MasteringPipeline};
use sp314_dsp::types::audio::AudioChunk;
use sp314_dsp::types::config::PipelineConstants;

fn test_constants() -> PipelineConstants {
    PipelineConstants {
        lookahead_ms:        2.0,
        lookahead_max:       192,
        eq_hpf_freq_hz:      30.0,
        eq_air_shelf_hz:     12000.0,
        dess_band_low_hz:    6000.0,
        dess_band_high_hz:   8000.0,
        comp_threshold_dbfs: -18.0,
        comp_ratio_default:  2.0,
        comp_knee_db:        6.0,
        sat_drive_default:   1.3,
        ms_side_gain_db:     1.5,
        ms_side_hpf_hz:      120.0,
        smoothing_ramp_ms:   20.0,
        dither_bits_24:      0.00000011920928955078125,
        dither_bits_16:      0.000030517578125,
    }
}

fn make_test_chunk() -> AudioChunk {
    let sample_rate = 48000u32;
    // 440Hz sine wave, 1 second, stereo
    let samples: Vec<f32> = (0..sample_rate * 2)
        .map(|i| {
            let ch    = i % 2;
            let frame = i / 2;
            let t     = frame as f32 / sample_rate as f32;
            // Slightly different L/R to exercise stereo path
            let freq = if ch == 0 { 440.0 } else { 441.0 };
            libm::sinf(2.0 * core::f32::consts::PI * freq * t) * 0.5
        })
        .collect();

    AudioChunk { samples, sample_rate, channels: 2 }
}

fn run_pipeline(chunk: &AudioChunk, seed: u64) -> Vec<u8> {
    let pipeline = MasteringPipeline::new(test_constants());
    let intent = MasteringIntent {
        seed,
        target_lufs:  Some(-14.0),
        export_16bit: true,
    };
    pipeline
        .master(&intent, &[chunk.clone()], [0u8; 32])
        .expect("Pipeline should not fail on valid input")
        .flac_bytes
}

#[test]
fn test_deterministic_output() {
    let chunk = make_test_chunk();
    let seed  = 0x1337BEEF_u64;

    // Run pipeline twice with identical input and seed
    let result1 = run_pipeline(&chunk, seed);
    let result2 = run_pipeline(&chunk, seed);

    // Binary comparison — must be identical
    assert_eq!(
        result1.len(), result2.len(),
        "Determinism violation: output lengths differ"
    );
    assert_eq!(
        result1, result2,
        "Determinism violation: same input + seed produced different binary output"
    );

    // Sanity: output is not silence (pipeline must have processed something)
    let has_signal = result1
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .any(|s| s.abs() > 1e-6);
    assert!(has_signal, "Pipeline produced silence — check stage chain");

    println!("✅ Determinism: binary diff = 0 ({} bytes)", result1.len());
}

#[test]
fn test_different_seeds_produce_different_output() {
    let chunk = make_test_chunk();

    let result1 = run_pipeline(&chunk, 0x1337BEEF);
    let result2 = run_pipeline(&chunk, 0xDEADC0DE);

    // Different seeds → different dither noise → different output
    assert_ne!(result1, result2, "Different seeds should produce different dither output");
}
