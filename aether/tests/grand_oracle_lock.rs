use aether::markov::ambience_v1::AmbienceMarkovStateClassifier;
use aether::markov::bass_v1::BassMarkovStateClassifier;
use aether::markov::drums_v1::DrumsMarkovStateClassifier;
use aether::markov::firewall::{
    IntegrationFirewall, BASS_BOUNDS, DRUMS_BOUNDS, HARMONICS_AMBIENCE_BOUNDS,
};
use aether::markov::harmonics_v1::HarmonicsMarkovStateClassifier;
use aether::markov::predictive::{InstrumentDeltas, PredictiveController};
use lineos_types::analysis::StemMetrics;

#[test]
fn test_grand_oracle_lock() {
    let drums_m = StemMetrics {
        rms_db: -20.0,
        crest_factor_db: 18.0,
        ..StemMetrics::default()
    };
    let bass_m = StemMetrics {
        rms_db: -25.0,
        crest_factor_db: 14.0,
        ..StemMetrics::default()
    };
    let harm_m = StemMetrics {
        rms_db: -18.0,
        crest_factor_db: 16.0,
        ..StemMetrics::default()
    };
    let amb_m = StemMetrics {
        rms_db: -22.0,
        crest_factor_db: 5.0,
        ..StemMetrics::default()
    };

    let drums_predicted = DrumsMarkovStateClassifier::predict_next(
        DrumsMarkovStateClassifier::classify_drums(&drums_m),
    );
    let bass_predicted =
        BassMarkovStateClassifier::predict_next(BassMarkovStateClassifier::classify_bass(&bass_m));
    let harm_predicted = HarmonicsMarkovStateClassifier::predict_next(
        HarmonicsMarkovStateClassifier::classify_harmonics(&harm_m),
    );
    let amb_predicted = AmbienceMarkovStateClassifier::predict_next(
        AmbienceMarkovStateClassifier::classify_ambience(&amb_m),
    );

    let final_deltas = InstrumentDeltas {
        drums: IntegrationFirewall::clamp_instrument_delta(
            PredictiveController::compute_drums_delta(drums_predicted),
            &DRUMS_BOUNDS,
        ),
        bass: IntegrationFirewall::clamp_instrument_delta(
            PredictiveController::compute_bass_delta(bass_predicted),
            &BASS_BOUNDS,
        ),
        harmonics: IntegrationFirewall::clamp_instrument_delta(
            PredictiveController::compute_harmonics_delta(harm_predicted),
            &HARMONICS_AMBIENCE_BOUNDS,
        ),
        ambience: IntegrationFirewall::clamp_instrument_delta(
            PredictiveController::compute_ambience_delta(amb_predicted),
            &HARMONICS_AMBIENCE_BOUNDS,
        ),
    };

    let oracle_snapshot = format!(
        "drums:({:.2},{:.2},{:.2})|bass:({:.2},{:.2},{:.2})|harm:({:.2})|amb:({:.2})",
        final_deltas.drums.comp_attack_ms,
        final_deltas.drums.comp_threshold_db,
        final_deltas.drums.eq_presence_db,
        final_deltas.bass.comp_attack_ms,
        final_deltas.bass.comp_threshold_db,
        final_deltas.bass.eq_presence_db,
        final_deltas.harmonics.eq_presence_db,
        final_deltas.ambience.eq_presence_db,
    );

    let expected_lock = "drums:(-5.00,0.00,0.00)|bass:(5.00,0.00,1.50)|harm:(2.00)|amb:(1.00)";

    assert_eq!(oracle_snapshot, expected_lock, "Got: {}", oracle_snapshot);
}
