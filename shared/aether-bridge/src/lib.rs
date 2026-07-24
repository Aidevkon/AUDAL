// shared/aether-bridge/src/lib.rs
pub mod reference_resolver;
// Authority: RFC-005 v0.3 FINAL
// Cross-layer orchestration bridge:
// lineos/m0/m0-daemon → shared/aether-bridge → aether/

use aether::chaos::engine::ChaosEngine;
use aether::mapping::mapper::MacroMicroMapper;
use aether::markov::ambience_v1::AmbienceMarkovStateClassifier;
use aether::markov::bass_v1::BassMarkovStateClassifier;
use aether::markov::chaos::ChaosLayer;
use aether::markov::drums_v1::DrumsMarkovStateClassifier;
use aether::markov::firewall::IntegrationFirewall as MarkovFirewall;
use aether::markov::firewall::{BASS_BOUNDS, DRUMS_BOUNDS, HARMONICS_AMBIENCE_BOUNDS};
use aether::markov::harmonics_v1::HarmonicsMarkovStateClassifier;
use aether::markov::predictive::InstrumentDeltas;
use aether::markov::predictive::PredictiveController;
use aether::markov::voice_v1::MarkovStateClassifier;
use aether::personas::config::{MacroControls, PersonaConfig};
use aether::personas::manager::PersonaManager;
use aether::semantic::resolver::SemanticZoneResolver;
use integration::config::DspConfig;
use integration::firewall::IntegrationFirewall;
use integration::proof_log::ProofLog;
use lineos_types::analysis::StemFeatures;
use lineos_types::pre_analysis::PreAnalysisData;
use proof::certificate::ExecutionCertificate;
use proof::proof::{CertificateContext, ExecutionProof};

/// Default is Music (reference correction skipped): unknown callers get no reference EQ rather than the wrong one. Episode must be explicit.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub enum ContentType {
    #[default]
    Music,
    Episode,
}

/// Aether tuning parameters from the caller.
/// Decoupled from MasterRequest (m0 network DTO).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AetherRequest {
    pub persona_id: Option<String>,
    pub tone: Option<f32>,
    pub dynamics: Option<f32>,
    pub ambience: Option<AmbienceIntent>,
    pub chaos_seed: Option<u64>,
    pub project_id: Option<String>,
    pub track_id: Option<String>,
    pub preset_name: Option<String>,
    #[serde(default)]
    pub content_type: ContentType,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AmbienceIntent {
    pub space: Option<f32>,
    pub width: Option<f32>,
    pub tone: Option<f32>,
    pub loudness: Option<f32>,
}

#[derive(Debug)]
pub enum AetherBridgeError {
    PersonaNotFound(String),
    FirewallError(String),
}

impl std::fmt::Display for AetherBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PersonaNotFound(id) => write!(f, "Persona not found: {}", id),
            Self::FirewallError(e) => write!(f, "Firewall error: {}", e),
        }
    }
}

/// Phase 1 of RFC-005 pipeline: Pre-DSP Aether processing.
/// Run BEFORE DSP render.
/// Returns: (DspConfig, ProofLog, PersonaConfig)
/// DspConfig is passed to sp314-dsp for rendering.
/// ProofLog is passed to generate_certificate() after render.
pub fn build_dsp_config(
    req: &AetherRequest,
    features: &StemFeatures,
    pre_analysis: Option<&PreAnalysisData>,
) -> Result<(DspConfig, ProofLog, PersonaConfig), AetherBridgeError> {
    let mgr = PersonaManager::load();
    let persona = mgr
        .get(req.persona_id.as_deref().unwrap_or("warm_analog"))
        .ok_or_else(|| {
            AetherBridgeError::PersonaNotFound(
                req.persona_id.clone().unwrap_or("warm_analog".into()),
            )
        })?
        .clone();

    let macros = MacroControls {
        tone: req.tone.unwrap_or(persona.macros.tone.default),
        dynamics: req.dynamics.unwrap_or(persona.macros.dynamics.default),
    };

    let micro = MacroMicroMapper::map(&persona, &macros);

    let seed = req.chaos_seed.unwrap_or_else(|| {
        ChaosEngine::build_seed(
            req.project_id.as_deref().unwrap_or("default"),
            req.track_id.as_deref().unwrap_or("default"),
            &persona.id,
        )
    });

    let mut chaos_engine = ChaosEngine::new(seed);
    let chaos_delta = chaos_engine.next_delta(persona.chaos_intensity, &persona.chaos);

    let modulated = ChaosEngine::apply(&micro, &chaos_delta);
    let mut zones = SemanticZoneResolver::auto_carve(&persona, features, pre_analysis);

    // ── Reference-Driven Spectral Correction ──
    // Closes the delta between the input's spectral
    // shape and the LTASS-sourced podcast reference
    // profile. Merged PRE-firewall so IntegrationFirewall
    // sees the full gain picture and can protect the
    // True Peak ceiling (INV-REF-1). Post-firewall
    // extend was rejected: it hides gains from the
    // firewall's headroom calculation.
    // spec §3.4 step 6 + §4 (resolver proposes,
    // firewall bounds).
    {
        // Geometric centers of the 8 analysis bands
        // (Sub, Bass, LowMid, MidLow, MidHigh,
        //  HighMid, Presence, Air).
        const REF_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
        // Q=0.707 ≈ 1 octave bandwidth — broad
        // shape correction, not surgical (that is
        // SemanticZoneResolver's job).
        const REF_Q: f32 = 0.707;
        // Threshold: ignore sub-audible corrections
        // (< 0.1 dB) to keep the zone pool clean.
        const MIN_GAIN_DB: f32 = 0.1;

        // pre_analysis is Option<&PreAnalysisData>;
        // skip reference correction if unavailable.
        let mut target_profile_id = None;

        if req.content_type == ContentType::Episode {
            target_profile_id = Some(crate::reference_resolver::ProfileId::PodcastV1);
        } else if req.content_type == ContentType::Music {
            // Unclassified music skips reference correction
        }

        if let Some(profile_id) = target_profile_id {
            if let Some(pa) = pre_analysis {
                // ── Signal normalization (mean-center) ──
                // The ReferenceProfile target is a relative
                // SHAPE, mean-subtracted over the 6 SPEECH
                // bands only (Sub..HighMid, 20 Hz–4 kHz) —
                // the bands carrying real Byrne LTASS data.
                // Bands 6–7 (Treble/Air) are synthetic
                // -4 dB/oct tilt extensions (Byrne stops at
                // 2.5 kHz) and are EXCLUDED from the mean:
                // the 6 measured targets sum to exactly 0.00,
                // and including the dark synthetic bands
                // would drag the mean down and corrupt the
                // speech normalization.
                //
                // The raw signal is absolute dBFS, so it must
                // be centered with the SAME 6-band mean
                // before comparison — otherwise absolute
                // levels meet a relative shape and every band
                // reads as full-boost (the bug this fixes).
                //
                // Verified: a pure-LTASS signal at any level
                // round-trips to ~0 gains (balanced test).
                //
                // If podcast-v1.json changes which bands carry
                // measured LTASS data, update normalization_band_count in podcast-v1.json.
                let profile = crate::reference_resolver::ReferenceProfile::load(profile_id);
                let n = profile.normalization_band_count;
                let speech_mean: f32 = pa.spectral_profile_db[..n].iter().sum::<f32>() / n as f32;
                let normalized_profile: [f32; 8] =
                    core::array::from_fn(|k| pa.spectral_profile_db[k] - speech_mean);

                let ref_gains = crate::reference_resolver::ReferenceResolver::resolve(
                    &normalized_profile,
                    &profile,
                );

                zones.bands.extend(
                    ref_gains
                        .iter()
                        .enumerate()
                        .filter(|(_, &g)| g.abs() > MIN_GAIN_DB)
                        .map(|(i, &g)| aether::semantic::zone::ZoneAdjustment {
                            center_hz: REF_CFS[i],
                            gain_db: g,
                            q: REF_Q,
                            source: aether::semantic::zone::EqSource::Reference,
                        }),
                );
            }
        }
    }

    let mut proof_log = ProofLog::new();
    let mut dsp_config = IntegrationFirewall::build(
        &persona,
        &macros,
        &modulated,
        &zones,
        &chaos_delta,
        seed,
        &mut proof_log,
    )
    .map_err(|e| AetherBridgeError::FirewallError(format!("{:?}", e)))?;

    if let Some(amb) = &req.ambience {
        let amb_macros = aether::mapping::types::AmbienceMacroControls {
            space: amb.space.unwrap_or(0.5),
            width: amb.width.unwrap_or(0.5),
            tone: amb.tone.unwrap_or(0.5),
            loudness: amb.loudness.unwrap_or(0.5),
        };
        let delta = aether::mapping::ambience::AmbienceMicroMapper::map(&amb_macros);
        dsp_config.ambience = Some(integration::config::DspAmbienceConfig {
            reverb_time_delta_s: delta.reverb_time_delta_s,
            pre_delay_delta_ms: delta.pre_delay_delta_ms,
            diffusion_delta: delta.diffusion_delta,
            high_shelf_gain_db: delta.high_shelf_gain_db,
            high_shelf_freq_delta: delta.high_shelf_freq_delta,
            low_shelf_cut_db: delta.low_shelf_cut_db,
            reverb_send_level: delta.reverb_send_level,
            decorrelation: delta.decorrelation,
            side_gain_db: delta.side_gain_db,
            phase_variance: delta.phase_variance,
            mono_comp_shelf_db: delta.mono_comp_shelf_db,
            hf_damping_db: delta.hf_damping_db,
            low_mid_cut_db: delta.low_mid_cut_db,
            tail_density_delta: delta.tail_density_delta,
            output_gain_db: delta.output_gain_db,
            hf_tail_cut_db: delta.hf_tail_cut_db,
        });
    }

    // Aether Black enrichment — INV-AB-9: if no voice features, skip (Mark III behavior)
    let voice_metrics = &features.voice;
    let current = MarkovStateClassifier::classify_voice(voice_metrics);
    let predicted = MarkovStateClassifier::predict_next(current);
    let raw_delta = PredictiveController::compute_voice_delta(current, predicted);

    // SIM-P2: Wire Simulation into PredictiveController
    if let Some(pre) = pre_analysis {
        let sim_delta = aether::simulation::SimulationLayer::run(pre, -14.0);
        let _predictive_delta = PredictiveController::evaluate(&raw_delta, &sim_delta);
        // Note: Map density_bias, ducking_hint, pre_gain_db to DSP fields if they exist.
        // For v1.4, they are computed but currently unused since DspConfig lacks them.
    }

    let chaos = ChaosLayer {
        bypass: false,
        seed: dsp_config.chaos_seed,
    };
    let modulated = chaos.modulate(raw_delta, dsp_config.chaos_seed);
    let md = MarkovFirewall::clamp_voice_delta(modulated);

    // Apply delta to baseline (enrichment, not replacement)
    dsp_config.dynamics.comp_threshold_db += md.comp_threshold_db;
    dsp_config.dynamics.comp_attack_ms += md.comp_attack_ms;
    dsp_config.dynamics.comp_release_ms += md.comp_release_ms;

    let drums_predicted = DrumsMarkovStateClassifier::predict_next(
        DrumsMarkovStateClassifier::classify_drums(&features.drums),
    );
    let bass_predicted = BassMarkovStateClassifier::predict_next(
        BassMarkovStateClassifier::classify_bass(&features.bass),
    );
    let harm_predicted = HarmonicsMarkovStateClassifier::predict_next(
        HarmonicsMarkovStateClassifier::classify_harmonics(&features.harmonics),
    );
    let amb_predicted = AmbienceMarkovStateClassifier::predict_next(
        AmbienceMarkovStateClassifier::classify_ambience(&features.ambience),
    );

    dsp_config.instrument_deltas = InstrumentDeltas {
        drums: MarkovFirewall::clamp_instrument_delta(
            PredictiveController::compute_drums_delta(drums_predicted),
            &DRUMS_BOUNDS,
        ),
        bass: MarkovFirewall::clamp_instrument_delta(
            PredictiveController::compute_bass_delta(bass_predicted),
            &BASS_BOUNDS,
        ),
        harmonics: MarkovFirewall::clamp_instrument_delta(
            PredictiveController::compute_harmonics_delta(harm_predicted),
            &HARMONICS_AMBIENCE_BOUNDS,
        ),
        ambience: MarkovFirewall::clamp_instrument_delta(
            PredictiveController::compute_ambience_delta(amb_predicted),
            &HARMONICS_AMBIENCE_BOUNDS,
        ),
    };

    Ok((dsp_config, proof_log, persona))
}

/// Phase 3 of RFC-005 pipeline: Post-DSP certificate generation.
/// Run AFTER DSP render with actual input + output PCM.
/// Cryptographically binds audio to DspConfig (S-010).
pub struct CertificateRequest<'a> {
    pub lra: f32,
    pub persona: &'a PersonaConfig,
    pub dsp_config: &'a DspConfig,
    pub proof_log: &'a ProofLog,
    pub req: &'a AetherRequest,
    pub system_version: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub fn generate_certificate(
    input_pcm_hash: String,
    output_pcm: &[f32],
    ctx: CertificateRequest,
) -> ExecutionCertificate {
    let rendered_at = chrono::Utc::now().to_rfc3339();
    ExecutionProof::generate(
        input_pcm_hash,
        output_pcm,
        CertificateContext {
            lra: ctx.lra,
            persona: ctx.persona,
            dsp_config: ctx.dsp_config,
            proof_log: ctx.proof_log,
            project_id: ctx.req.project_id.as_deref().unwrap_or("default"),
            track_id: ctx.req.track_id.as_deref().unwrap_or("default"),
            rendered_at: &rendered_at,
            system_version: ctx.system_version,
            preset_name: ctx.req.preset_name.as_deref().unwrap_or("default"),
        },
    )
}

/// Same as generate_certificate() but takes an
/// already-computed output PCM hash instead of
/// the full buffer. Used by the Episode streaming
/// path, which hashes its output incrementally
/// during render and never holds the whole buffer
/// in RAM. The certificate produced is identical
/// to the batch path for the same audio.
#[allow(clippy::too_many_arguments)]
pub fn generate_certificate_from_hash(
    input_pcm_hash: String,
    output_pcm_hash: String,
    ctx: CertificateRequest,
) -> ExecutionCertificate {
    let rendered_at = chrono::Utc::now().to_rfc3339();
    ExecutionProof::generate_from_hash(
        input_pcm_hash,
        output_pcm_hash,
        CertificateContext {
            lra: ctx.lra,
            persona: ctx.persona,
            dsp_config: ctx.dsp_config,
            proof_log: ctx.proof_log,
            project_id: ctx.req.project_id.as_deref().unwrap_or("default"),
            track_id: ctx.req.track_id.as_deref().unwrap_or("default"),
            rendered_at: &rendered_at,
            system_version: ctx.system_version,
            preset_name: ctx.req.preset_name.as_deref().unwrap_or("default"),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::{MixMetrics, StemFeatures, StemMetrics};

    fn test_features() -> StemFeatures {
        StemFeatures {
            bass: StemMetrics::default(),
            harmonics: StemMetrics::default(),
            drums: StemMetrics::default(),
            ambience: StemMetrics::default(),
            voice: StemMetrics::default(),
            mix: MixMetrics::default(),
        }
    }

    #[test]
    fn bridge_build_dsp_config_default_persona() {
        let req = AetherRequest {
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();
        let result = build_dsp_config(&req, &features, None);
        assert!(result.is_ok());
        let (cfg, _, persona) = result.unwrap();
        assert_eq!(persona.id, "warm_analog");
        assert!(cfg.stereo.width >= 0.5);
    }

    #[test]
    fn bridge_build_dsp_config_all_personas() {
        let features = test_features();
        for id in [
            "warm_analog",
            "clean_punch",
            "hybrid_hifi",
            "cinematic_wide",
        ] {
            let req = AetherRequest {
                persona_id: Some(id.into()),
                content_type: ContentType::Music,
                ..Default::default()
            };
            assert!(
                build_dsp_config(&req, &features, None).is_ok(),
                "Failed for persona: {}",
                id
            );
        }
    }

    #[test]
    fn bridge_unknown_persona_error() {
        let req = AetherRequest {
            persona_id: Some("nonexistent".into()),
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();
        assert!(matches!(
            build_dsp_config(&req, &features, None),
            Err(AetherBridgeError::PersonaNotFound(_))
        ));
    }

    #[test]
    fn bridge_deterministic() {
        let req = AetherRequest {
            chaos_seed: Some(42),
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();
        let (cfg1, _, _) = build_dsp_config(&req, &features, None).unwrap();
        let (cfg2, _, _) = build_dsp_config(&req, &features, None).unwrap();
        assert_eq!(cfg1.eq.low_shelf_gain_db, cfg2.eq.low_shelf_gain_db);
        assert_eq!(cfg1.stereo.width, cfg2.stereo.width);
    }

    #[test]
    fn bridge_certificate_generation() {
        let req = AetherRequest {
            chaos_seed: Some(42),
            project_id: Some("test_proj".into()),
            track_id: Some("test_track".into()),
            preset_name: Some("spotify".into()),
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();
        let (cfg, log, persona) = build_dsp_config(&req, &features, None).unwrap();

        let input_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        let output = vec![0.05_f32; 1000];

        let cert = generate_certificate(
            input_hash,
            &output,
            CertificateRequest {
                lra: 7.5, // plausible LRA — these tests exercise hash/signature logic, not LRA-specific behavior; confirmed via recon that lra isn't part of the signed payload (F-029)
                persona: &persona,
                dsp_config: &cfg,
                proof_log: &log,
                req: &req,
                system_version: "1.0.0",
            },
        );
        assert_eq!(cert.persona_id, "warm_analog");
        assert_eq!(cert.preset_name, "spotify");
        assert_eq!(cert.version, "1.0");
        assert_eq!(cert.input_pcm_hash.len(), 64);
        assert_eq!(cert.output_pcm_hash.len(), 64);
    }

    #[test]
    fn bridge_certificate_different_output_different_hash() {
        let req = AetherRequest {
            chaos_seed: Some(42),
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();
        let (cfg, log, persona) = build_dsp_config(&req, &features, None).unwrap();

        let input_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();
        let output1 = vec![0.05_f32; 100];
        let mut output2 = output1.clone();
        output2[0] += 0.001;

        let c1 = generate_certificate(
            input_hash.clone(),
            &output1,
            CertificateRequest {
                lra: 7.5,
                persona: &persona,
                dsp_config: &cfg,
                proof_log: &log,
                req: &req,
                system_version: "1.0.0",
            },
        );
        let c2 = generate_certificate(
            input_hash,
            &output2,
            CertificateRequest {
                lra: 7.5,
                persona: &persona,
                dsp_config: &cfg,
                proof_log: &log,
                req: &req,
                system_version: "1.0.0",
            },
        );

        assert_ne!(c1.output_pcm_hash, c2.output_pcm_hash);
    }

    #[test]
    fn bridge_pre_analysis_zone_flags_wire_through() {
        use lineos_types::pre_analysis::PreAnalysisData;

        let req = AetherRequest {
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();

        // Construct pre_analysis with zone flags active
        let mut pa = PreAnalysisData::silent();
        pa.zone_flags.zone_sub_rumble = true;
        pa.zone_flags.zone_cymbal_harsh = true;

        let (cfg, _, _) = build_dsp_config(&req, &features, Some(&pa)).unwrap();

        // Cymbal harsh zone: resolved band above 5000 Hz with negative gain
        let harsh_band = cfg
            .eq
            .zone_bands
            .iter()
            .find(|b| b.center_hz > 5000.0 && b.gain_db < 0.0);
        assert!(
            harsh_band.is_some(),
            "zone_cymbal_harsh=true must produce a high-mid cut band. \
             Got bands: {:?}",
            cfg.eq.zone_bands
        );

        // Sub rumble zone: when active, S-007 merges it with bass zone.
        // Verify more bands exist than without pre_analysis flags.
        let (cfg_none, _, _) = build_dsp_config(&req, &features, None).unwrap();
        assert!(
            cfg.eq.zone_bands.len() >= cfg_none.eq.zone_bands.len(),
            "Zone flags active must produce >= bands than inactive. \
             With flags: {}, without: {}",
            cfg.eq.zone_bands.len(),
            cfg_none.eq.zone_bands.len()
        );
    }

    #[test]
    fn bridge_no_pre_analysis_no_corrective_zones() {
        let req = AetherRequest {
            content_type: ContentType::Music,
            ..Default::default()
        };
        let features = test_features();

        // Without pre_analysis and with default (zero) StemFeatures,
        // no corrective zones should fire
        let (cfg, _, _) = build_dsp_config(&req, &features, None).unwrap();

        let sub_band = cfg
            .eq
            .zone_bands
            .iter()
            .find(|b| b.center_hz < 100.0 && b.gain_db < 0.0);
        assert!(
            sub_band.is_none(),
            "No sub_rumble zone expected without pre_analysis. \
             Got bands: {:?}",
            cfg.eq.zone_bands
        );
    }
}
