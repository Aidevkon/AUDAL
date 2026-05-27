# S-008 — Auto-Tuning Engine (ATE)

**Document:** `spec/locked/S-008_autotuning_engine.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** S-002 v1.1 (Stem Feature Analyzer), S-003 v1.1 (Persona Schema)
**Used by:** S-009 (Integration Firewall)
**Grounded in:** RFC-004 (Aether Auto-Tuning Engine)
**Audit:** DeepSeek v0.1 → REJECT · v0.2 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. M7: ATE_TILT_TO_FORWARDNESS added. M8: renamed from ATE_TILT_TO_SMOOTHNESS. |
| 0.2 | 2026-05-27 | C1: single-pass. C2: mix_centroid_hz. M4: width removed. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Auto-Tuning Engine (ATE) performs a **single-pass** measurement of
the deviation between a reference track and the Creator OS output, then
maps that deviation to a bounded `PersonaOverride`.

**v1.0 architecture:** Single-pass only.
Iterative re-render tuning is deferred to v2.0 — without re-rendering
the output audio, looping over the same deviation produces no new information.

**One sentence:** Given a reference track and current master output,
compute a single-pass deviation vector and produce a bounded
`PersonaOverride` that moves the active persona toward the reference.

---

## 2. Constitutional Position

```
Reference Track (audio) + Creator OS Output (audio)
    ↓
StemFeatureAnalyzer (S-002 v1.1) × 2
    ↓
AteEngine::compute_deviation()
    ↓ DeviationVector (deterministic)
AteEngine::deviation_to_override()
    ↓ PersonaOverride (bounded, session-only)
PersonaManager::apply_override() (S-003 v1.1)
    ↓ Updated PersonaConfig → S-005 → S-007 → S-006 → S-009
```

**Rules:**
- No ML, no randomness — pure deterministic metrics
- Single-pass — no re-render loop in v1.0
- Session override only — original TOML never modified
- All persona deltas bounded by PERSONA_OVERRIDE_DELTA_MAX (S-003)
- Same reference + same output → same override (bit-identical)
- Stereo width NOT driven by ATE — belongs to S-006 (Chaos)
- Reuses S-002 v1.1 feature extractor — no new feature code

---

## 3. S-002 v1.1 Amendment (mix_centroid_hz)

S-002 requires a non-breaking v1.1 update to add `spectral_centroid_hz`
to `MixMetrics`.

**Formula — energy-weighted average of stem centroids:**

```rust
pub spectral_centroid_hz: f32,  // added to MixMetrics in S-002 v1.1

fn compute_mix_centroid(features: &StemFeatures) -> f32 {
    let weights   = &features.mix.stem_energy_ratios; // [bass, vocals, drums, other]
    let centroids = [
        features.bass.spectral_centroid_hz,
        features.vocals.spectral_centroid_hz,
        features.drums.spectral_centroid_hz,
        features.other.spectral_centroid_hz,
    ];
    let total_w: f32 = weights.iter().sum();
    if total_w < 1e-10 { return 1000.0; }  // fallback: 1kHz
    weights.iter().zip(centroids.iter())
        .map(|(w, c)| w * c)
        .sum::<f32>() / total_w
}
```

High value = bright mix. Low value = dark/bass-heavy mix.

---

## 4. Interface

### Constants

```rust
pub const ATE_CONVERGENCE_TILT:       f32 = 0.3;   // dB
pub const ATE_CONVERGENCE_BODY:       f32 = 0.25;  // dB
pub const ATE_CONVERGENCE_TRANS:      f32 = 0.15;  // normalized

// Mapping weights (RFC-004 §4.5)
pub const ATE_TILT_TO_WARMTH:         f32 = 0.5;
pub const ATE_BODY_TO_WARMTH:         f32 = 0.4;
pub const ATE_TRANS_TO_PUNCH:         f32 = 0.3;
pub const ATE_LOUD_TO_FORWARDNESS:    f32 = 0.3;
pub const ATE_TILT_TO_FORWARDNESS:    f32 = 0.2;
// Note: stereo width NOT mapped — belongs to S-006 (Chaos)
// Note: smoothness NOT mapped in v1.0 — no reliable metric
```

### DeviationVector

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeviationVector {
    /// Spectral tilt dB — empirical log ratio of mix centroids
    pub delta_tilt_db:       f32,
    /// Low-mid body dB — bass RMS difference
    pub delta_body_db:       f32,
    /// Transient density — drums crest factor diff, normalized [-1,1]
    pub delta_transients:    f32,
    /// Integrated LUFS — loudness difference
    pub delta_loudness_lufs: f32,
    /// True if all deltas within convergence thresholds
    pub converged:           bool,
}

impl DeviationVector {
    pub fn is_converged(&self) -> bool {
        self.delta_tilt_db.abs()       < ATE_CONVERGENCE_TILT
        && self.delta_body_db.abs()    < ATE_CONVERGENCE_BODY
        && self.delta_transients.abs() < ATE_CONVERGENCE_TRANS
    }
}
```

### AteResult

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AteResult {
    pub deviation:        DeviationVector,
    pub persona_override: PersonaOverride,
    pub converged:        bool,
}
```

### AteEngine

```rust
pub struct AteEngine;

impl AteEngine {
    pub fn compute_deviation(reference: &StemFeatures,
                              output:    &StemFeatures) -> DeviationVector;
    pub fn deviation_to_override(deviation: &DeviationVector) -> PersonaOverride;
    pub fn tune(reference_pcm: &[f32], output_pcm: &[f32],
                sample_rate: u32) -> AteResult;
}
```

---

## 5. Deviation Computation

```rust
pub fn compute_deviation(reference: &StemFeatures,
                          output: &StemFeatures) -> DeviationVector {
    // Tilt: log ratio of mix centroids (empirical)
    let ref_c    = reference.mix.spectral_centroid_hz.max(1.0);
    let out_c    = output.mix.spectral_centroid_hz.max(1.0);
    let d_tilt   = 20.0_f32 * libm::log10f(ref_c / out_c);

    // Body: bass RMS difference
    let d_body   = reference.bass.rms_db - output.bass.rms_db;

    // Transients: drums crest factor diff, normalized to [-1,1]
    let d_trans  = ((reference.drums.crest_factor_db
                   - output.drums.crest_factor_db) / 20.0)
                   .clamp(-1.0, 1.0);

    // Loudness: LUFS difference
    let d_loud   = reference.mix.integrated_lufs
                 - output.mix.integrated_lufs;

    let dv = DeviationVector {
        delta_tilt_db:       d_tilt,
        delta_body_db:       d_body,
        delta_transients:    d_trans,
        delta_loudness_lufs: d_loud,
        converged:           false,
    };
    DeviationVector { converged: dv.is_converged(), ..dv }
}
```

---

## 6. Deviation → PersonaOverride Mapping

```rust
pub fn deviation_to_override(dev: &DeviationVector) -> PersonaOverride {
    // Warmth: tilt + body
    let warmth_delta = (
        dev.delta_tilt_db * ATE_TILT_TO_WARMTH
      + dev.delta_body_db * ATE_BODY_TO_WARMTH
    ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

    // Punch: transients
    let punch_delta = (
        dev.delta_transients * ATE_TRANS_TO_PUNCH
    ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

    // Forwardness: loudness + tilt (presence/air)
    let forwardness_delta = (
        dev.delta_loudness_lufs * ATE_LOUD_TO_FORWARDNESS
      + dev.delta_tilt_db      * ATE_TILT_TO_FORWARDNESS
    ).clamp(-PERSONA_OVERRIDE_DELTA_MAX, PERSONA_OVERRIDE_DELTA_MAX);

    // Smoothness: not mapped in v1.0
    // Width → S-006 (Chaos). No reliable smoothness metric in ATE.
    let smoothness_delta = 0.0_f32;

    PersonaOverride { warmth_delta, punch_delta,
                      forwardness_delta, smoothness_delta }
}
```

---

## 7. Single-Pass Tune

```rust
pub fn tune(reference_pcm: &[f32], output_pcm: &[f32],
            sample_rate: u32) -> AteResult {
    let ref_f    = StemFeatureAnalyzer::analyze_stereo(reference_pcm,
                                                        sample_rate);
    let out_f    = StemFeatureAnalyzer::analyze_stereo(output_pcm,
                                                        sample_rate);
    let deviation        = Self::compute_deviation(&ref_f, &out_f);
    let persona_override = Self::deviation_to_override(&deviation);
    AteResult { converged: deviation.converged, deviation, persona_override }
}
```

**v2.0:** Iterative re-render loop (re-render → measure → update → repeat)
deferred until S-009 + full render pipeline are stable.

---

## 8. Session Override Safety

```
PersonaOverride fields: ∈ [-0.5, +0.5]
Applied via: PersonaManager::apply_override() — handle-specific clamp
Original TOML: NEVER modified
Session end:   override discarded
```

---

## 9. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same ref + output → same deviation | ✅ Pure function |
| Same deviation → same override | ✅ Pure function |
| No ML | ✅ Metric arithmetic |
| No randomness | ✅ No rand |
| libm only | ✅ log10f |
| Session-only | ✅ TOML unchanged |
| Override bounded | ✅ ±0.5 |

---

## 10. Performance Targets

| Metric | Target |
|--------|--------|
| `compute_deviation()` | < 1ms |
| `deviation_to_override()` | < 10μs |
| `tune()` total | < 2ms |
| Memory | < 1 MB |

---

## 11. Contract Tests

```rust
#[test]
fn ate_deviation_deterministic() {
    let (r, o) = test_feature_pair();
    assert_eq!(AteEngine::compute_deviation(&r, &o),
               AteEngine::compute_deviation(&r, &o));
}
#[test]
fn ate_identical_zero_deviation() {
    let f = test_stem_features();
    let d = AteEngine::compute_deviation(&f, &f);
    assert!(d.delta_tilt_db.abs()       < 1e-4);
    assert!(d.delta_body_db.abs()       < 1e-4);
    assert!(d.delta_transients.abs()    < 1e-4);
    assert!(d.delta_loudness_lufs.abs() < 1e-4);
}
#[test]
fn ate_identical_converged() {
    let f = test_stem_features();
    assert!(AteEngine::compute_deviation(&f, &f).converged);
}
#[test]
fn ate_override_bounded() {
    let dev = DeviationVector {
        delta_tilt_db:99.0, delta_body_db:-99.0,
        delta_transients:-99.0, delta_loudness_lufs:99.0,
        converged:false,
    };
    let ov = AteEngine::deviation_to_override(&dev);
    assert!(ov.warmth_delta.abs()      <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
    assert!(ov.punch_delta.abs()       <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
    assert!(ov.forwardness_delta.abs() <= PERSONA_OVERRIDE_DELTA_MAX + 1e-5);
    assert_eq!(ov.smoothness_delta,    0.0);
}
#[test]
fn ate_tune_deterministic() {
    let r = test_pcm_reference();
    let o = test_pcm_output();
    assert_eq!(AteEngine::tune(&r, &o, 48000).persona_override.warmth_delta,
               AteEngine::tune(&r, &o, 48000).persona_override.warmth_delta);
}
#[test]
fn ate_tune_identical_converged() {
    let p = test_pcm_reference();
    assert!(AteEngine::tune(&p, &p, 48000).converged);
}
#[test]
fn ate_warmth_positive_when_output_dark() {
    let dev = DeviationVector {
        delta_tilt_db:2.0, delta_body_db:1.0,
        delta_transients:0.0, delta_loudness_lufs:0.0,
        converged:false,
    };
    assert!(AteEngine::deviation_to_override(&dev).warmth_delta > 0.0);
}
#[test]
fn ate_smoothness_always_zero_v1() {
    let dev = DeviationVector {
        delta_tilt_db:1.0, delta_body_db:1.0,
        delta_transients:1.0, delta_loudness_lufs:1.0,
        converged:false,
    };
    assert_eq!(AteEngine::deviation_to_override(&dev).smoothness_delta, 0.0);
}
#[test]
fn ate_result_serializable() {
    let r = test_pcm_reference();
    let o = test_pcm_output();
    let result = AteEngine::tune(&r, &o, 48000);
    let _: AteResult = serde_json::from_str(
        &serde_json::to_string(&result).unwrap()).unwrap();
}
```

---

## 12. Error Handling

| Condition | Behavior |
|-----------|----------|
| Empty PCM | Zero deviation, converged=true |
| Mismatched lengths | Truncate to shorter, log warning |
| NaN in features | Treat as 0.0 |
| All deltas at max | Clamped — never panics |

---

## 13. Implementation Path

```
aether/tuning/
├── mod.rs     ← pub use engine::AteEngine
├── engine.rs  ← compute_deviation(), deviation_to_override(), tune()
└── types.rs   ← DeviationVector, AteResult, constants
```

**Reuses:**
- S-002 v1.1: `StemFeatureAnalyzer`, `MixMetrics.spectral_centroid_hz`
- S-003 v1.1: `PersonaOverride`, `PERSONA_OVERRIDE_DELTA_MAX`

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-008_autotuning_engine.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Single-pass. Deterministic. Session-only.*
*Width belongs to Chaos. Always.*
