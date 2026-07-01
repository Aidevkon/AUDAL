// proof/src/proof.rs — ExecutionProof
// Authority: spec/locked/S-010_execution_proof.md v1.0
// SHA-256 only. PCM as big-endian f32 bytes (platform-independent).
// Canonical JSON: deserialize to Value → BTreeMap → reserialize.

use super::certificate::{ExecutionCertificate, VerificationError};
use aether::personas::config::PersonaConfig;
use integration::config::DspConfig;
use integration::proof_log::ProofLog;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub struct ExecutionProof;

impl ExecutionProof {
    /// Generate certificate after render completes.
    /// project_id, track_id: provided by caller (session context).
    /// TODO: obtain from SessionContext in v2.0.
    pub fn generate(
        input_pcm_hash: String,
        output_pcm: &[f32],
        persona: &PersonaConfig,
        dsp_config: &DspConfig,
        proof_log: &ProofLog,
        project_id: &str,
        track_id: &str,
        rendered_at: &str,
        system_version: &str,
        preset_name: &str,
    ) -> ExecutionCertificate {
        // Compute the output hash from the full
        // buffer, then delegate. Streaming paths
        // that already have the hash call
        // generate_from_hash() directly.
        let output_pcm_hash = Self::hash_pcm(output_pcm);
        #[allow(clippy::too_many_arguments)]
        Self::generate_from_hash(
            input_pcm_hash,
            output_pcm_hash,
            persona,
            dsp_config,
            proof_log,
            project_id,
            track_id,
            rendered_at,
            system_version,
            preset_name,
        )
    }

    /// Build an ExecutionCertificate from an
    /// already-computed output PCM hash. Used by
    /// the Episode streaming path, which hashes
    /// the output incrementally during render
    /// (see episode_render output_sha256) rather
    /// than holding the full buffer in RAM.
    ///
    /// generate() is the same thing with the hash
    /// computed from a &[f32] buffer up front —
    /// one source of truth for the certificate
    /// structure.
    #[allow(clippy::too_many_arguments)]
    pub fn generate_from_hash(
        input_pcm_hash: String,
        output_pcm_hash: String,
        persona: &PersonaConfig,
        dsp_config: &DspConfig,
        proof_log: &ProofLog,
        project_id: &str,
        track_id: &str,
        rendered_at: &str,
        system_version: &str,
        preset_name: &str,
    ) -> ExecutionCertificate {
        // chaos_seed_hash: SHA-256 of compound string
        // Matches ChaosEngine::build_seed() input (S-006)
        let chaos_compound = format!("{}:{}:{}", project_id, track_id, persona.id);
        let chaos_seed_hash = Self::sha256_hex(chaos_compound.as_bytes());
        ExecutionCertificate {
            version: "1.0".into(),
            input_pcm_hash,
            persona_hash: Self::hash_json(persona),
            intent_hash: proof_log
                .intent
                .as_ref()
                .map(Self::hash_json)
                .unwrap_or_else(|| "none".into()),
            chaos_seed_hash,
            zone_resolutions_hash: proof_log
                .zone_adj
                .as_ref()
                .map(Self::hash_json)
                .unwrap_or_else(|| "none".into()),
            final_dsp_config_hash: Self::hash_json(dsp_config),
            output_pcm_hash,
            rendered_at: rendered_at.into(),
            system_version: system_version.into(),
            persona_id: persona.id.clone(),
            preset_name: preset_name.into(),
        }
    }

    /// Verify certificate against known inputs.
    /// Returns Ok(()) if all hashes match.
    pub fn verify(
        cert: &ExecutionCertificate,
        input_pcm: &[f32],
        output_pcm: &[f32],
        persona: &PersonaConfig,
        dsp_config: &DspConfig,
    ) -> Result<(), VerificationError> {
        let actual = Self::hash_pcm(input_pcm);
        if cert.input_pcm_hash != actual {
            return Err(VerificationError::InputPcmMismatch {
                expected: cert.input_pcm_hash.clone(),
                actual,
            });
        }
        let actual = Self::hash_json(persona);
        if cert.persona_hash != actual {
            return Err(VerificationError::PersonaMismatch {
                expected: cert.persona_hash.clone(),
                actual,
            });
        }
        let actual = Self::hash_json(dsp_config);
        if cert.final_dsp_config_hash != actual {
            return Err(VerificationError::DspConfigMismatch {
                expected: cert.final_dsp_config_hash.clone(),
                actual,
            });
        }
        let actual = Self::hash_pcm(output_pcm);
        if cert.output_pcm_hash != actual {
            return Err(VerificationError::OutputPcmMismatch {
                expected: cert.output_pcm_hash.clone(),
                actual,
            });
        }
        Ok(())
    }

    /// Hash PCM as big-endian f32 bytes (platform-independent).
    pub fn hash_pcm(pcm: &[f32]) -> String {
        let bytes: Vec<u8> = pcm.iter().flat_map(|f| f.to_be_bytes()).collect();
        Self::sha256_hex(&bytes)
    }

    /// Hash value as canonical JSON (sorted keys — deterministic).
    /// Deserialize to Value → sort all object keys via BTreeMap
    /// recursively → reserialize → hash.
    pub fn hash_json<T: Serialize>(value: &T) -> String {
        let json_str = serde_json::to_string(value).expect("serialization must not fail");
        let canonical = Self::canonicalize_json(&json_str);
        Self::sha256_hex(canonical.as_bytes())
    }

    /// SHA-256 → 64-char lowercase hex string.
    pub fn sha256_hex(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    /// Canonicalize JSON: parse → sort keys recursively → serialize.
    /// Uses serde_json's default Value which sorts BTreeMap keys.
    fn canonicalize_json(json_str: &str) -> String {
        let value: Value =
            serde_json::from_str(json_str).expect("valid JSON required for canonicalization");
        let sorted = Self::sort_value(value);
        serde_json::to_string(&sorted).expect("re-serialization must not fail")
    }

    /// Recursively sort all object keys in a JSON Value.
    fn sort_value(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let sorted: BTreeMap<String, Value> = map
                    .into_iter()
                    .map(|(k, v)| (k, Self::sort_value(v)))
                    .collect();
                Value::Object(sorted.into_iter().collect())
            }
            Value::Array(arr) => Value::Array(arr.into_iter().map(Self::sort_value).collect()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether::chaos::ChaosDelta;
    use aether::mapping::mapper::MacroMicroMapper;
    use aether::personas::manager::PersonaManager;
    use aether::semantic::ZoneAdjustments;

    fn test_persona() -> PersonaConfig {
        PersonaManager::load().default_persona().clone()
    }

    fn test_dsp_config() -> DspConfig {
        use aether::personas::config::MacroControls;
        use integration::firewall::IntegrationFirewall;
        let persona = test_persona();
        let macros = MacroControls::default();
        let micro = MacroMicroMapper::map(&persona, &macros);
        let mut log = ProofLog::new();
        IntegrationFirewall::build(
            &persona,
            &macros,
            &micro,
            &ZoneAdjustments::empty(),
            &ChaosDelta::zero(),
            42,
            &mut log,
        )
        .unwrap()
    }

    fn test_proof_inputs() -> (Vec<f32>, Vec<f32>, PersonaConfig, DspConfig, ProofLog) {
        let input = vec![0.1_f32, -0.2, 0.3, -0.4];
        let output = vec![0.05_f32, -0.1, 0.15, -0.2];
        (
            input,
            output,
            test_persona(),
            test_dsp_config(),
            ProofLog::new(),
        )
    }

    #[test]
    fn proof_generate_deterministic() {
        let (i, o, p, cfg, log) = test_proof_inputs();
        let input_hash = ExecutionProof::hash_pcm(&i);
        let c1 = ExecutionProof::generate(
            input_hash.clone(),
            &o,
            &p,
            &cfg,
            &log,
            "proj1",
            "track1",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        let c2 = ExecutionProof::generate(
            input_hash,
            &o,
            &p,
            &cfg,
            &log,
            "proj1",
            "track1",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        assert_eq!(c1.input_pcm_hash, c2.input_pcm_hash);
        assert_eq!(c1.persona_hash, c2.persona_hash);
        assert_eq!(c1.final_dsp_config_hash, c2.final_dsp_config_hash);
        assert_eq!(c1.output_pcm_hash, c2.output_pcm_hash);
    }

    #[test]
    fn proof_different_output_different_hash() {
        let (i, o, p, cfg, log) = test_proof_inputs();
        let mut o2 = o.clone();
        o2[0] += 0.001;
        let input_hash = ExecutionProof::hash_pcm(&i);
        let c1 = ExecutionProof::generate(
            input_hash.clone(),
            &o,
            &p,
            &cfg,
            &log,
            "p",
            "t",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        let c2 = ExecutionProof::generate(
            input_hash,
            &o2,
            &p,
            &cfg,
            &log,
            "p",
            "t",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        assert_ne!(c1.output_pcm_hash, c2.output_pcm_hash);
    }

    #[test]
    fn proof_verify_correct_ok() {
        let (i, o, p, cfg, log) = test_proof_inputs();
        let input_hash = ExecutionProof::hash_pcm(&i);
        let cert = ExecutionProof::generate(
            input_hash,
            &o,
            &p,
            &cfg,
            &log,
            "p",
            "t",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        assert!(ExecutionProof::verify(&cert, &i, &o, &p, &cfg).is_ok());
    }

    #[test]
    fn proof_verify_wrong_output_fails() {
        let (i, o, p, cfg, log) = test_proof_inputs();
        let input_hash = ExecutionProof::hash_pcm(&i);
        let cert = ExecutionProof::generate(
            input_hash,
            &o,
            &p,
            &cfg,
            &log,
            "p",
            "t",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        let mut wrong = o.clone();
        wrong[0] += 1.0;
        assert!(matches!(
            ExecutionProof::verify(&cert, &i, &wrong, &p, &cfg),
            Err(VerificationError::OutputPcmMismatch { .. })
        ));
    }

    #[test]
    fn proof_hash_length_and_hex() {
        let hash = ExecutionProof::hash_pcm(&vec![0.5_f32; 1000]);
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn proof_certificate_serializable() {
        let (i, o, p, cfg, log) = test_proof_inputs();
        let input_hash = ExecutionProof::hash_pcm(&i);
        let cert = ExecutionProof::generate(
            input_hash,
            &o,
            &p,
            &cfg,
            &log,
            "p",
            "t",
            "2026-05-27T00:00:00Z",
            "1.0.0",
            "spotify",
        );
        let cert2: ExecutionCertificate =
            serde_json::from_str(&serde_json::to_string(&cert).unwrap()).unwrap();
        assert_eq!(cert.output_pcm_hash, cert2.output_pcm_hash);
    }

    #[test]
    fn proof_log_clamp_count() {
        let mut log = ProofLog::new();
        assert_eq!(log.clamp_count(), 0);
        use integration::error::FirewallError;
        log.record_clamp(FirewallError::FirewallClamp {
            field: "test".into(),
            original: 99.0,
            clamped: 12.0,
        });
        assert_eq!(log.clamp_count(), 1);
    }
}
