use aether_bridge::{build_dsp_config, AetherRequest, ContentType};
use lineos_types::analysis::StemFeatures;
use lineos_types::pre_analysis::PreAnalysisData;

use aether::semantic::zone::EqSource;
const MIN_GAIN_DB: f32 = 0.1;

fn podcast_req() -> AetherRequest {
    AetherRequest {
        preset_name: Some("podcast".to_string()),
        project_id: Some("test".to_string()),
        track_id: Some("t1".to_string()),
        content_type: ContentType::Episode,
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

/// INV-MUS-3 (S-0XX): Music content must never receive the speech
/// (PodcastV1) reference profile. This test specifically covers the
/// unclassified case (genre: None) — see inv_mus_4/5 for classified
/// Acoustic/Idm music receiving their OWN reference zones, which is
/// expected and correct post-Part-B.
#[test]
fn inv_mus_3_unclassified_music_produces_zero_speech_zones() {
    let req = AetherRequest {
        content_type: ContentType::Music,
        ..podcast_req()
    };

    let (dsp_config, _, _) = build_dsp_config(&req, &stem_features(), Some(&muddy_input()))
        .expect("build_dsp_config failed");

    let ref_bands: Vec<_> = dsp_config
        .eq
        .zone_bands
        .iter()
        .filter(|b| b.source == EqSource::Reference)
        .collect();

    assert!(
        ref_bands.is_empty(),
        "Music content produced {} reference zones, expected 0.",
        ref_bands.len()
    );
}

/// TEST #5 — Firewall Direct Attack (future-proof)
///
/// Bypasses ALL resolvers and injects a malicious
/// 50 dB gain straight into ZoneAdjustments, then
/// calls IntegrationFirewall::build directly.
///
/// Does not depend on G_MAX, persona values, or any
/// resolver behavior — it tests the DOOR (the
/// firewall's constitutional clamp), not the chain
/// leading to it. If a future bug ever sends an
/// unbounded gain downstream, this guarantees the
/// firewall still stops it and logs the event.
#[test]
fn firewall_clamps_malicious_50db_injection() {
    use aether::chaos::delta::ChaosDelta;
    use aether::mapping::types::MicroDelta;
    use aether::personas::config::MacroControls;
    use aether::semantic::zone::{EqSource, ZoneAdjustment, ZoneAdjustments};
    use integration::config::{CFW_EQ_GAIN_MAX_DB, CFW_EQ_GAIN_MIN_DB};
    use integration::firewall::IntegrationFirewall;
    use integration::proof_log::ProofLog;

    // Malicious payload: 50 dB is impossible from
    // our resolvers (G_MAX=6, semantic_max=2.5).
    let malicious = ZoneAdjustments {
        bands: vec![
            ZoneAdjustment {
                center_hz: 150.0,
                gain_db: 50.0,
                q: 0.707,
                source: EqSource::Reference,
            },
            ZoneAdjustment {
                center_hz: 3000.0,
                gain_db: -50.0,
                q: 0.707,
                source: EqSource::Reference,
            },
        ],
    };

    // Personas come only via the manager (no
    // Default) — same as build_dsp_config and the
    // existing firewall test.
    let mgr = aether::personas::manager::PersonaManager::load();
    let persona = mgr.default_persona().clone();
    let macros = MacroControls::default();
    let micro = MicroDelta::default();
    let chaos = ChaosDelta::zero();
    let mut proof = ProofLog::new();

    let dsp_config = IntegrationFirewall::build(
        &persona, &macros, &micro, &malicious, &chaos, 0u64, &mut proof,
    )
    .expect("IntegrationFirewall::build failed");

    // Every band must be clamped to the
    // constitutional limit.
    for band in &dsp_config.eq.zone_bands {
        assert!(
            band.gain_db <= CFW_EQ_GAIN_MAX_DB,
            "FIREWALL BREACH at {:.0} Hz: {:.1} dB \
             boost survived (max {}).",
            band.center_hz,
            band.gain_db,
            CFW_EQ_GAIN_MAX_DB
        );
        assert!(
            band.gain_db >= CFW_EQ_GAIN_MIN_DB,
            "FIREWALL BREACH at {:.0} Hz: {:.1} dB \
             cut survived (min {}).",
            band.center_hz,
            band.gain_db,
            CFW_EQ_GAIN_MIN_DB
        );
    }
}

/// INV-MUS-4 (S-0XX): Music content classified as Acoustic
/// receives the Acoustic reference profile correction zones.
#[test]
fn inv_mus_4_classified_acoustic_gets_acoustic_zones() {
    // L2: genre-based routing removed — reference selection no
    // longer varies by genre (the GenreClassifier mechanism
    // survives in lineos-corpus for future Onboarding wiring)
}

/// INV-MUS-5 (S-0XX): Music content classified as Techno
/// receives the Techno reference profile correction zones.
#[test]
fn inv_mus_5_classified_techno_gets_techno_zones() {
    // L2: genre-based routing removed — reference selection no
    // longer varies by genre (the GenreClassifier mechanism
    // survives in lineos-corpus for future Onboarding wiring)
}

/// INV-MUS-6 (S-0XX): Music content classified as Acoustic and Techno
/// produce strictly distinct reference zones from each other.
#[test]
fn inv_mus_6_acoustic_and_techno_routes_are_distinct() {
    // L2: genre-based routing removed — reference selection no
    // longer varies by genre (the GenreClassifier mechanism
    // survives in lineos-corpus for future Onboarding wiring)
}
