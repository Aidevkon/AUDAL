# S-010 — Execution Proof & Certificate (V8)

**Document:** `spec/locked/S-010_execution_proof.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Proof
**Depends on:** S-009 (Integration Firewall)
**Used by:** BMR-128 Report, Export Pipeline
**Audit:** DeepSeek v0.1 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. N1: chaos seed hash note. N2: dsp_config parameter note. N3: canonical JSON implementation note. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

Every render produces a cryptographic certificate that proves:
- What audio was processed (input hash)
- Which persona was active (persona hash)
- What user intent was applied (intent hash)
- What chaos seed was used (chaos hash)
- How zones were resolved (zone hash)
- What DSP parameters were used (config hash)
- What the output sounds like (output hash)

Given the same inputs, anyone can re-run the pipeline and verify
the output matches the certificate.

**One sentence:** Every render produces a SHA-256 certificate that
makes the mastering process externally verifiable and reproducible.

---

## 2. Constitutional Position

```
ProofLog (S-009) accumulates during render:
  - All clamp events (FirewallError::FirewallClamp)
  - Intent (recorded by S-004)
  - ZoneAdjustments (recorded by S-007)

After render completes:
ExecutionProof::generate(proof_log, output_pcm, dsp_config, ...)
    ↓
ExecutionCertificate (SHA-256 hashes)
    ↓
Stored in GoldenBlob (M0)
Displayed in BMR-128 report
Exportable as JSON
```

**Rules:**
- SHA-256 only — no other hash function
- PCM hashed as big-endian f32 bytes (platform-independent)
- JSON hashed using canonical form (sorted keys)
- Certificate is append-only — never modified after generation
- Same inputs → same certificate (bit-identical)
- Certificate verifiable by third party with same inputs

---

## 3. Interface

### ExecutionCertificate

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionCertificate {
    pub version:               String,  // "1.0"

    /// SHA-256 of input PCM (raw f32 big-endian bytes)
    pub input_pcm_hash:        String,  // 64 hex chars

    /// SHA-256 of PersonaConfig JSON (canonical, sorted keys)
    pub persona_hash:          String,

    /// SHA-256 of Intent JSON (canonical)
    /// "none" if no intent was recorded
    pub intent_hash:           String,

    /// SHA-256 of compound string "project_id:track_id:persona_id"
    /// This matches ChaosEngine::build_seed() input (S-006).
    /// Verifier can recompute: SHA-256(compound) → same hash.
    /// The u64 seed is derived from this compound — not hashed directly.
    pub chaos_seed_hash:       String,

    /// SHA-256 of ZoneAdjustments JSON (canonical)
    pub zone_resolutions_hash: String,

    /// SHA-256 of DspConfig JSON (canonical, sorted keys)
    /// DspConfig is passed directly to generate() — not from ProofLog.
    /// ProofLog may also store it for debugging purposes.
    pub final_dsp_config_hash: String,

    /// SHA-256 of output PCM (raw f32 big-endian bytes)
    pub output_pcm_hash:       String,

    pub rendered_at:    String,  // ISO-8601
    pub system_version: String,
    pub persona_id:     String,
    pub preset_name:    String,
}
```

### ProofLog

```rust
/// Accumulates all Aether decisions during a render.
/// Passed through S-009 (Integration Firewall).
/// Used by ExecutionProof::generate() after render completes.
///
/// Recording responsibilities:
///   record_clamp()      — called by S-009 (IntegrationFirewall)
///   record_intent()     — called by S-004 (IntentParser)
///   record_zone_adj()   — called by S-007 (SemanticZoneResolver)
///   record_dsp_config() — optional, for debugging only
///                         (DspConfig is passed directly to generate())
#[derive(Debug, Default)]
pub struct ProofLog {
    pub clamp_events: Vec<ClampEvent>,
    pub intent:       Option<Intent>,
    pub zone_adj:     Option<ZoneAdjustments>,
    pub dsp_config:   Option<DspConfig>,  // optional debug copy
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClampEvent {
    pub field:    String,
    pub original: f32,
    pub clamped:  f32,
}

impl ProofLog {
    pub fn new() -> Self { Self::default() }

    pub fn record_clamp(&mut self, e: FirewallError) {
        if let FirewallError::FirewallClamp { field, original, clamped } = e {
            self.clamp_events.push(ClampEvent { field, original, clamped });
        }
    }

    pub fn record_intent(&mut self, intent: &Intent) {
        self.intent = Some(intent.clone());
    }

    pub fn record_zone_adj(&mut self, zones: &ZoneAdjustments) {
        self.zone_adj = Some(zones.clone());
    }

    pub fn record_dsp_config(&mut self, eq: &DspEqConfig,
        dynamics: &DspDynamicsConfig, sat: &DspSatConfig,
        stereo: &DspStereoConfig) {
        // Optional debug copy — not used in certificate generation
    }

    pub fn clamp_count(&self) -> usize { self.clamp_events.len() }
}
```

### ExecutionProof

```rust
pub struct ExecutionProof;

impl ExecutionProof {
    /// Generate certificate after render completes.
    /// dsp_config is passed directly (not read from proof_log).
    /// proof_log provides intent and zone_adj hashes.
    pub fn generate(
        input_pcm:      &[f32],
        output_pcm:     &[f32],
        persona:        &PersonaConfig,
        dsp_config:     &DspConfig,
        proof_log:      &ProofLog,
        project_id:     &str,
        track_id:       &str,
        rendered_at:    &str,
        system_version: &str,
        preset_name:    &str,
    ) -> ExecutionCertificate;

    /// Verify certificate against known inputs.
    /// Returns Ok(()) if all hashes match.
    pub fn verify(
        cert:       &ExecutionCertificate,
        input_pcm:  &[f32],
        output_pcm: &[f32],
        persona:    &PersonaConfig,
        dsp_config: &DspConfig,
    ) -> Result<(), VerificationError>;
}

#[derive(Debug)]
pub enum VerificationError {
    InputPcmMismatch  { expected: String, actual: String },
    PersonaMismatch   { expected: String, actual: String },
    DspConfigMismatch { expected: String, actual: String },
    OutputPcmMismatch { expected: String, actual: String },
}
```

---

## 4. Hash Computation

### PCM Hash (platform-independent)

```rust
fn hash_pcm(pcm: &[f32]) -> String {
    let bytes: Vec<u8> = pcm.iter()
        .flat_map(|f| f.to_be_bytes())
        .collect();
    sha256_hex(&bytes)
}
```

### Canonical JSON Hash

```rust
/// Hash a value as canonical JSON (sorted object keys).
/// Implementation note: use `serde_canonical_json` crate,
/// OR convert to serde_json::Value and serialize a BTreeMap
/// to guarantee sorted keys before hashing.
///
/// Example with BTreeMap:
///   let map: BTreeMap<String, Value> = serde_json::from_str(
///       &serde_json::to_string(value).unwrap()
///   ).unwrap();
///   sha256_hex(serde_json::to_string(&map).unwrap().as_bytes())
fn hash_json<T: Serialize>(value: &T) -> String {
    let json_str = canonical_json_string(value);
    sha256_hex(json_str.as_bytes())
}
```

### SHA-256 → 64-char hex

```rust
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Sha256, Digest};
    format!("{:x}", Sha256::digest(bytes))
}
```

---

## 5. Certificate Generation

```rust
pub fn generate(
    input_pcm:      &[f32],
    output_pcm:     &[f32],
    persona:        &PersonaConfig,
    dsp_config:     &DspConfig,
    proof_log:      &ProofLog,
    project_id:     &str,
    track_id:       &str,
    rendered_at:    &str,
    system_version: &str,
    preset_name:    &str,
) -> ExecutionCertificate {

    // chaos_seed_hash: SHA-256 of compound string
    // Matches ChaosEngine::build_seed() input (S-006)
    let chaos_compound = format!("{}:{}:{}", project_id, track_id, persona.id);
    let chaos_seed_hash = sha256_hex(chaos_compound.as_bytes());

    ExecutionCertificate {
        version:               "1.0".into(),
        input_pcm_hash:        hash_pcm(input_pcm),
        persona_hash:          hash_json(persona),
        intent_hash:           proof_log.intent.as_ref()
                                   .map(|i| hash_json(i))
                                   .unwrap_or_else(|| "none".into()),
        chaos_seed_hash,
        zone_resolutions_hash: proof_log.zone_adj.as_ref()
                                   .map(|z| hash_json(z))
                                   .unwrap_or_else(|| "none".into()),
        final_dsp_config_hash: hash_json(dsp_config),
        output_pcm_hash:       hash_pcm(output_pcm),
        rendered_at:           rendered_at.into(),
        system_version:        system_version.into(),
        persona_id:            persona.id.clone(),
        preset_name:           preset_name.into(),
    }
}
```

---

## 6. Verification

```rust
pub fn verify(
    cert:       &ExecutionCertificate,
    input_pcm:  &[f32],
    output_pcm: &[f32],
    persona:    &PersonaConfig,
    dsp_config: &DspConfig,
) -> Result<(), VerificationError> {

    let actual = hash_pcm(input_pcm);
    if cert.input_pcm_hash != actual {
        return Err(VerificationError::InputPcmMismatch {
            expected: cert.input_pcm_hash.clone(), actual });
    }

    let actual = hash_json(persona);
    if cert.persona_hash != actual {
        return Err(VerificationError::PersonaMismatch {
            expected: cert.persona_hash.clone(), actual });
    }

    let actual = hash_json(dsp_config);
    if cert.final_dsp_config_hash != actual {
        return Err(VerificationError::DspConfigMismatch {
            expected: cert.final_dsp_config_hash.clone(), actual });
    }

    let actual = hash_pcm(output_pcm);
    if cert.output_pcm_hash != actual {
        return Err(VerificationError::OutputPcmMismatch {
            expected: cert.output_pcm_hash.clone(), actual });
    }

    Ok(())
}
```

---

## 7. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same inputs → same certificate | ✅ SHA-256 + canonical JSON |
| PCM hashing platform-independent | ✅ `to_be_bytes()` |
| JSON hashing deterministic | ✅ Sorted keys (canonical) |
| `verify()` pure function | ✅ No side effects |
| Certificate immutable | ✅ No post-generation modification |
| No randomness | ✅ No rand |

---

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| `generate()` (3-min track) | < 500ms |
| `verify()` | < 500ms |
| Certificate size | < 2 KB |

---

## 9. Contract Tests

```rust
#[test]
fn proof_generate_deterministic() {
    let (input, output, persona, cfg, log) = test_proof_inputs();
    let c1 = ExecutionProof::generate(&input, &output, &persona, &cfg,
        &log, "proj1", "track1", "2026-05-27T00:00:00Z", "1.0.0", "spotify");
    let c2 = ExecutionProof::generate(&input, &output, &persona, &cfg,
        &log, "proj1", "track1", "2026-05-27T00:00:00Z", "1.0.0", "spotify");
    assert_eq!(c1.input_pcm_hash,        c2.input_pcm_hash);
    assert_eq!(c1.persona_hash,          c2.persona_hash);
    assert_eq!(c1.final_dsp_config_hash, c2.final_dsp_config_hash);
    assert_eq!(c1.output_pcm_hash,       c2.output_pcm_hash);
}
#[test]
fn proof_different_output_different_hash() {
    let (input, output, persona, cfg, log) = test_proof_inputs();
    let mut output2 = output.clone();
    output2[0] += 0.001;
    let c1 = ExecutionProof::generate(&input, &output,  &persona, &cfg,
        &log, "p","t","2026-05-27T00:00:00Z","1.0.0","spotify");
    let c2 = ExecutionProof::generate(&input, &output2, &persona, &cfg,
        &log, "p","t","2026-05-27T00:00:00Z","1.0.0","spotify");
    assert_ne!(c1.output_pcm_hash, c2.output_pcm_hash);
}
#[test]
fn proof_verify_correct_ok() {
    let (input, output, persona, cfg, log) = test_proof_inputs();
    let cert = ExecutionProof::generate(&input, &output, &persona, &cfg,
        &log, "p","t","2026-05-27T00:00:00Z","1.0.0","spotify");
    assert!(ExecutionProof::verify(&cert, &input, &output,
        &persona, &cfg).is_ok());
}
#[test]
fn proof_verify_wrong_output_fails() {
    let (input, output, persona, cfg, log) = test_proof_inputs();
    let cert = ExecutionProof::generate(&input, &output, &persona, &cfg,
        &log, "p","t","2026-05-27T00:00:00Z","1.0.0","spotify");
    let mut wrong = output.clone();
    wrong[0] += 1.0;
    assert!(matches!(
        ExecutionProof::verify(&cert, &input, &wrong, &persona, &cfg),
        Err(VerificationError::OutputPcmMismatch { .. })
    ));
}
#[test]
fn proof_hash_length_and_hex() {
    let hash = hash_pcm(&vec![0.5_f32; 1000]);
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}
#[test]
fn proof_certificate_serializable() {
    let (input, output, persona, cfg, log) = test_proof_inputs();
    let cert = ExecutionProof::generate(&input, &output, &persona, &cfg,
        &log, "p","t","2026-05-27T00:00:00Z","1.0.0","spotify");
    let cert2: ExecutionCertificate = serde_json::from_str(
        &serde_json::to_string(&cert).unwrap()).unwrap();
    assert_eq!(cert.output_pcm_hash, cert2.output_pcm_hash);
}
#[test]
fn proof_log_clamp_count() {
    let mut log = ProofLog::new();
    assert_eq!(log.clamp_count(), 0);
    log.record_clamp(FirewallError::FirewallClamp {
        field:"test".into(), original:99.0, clamped:12.0
    });
    assert_eq!(log.clamp_count(), 1);
}
```

---

## 10. Error Handling

| Condition | Behavior |
|-----------|----------|
| Empty PCM | Hash of empty bytes — valid |
| Serialization fail | Panic (structs always serializable) |
| Hash mismatch | Return VerificationError |

---

## 11. Implementation Path

```
proof/
├── mod.rs         ← pub use proof::ExecutionProof
├── proof.rs       ← generate(), verify(), hash_pcm(), hash_json()
├── certificate.rs ← ExecutionCertificate, VerificationError
└── log.rs         ← ProofLog, ClampEvent (shared with S-009)
```

**External:** `sha2` (pure Rust, MIT) — already used by S-006.
**Canonical JSON:** `serde_canonical_json` or BTreeMap approach (see §4).

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-010_execution_proof.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Same inputs → same certificate. Always.*
*Every render is verifiable. Every decision is logged.*
