// shared/aether-bridge/src/lib.rs
// Authority: RFC-005 v0.3 FINAL
// Cross-layer orchestration bridge:
// lineos/m0/m0-daemon → shared/aether-bridge → aether/

use aether::personas::manager::PersonaManager;
use aether::personas::config::{PersonaConfig, MacroControls};
use aether::mapping::mapper::MacroMicroMapper;
use aether::chaos::engine::ChaosEngine;
use aether::semantic::resolver::SemanticZoneResolver;
use integration::firewall::IntegrationFirewall;
use integration::config::DspConfig;
use integration::proof_log::ProofLog;
use proof::proof::ExecutionProof;
use proof::certificate::ExecutionCertificate;
use lineos_types::analysis::StemFeatures;

/// Aether tuning parameters from the caller.
/// Decoupled from MasterRequest (m0 network DTO).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AetherRequest {
    pub persona_id:  Option<String>,
    pub warmth:      Option<f32>,
    pub punch:       Option<f32>,
    pub forwardness: Option<f32>,
    pub smoothness:  Option<f32>,
    pub chaos_seed:  Option<u64>,
    pub project_id:  Option<String>,
    pub track_id:    Option<String>,
    pub preset_name: Option<String>,
}

impl Default for AetherRequest {
    fn default() -> Self {
        Self {
            persona_id:  None,
            warmth:      None,
            punch:       None,
            forwardness: None,
            smoothness:  None,
            chaos_seed:  None,
            project_id:  None,
            track_id:    None,
            preset_name: None,
        }
    }
}

#[derive(Debug)]
pub enum AetherBridgeError {
    PersonaNotFound(String),
    FirewallError(String),
}

impl std::fmt::Display for AetherBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PersonaNotFound(id) =>
                write!(f, "Persona not found: {}", id),
            Self::FirewallError(e) =>
                write!(f, "Firewall error: {}", e),
        }
    }
}

/// Phase 1 of RFC-005 pipeline: Pre-DSP Aether processing.
/// Run BEFORE DSP render.
/// Returns: (DspConfig, ProofLog, PersonaConfig)
/// DspConfig is passed to sp314-dsp for rendering.
/// ProofLog is passed to generate_certificate() after render.
pub fn build_dsp_config(
    req:      &AetherRequest,
    features: &StemFeatures,
) -> Result<(DspConfig, ProofLog, PersonaConfig), AetherBridgeError> {

    let mgr     = PersonaManager::load();
    let persona = mgr.get(
        req.persona_id.as_deref().unwrap_or("warm_analog")
    ).ok_or_else(|| AetherBridgeError::PersonaNotFound(
        req.persona_id.clone().unwrap_or("warm_analog".into())
    ))?.clone();

    let macros = MacroControls {
        warmth:      req.warmth.unwrap_or(
                         persona.macros.warmth.default),
        punch:       req.punch.unwrap_or(
                         persona.macros.punch.default),
        forwardness: req.forwardness.unwrap_or(
                         persona.macros.forwardness.default),
        smoothness:  req.smoothness.unwrap_or(
                         persona.macros.smoothness.default),
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
    let chaos_delta = chaos_engine.next_delta(
        persona.chaos_intensity,
        &persona.chaos,
    );

    let modulated = ChaosEngine::apply(&micro, &chaos_delta);
    let zones     = SemanticZoneResolver::auto_carve(
                        &persona, features);

    let mut proof_log = ProofLog::new();
    let dsp_config    = IntegrationFirewall::build(
        &persona, &macros, &modulated,
        &zones, &chaos_delta, seed, &mut proof_log,
    ).map_err(|e| AetherBridgeError::FirewallError(
        format!("{:?}", e)
    ))?;

    Ok((dsp_config, proof_log, persona))
}

/// Phase 3 of RFC-005 pipeline: Post-DSP certificate generation.
/// Run AFTER DSP render with actual input + output PCM.
/// Cryptographically binds audio to DspConfig (S-010).
pub fn generate_certificate(
    input_pcm:  &[f32],
    output_pcm: &[f32],
    persona:    &PersonaConfig,
    dsp_config: &DspConfig,
    proof_log:  &ProofLog,
    req:        &AetherRequest,
    system_version: &str,
) -> ExecutionCertificate {
    ExecutionProof::generate(
        input_pcm,
        output_pcm,
        persona,
        dsp_config,
        proof_log,
        req.project_id.as_deref().unwrap_or("default"),
        req.track_id.as_deref().unwrap_or("default"),
        &chrono::Utc::now().to_rfc3339(),
        system_version,
        req.preset_name.as_deref().unwrap_or("default"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::analysis::{StemFeatures, StemMetrics,
                                  MixMetrics};

    fn test_features() -> StemFeatures {
        StemFeatures {
            bass:   StemMetrics::default(),
            vocals: StemMetrics::default(),
            drums:  StemMetrics::default(),
            other:  StemMetrics::default(),
            mix:    MixMetrics::default(),
        }
    }

    #[test]
    fn bridge_build_dsp_config_default_persona() {
        let req      = AetherRequest::default();
        let features = test_features();
        let result   = build_dsp_config(&req, &features);
        assert!(result.is_ok());
        let (cfg, _, persona) = result.unwrap();
        assert_eq!(persona.id, "warm_analog");
        assert!(cfg.stereo.width >= 0.5);
    }

    #[test]
    fn bridge_build_dsp_config_all_personas() {
        let features = test_features();
        for id in ["warm_analog","clean_punch",
                   "hybrid_hifi","cinematic_wide"] {
            let req = AetherRequest {
                persona_id: Some(id.into()),
                ..Default::default()
            };
            assert!(build_dsp_config(&req, &features).is_ok(),
                "Failed for persona: {}", id);
        }
    }

    #[test]
    fn bridge_unknown_persona_error() {
        let req = AetherRequest {
            persona_id: Some("nonexistent".into()),
            ..Default::default()
        };
        let features = test_features();
        assert!(matches!(
            build_dsp_config(&req, &features),
            Err(AetherBridgeError::PersonaNotFound(_))
        ));
    }

    #[test]
    fn bridge_deterministic() {
        let req      = AetherRequest {
            chaos_seed: Some(42),
            ..Default::default()
        };
        let features = test_features();
        let (cfg1, _, _) = build_dsp_config(&req, &features)
            .unwrap();
        let (cfg2, _, _) = build_dsp_config(&req, &features)
            .unwrap();
        assert_eq!(cfg1.eq.low_shelf_gain_db,
                   cfg2.eq.low_shelf_gain_db);
        assert_eq!(cfg1.stereo.width,
                   cfg2.stereo.width);
    }

    #[test]
    fn bridge_certificate_generation() {
        let req      = AetherRequest {
            chaos_seed:  Some(42),
            project_id:  Some("test_proj".into()),
            track_id:    Some("test_track".into()),
            preset_name: Some("spotify".into()),
            ..Default::default()
        };
        let features = test_features();
        let (cfg, log, persona) =
            build_dsp_config(&req, &features).unwrap();

        let input  = vec![0.1_f32; 1000];
        let output = vec![0.05_f32; 1000];

        let cert = generate_certificate(
            &input, &output, &persona,
            &cfg, &log, &req, "1.0.0"
        );
        assert_eq!(cert.persona_id, "warm_analog");
        assert_eq!(cert.preset_name, "spotify");
        assert_eq!(cert.version, "1.0");
        assert_eq!(cert.input_pcm_hash.len(), 64);
        assert_eq!(cert.output_pcm_hash.len(), 64);
    }

    #[test]
    fn bridge_certificate_different_output_different_hash() {
        let req      = AetherRequest {
            chaos_seed: Some(42),
            ..Default::default()
        };
        let features = test_features();
        let (cfg, log, persona) =
            build_dsp_config(&req, &features).unwrap();

        let input   = vec![0.1_f32; 100];
        let output1 = vec![0.05_f32; 100];
        let mut output2 = output1.clone();
        output2[0] += 0.001;

        let c1 = generate_certificate(
            &input, &output1, &persona,
            &cfg, &log, &req, "1.0.0");
        let c2 = generate_certificate(
            &input, &output2, &persona,
            &cfg, &log, &req, "1.0.0");

        assert_ne!(c1.output_pcm_hash, c2.output_pcm_hash);
    }
}
