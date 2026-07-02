use aether_bridge::{build_dsp_config, AetherRequest};
use lineos_types::analysis::StemFeatures;
use lineos_types::pre_analysis::PreAnalysisData;

use aether::semantic::zone::EqSource;
const MIN_GAIN_DB: f32 = 0.1;

fn podcast_req() -> AetherRequest {
    AetherRequest {
        preset_name: Some("podcast".to_string()),
        project_id: Some("test".to_string()),
        track_id: Some("t1".to_string()),
        ..Default::default()
    }
}

fn stem_features() -> StemFeatures {
    StemFeatures {
        bass: Default::default(),
        harmonics: Default::default(),
        voice: Default::default(),
        drums: Default::default(),
        ambience: Default::default(),
        mix: Default::default(),
    }
}

const LTASS_TARGET: [f32; 8] = [-10.41, 4.01, 6.65, 6.22, -0.98, -5.49, -9.49, -13.49];
const REF_LEVEL: f32 = -22.0;

fn ltass_input(offsets: [f32; 8]) -> PreAnalysisData {
    let mut pa = PreAnalysisData::silent();
    for k in 0..8 {
        pa.spectral_profile_db[k] = LTASS_TARGET[k] + REF_LEVEL + offsets[k];
    }
    pa.integrated_lufs = REF_LEVEL;
    pa.true_peak_dbtp = REF_LEVEL + 10.0;
    pa
}

fn balanced_input() -> PreAnalysisData {
    ltass_input([0.0; 8])
}

fn muddy_input() -> PreAnalysisData {
    ltass_input([0.0, 6.0, 6.0, 6.0, 0.0, 0.0, 0.0, 0.0])
}

fn thin_input() -> PreAnalysisData {
    ltass_input([0.0, -6.0, -6.0, -6.0, 0.0, 0.0, 0.0, 0.0])
}

/// TEST #1 — Balanced input → near-zero gains.
/// The core objective: a podcast that already
/// matches the LTASS reference must be left almost
/// untouched. Proves the resolver does not "correct"
/// what is already correct.
#[test]
fn balanced_input_yields_near_zero_gains() {
    let (dsp_config, _, _) =
        build_dsp_config(&podcast_req(), &stem_features(), Some(&balanced_input()))
            .expect("build_dsp_config failed");

    let ref_bands: Vec<_> = dsp_config
        .eq
        .zone_bands
        .iter()
        .filter(|b| b.source == EqSource::Reference)
        .collect();

    // Near-zero: any band that survived the 0.1 dB
    // filter must still be small. Ideally the list
    // is empty (all gains below threshold).
    for band in &ref_bands {
        assert!(
            band.gain_db.abs() < 1.0,
            "Balanced (LTASS) input should need \
             ~no correction, but band {:.0} Hz got \
             {:.3} dB. The resolver is 'fixing' an \
             already-correct podcast.",
            band.center_hz,
            band.gain_db
        );
    }
}

/// TEST #2 — Clamp holds on extreme deviation.
/// A wildly off input still produces gains bounded
/// by G_MAX (resolver) and CFW (firewall).
#[test]
fn clamp_gains_bounded_by_g_max_and_firewall() {
    const G_MAX_DB: f32 = 6.0;
    const CFW_MAX_DB: f32 = 12.0;

    // Extreme: +30 dB dumped on the low end.
    let pa = ltass_input([0.0, 30.0, 30.0, 30.0, 0.0, 0.0, 0.0, 0.0]);

    let (dsp_config, _, _) = build_dsp_config(&podcast_req(), &stem_features(), Some(&pa))
        .expect("build_dsp_config failed");

    for band in dsp_config
        .eq
        .zone_bands
        .iter()
        .filter(|b| b.source == EqSource::Reference)
    {
        assert!(
            band.gain_db.abs() <= G_MAX_DB + 1e-3,
            "Band {:.0} Hz: |{:.3}| dB > G_MAX={}.",
            band.center_hz,
            band.gain_db,
            G_MAX_DB
        );
        assert!(
            band.gain_db.abs() <= CFW_MAX_DB,
            "Band {:.0} Hz: |{:.3}| dB > CFW={}.",
            band.center_hz,
            band.gain_db,
            CFW_MAX_DB
        );
    }
}

/// TEST #3 — Muddy input → CUT the low end.
/// A boomy podcast (LTASS + 6 dB on Bass/LowMid/
/// Mid-Low) must be corrected downward there.
/// Objective: real muddy podcast → correct decision.
#[test]
fn muddy_input_cuts_low_end() {
    let (dsp_config, _, _) =
        build_dsp_config(&podcast_req(), &stem_features(), Some(&muddy_input()))
            .expect("build_dsp_config failed");

    let ref_bands: Vec<_> = dsp_config
        .eq
        .zone_bands
        .iter()
        .filter(|b| b.source == EqSource::Reference)
        .collect();

    assert!(
        !ref_bands.is_empty(),
        "Muddy input produced no corrections."
    );

    // Bass (150 Hz) must be cut.
    let bass = ref_bands
        .iter()
        .find(|b| (b.center_hz - 150.0).abs() < 1.0)
        .expect("no 150 Hz band for muddy input");
    assert!(
        bass.gain_db < -MIN_GAIN_DB,
        "Muddy Bass should be CUT. Got: {:.3} dB",
        bass.gain_db
    );
}

/// TEST #4 — Thin input → BOOST the low end.
/// A thin/harsh podcast (LTASS with lows pulled
/// down) must be corrected upward in the lows.
#[test]
fn thin_input_boosts_low_end() {
    let (dsp_config, _, _) =
        build_dsp_config(&podcast_req(), &stem_features(), Some(&thin_input()))
            .expect("build_dsp_config failed");

    let bass = dsp_config
        .eq
        .zone_bands
        .iter()
        .filter(|b| b.source == EqSource::Reference)
        .find(|b| (b.center_hz - 150.0).abs() < 1.0)
        .expect("no 150 Hz band for thin input");

    assert!(
        bass.gain_db > MIN_GAIN_DB,
        "Thin Bass should be BOOSTED. Got: {:.3} dB",
        bass.gain_db
    );
}
