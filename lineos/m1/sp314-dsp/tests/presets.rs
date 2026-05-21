//! Platform preset + validation unit tests — Sandbox step [2].

use sp314_dsp::pipeline::validate::validate_preset;
use sp314_dsp::types::mastering_preset::{
    CompressionStyle, MasteringPreset, APPLE_MUSIC, APPLE_PODCASTS, AMAZON, BROADCAST, RAW,
    SPOTIFY, TIDAL, YOUTUBE,
};

#[test]
fn validate_all_platform_presets_pass() {
    let presets = [
        SPOTIFY,
        YOUTUBE,
        APPLE_MUSIC,
        APPLE_PODCASTS,
        TIDAL,
        AMAZON,
        BROADCAST,
        RAW,
    ];
    for preset in presets {
        validate_preset(&preset)
            .unwrap_or_else(|e| panic!("platform preset should validate: {e}"));
    }
}

#[test]
fn validate_rejects_zero_seed_with_16bit_dither() {
    let bad = MasteringPreset {
        dither_bits: 16,
        dither_seed: 0,
        ..RAW
    };
    assert!(validate_preset(&bad).is_err());
}

#[test]
fn validate_rejects_invalid_dither_bits() {
    let bad = MasteringPreset {
        dither_bits: 15,
        ..SPOTIFY
    };
    assert!(validate_preset(&bad).is_err());
}

#[test]
fn validate_rejects_non_power_of_two_oversampling() {
    let bad = MasteringPreset {
        oversampling: 3,
        ..SPOTIFY
    };
    assert!(validate_preset(&bad).is_err());
}

#[test]
fn validate_accepts_raw_preset() {
    assert!(validate_preset(&RAW).is_ok());
    assert_eq!(RAW.dither_bits, 32);
    assert_eq!(RAW.dither_seed, 0);
}

#[test]
fn stereo_link_amount_all_styles() {
    assert_eq!(CompressionStyle::Transparent.stereo_link_amount(), 0.3);
    assert_eq!(CompressionStyle::Gentle.stereo_link_amount(), 0.5);
    assert_eq!(CompressionStyle::Medium.stereo_link_amount(), 0.7);
    assert_eq!(CompressionStyle::Aggressive.stereo_link_amount(), 1.0);
}

#[test]
fn time_to_coeff_aggressive_band0_attack_5ms_48khz() {
    // v2.9.1 §N5: 5 ms attack @ 48 kHz → α ≈ 0.004158 (RC formula).
    // Spec precomputed table lists 0.002900 for Aggressive Band 0 — table uses a different
    // reference; this test locks the N5 formula output.
    let coeff = CompressionStyle::time_to_coeff(5.0, 48_000.0);
    assert!(
        libm::fabsf(coeff - 0.004158) < 1e-4,
        "expected ~0.004158 from N5 formula, got {coeff}"
    );
}
