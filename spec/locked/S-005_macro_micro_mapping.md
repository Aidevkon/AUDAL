# S-005 — Macro → Micro Mapping

**Document:** `spec/locked/S-005_macro_micro_mapping.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** S-003 (Persona Schema), S-004 (Intent Parser)
**Used by:** S-009 (Integration Firewall)
**Audit:** DeepSeek v0.1 → REJECT · v0.2 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | Promoted to LOCKED. |
| 0.2 | 2026-05-27 | Critical: low_shelf_freq_delta. M2: forwardness +150Hz. M5+M6+stereo tests. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Macro → Micro Mapping layer converts resolved macro handle values
(warmth, punch, forwardness, smoothness) into concrete DSP parameter
deltas using persona-specific curves.

Its output — a `MicroDelta` struct — represents the exact adjustments
to be applied on top of the persona's DSP base configuration.

**One sentence:** Given a persona and macro values, produce a typed,
bounded set of DSP parameter deltas ready for the Integration Firewall.

---

## 2. Constitutional Position

```
(PersonaConfig, MacroControls) from S-004 §6
    ↓
MacroMicroMapper (S-005)
    ↓ MicroDelta (typed, bounded)
    ↓ validate against dsp_config.schema.json
[ADAPTER BOUNDARY]
    ↓
S-009 (Integration Firewall) → E11 DSP
```

**Rules:**
- No ML, no randomness — pure deterministic math
- libm only — no std::f32 methods
- All output values bounded at this layer (first DSP-level defence)
- Same persona + same macros → same MicroDelta (bit-identical)
- MicroDelta is additive on top of PersonaDspBase
- Validated against `contracts/dsp_config.schema.json`

---

## 3. Interface

### Constants

```rust
pub const EQ_GAIN_DELTA_MIN_DB:        f32 = -6.0;
pub const EQ_GAIN_DELTA_MAX_DB:        f32 =  6.0;
pub const EQ_FREQ_DELTA_MIN_HZ:        f32 = -500.0;
pub const EQ_FREQ_DELTA_MAX_HZ:        f32 =  500.0;
pub const COMP_THRESHOLD_DELTA_MIN_DB: f32 = -12.0;
pub const COMP_THRESHOLD_DELTA_MAX_DB: f32 =  12.0;
pub const COMP_RATIO_DELTA_MIN:        f32 = -3.0;
pub const COMP_RATIO_DELTA_MAX:        f32 =  3.0;
pub const COMP_ATTACK_DELTA_MIN_MS:    f32 = -20.0;
pub const COMP_ATTACK_DELTA_MAX_MS:    f32 =  20.0;
pub const COMP_RELEASE_DELTA_MIN_MS:   f32 = -100.0;
pub const COMP_RELEASE_DELTA_MAX_MS:   f32 =  100.0;
pub const SAT_DRIVE_DELTA_MIN:         f32 = -0.3;
pub const SAT_DRIVE_DELTA_MAX:         f32 =  0.3;
pub const SAT_MIX_DELTA_MIN:           f32 = -0.2;
pub const SAT_MIX_DELTA_MAX:           f32 =  0.2;
pub const STEREO_WIDTH_DELTA_MIN:      f32 = -0.3;
pub const STEREO_WIDTH_DELTA_MAX:      f32 =  0.3;
```

### MicroDelta

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MicroDelta {
    pub eq:       EqDelta,
    pub dynamics: DynamicsDelta,
    pub sat:      SaturationDelta,
    pub stereo:   StereoDelta,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EqDelta {
    pub low_shelf_gain_db:  f32,  // [-6.0,   6.0]
    pub low_shelf_freq_hz:  f32,  // [-500.0, 500.0]
    pub high_shelf_gain_db: f32,  // [-6.0,   6.0]
    pub high_shelf_freq_hz: f32,  // [-500.0, 500.0]
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DynamicsDelta {
    pub comp_threshold_db: f32,  // [-12.0, 12.0]
    pub comp_ratio:        f32,  // [-3.0,   3.0]
    pub comp_attack_ms:    f32,  // [-20.0,  20.0]
    pub comp_release_ms:   f32,  // [-100.0, 100.0]
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SaturationDelta {
    pub drive: f32,  // [-0.3, 0.3]
    pub mix:   f32,  // [-0.2, 0.2]
}

/// Stereo width delta.
/// NOT driven by macros — always 0.0 from this layer.
/// Set by S-006 (Chaos Engine) on top of PersonaDspBase.stereo_width.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StereoDelta {
    pub width: f32,  // [-0.3, 0.3] — always 0.0 from S-005
}

impl MicroDelta {
    pub fn zero() -> Self {
        MicroDelta {
            eq: EqDelta {
                low_shelf_gain_db:  0.0,
                low_shelf_freq_hz:  0.0,
                high_shelf_gain_db: 0.0,
                high_shelf_freq_hz: 0.0,
            },
            dynamics: DynamicsDelta {
                comp_threshold_db: 0.0,
                comp_ratio:        0.0,
                comp_attack_ms:    0.0,
                comp_release_ms:   0.0,
            },
            sat:    SaturationDelta { drive: 0.0, mix: 0.0 },
            stereo: StereoDelta { width: 0.0 },
        }
    }
}
```

### MacroMicroMapper

```rust
pub struct MacroMicroMapper;

impl MacroMicroMapper {
    /// Map (persona, macros) → MicroDelta.
    /// Pure function. Deterministic. libm only.
    pub fn map(persona: &PersonaConfig, macros: &MacroControls) -> MicroDelta;
}
```

---

## 4. Curve Functions

```rust
/// Apply curve shape to a normalized macro value.
/// Input:  x ∈ [0.0, 1.0]
/// Output: y ∈ [0.0, 1.0]
/// Verified: apply_curve(0.0, any) = 0.0, apply_curve(1.0, any) = 1.0
/// libm only.
pub fn apply_curve(x: f32, curve: &CurveShape) -> f32 {
    match curve {
        CurveShape::Linear => x,
        CurveShape::Log    => libm::log10f(1.0_f32 + 9.0_f32 * x),
        CurveShape::Exp    => (libm::powf(10.0_f32, x) - 1.0_f32) / 9.0_f32,
    }
}
```

---

## 5. Macro → Parameter Mapping

### 5.1 Warmth → EQ + Saturation

```
w = apply_curve(warmth, persona.macros.warmth.curve)

low_shelf_gain_db  = w × 3.0    dB   [max +3dB  — body]
low_shelf_freq_hz  = w × (-100) Hz   [max -100Hz — deeper shelf]
high_shelf_gain_db = w × (-1.5) dB   [max -1.5dB — less harshness]
sat_drive          = w × 0.2         [max +0.2]
sat_mix            = w × 0.15        [max +0.15]
```

Lowering the low-shelf frequency makes warmth deeper (wider bass range).

---

### 5.2 Punch → Dynamics

```
p = apply_curve(punch, persona.macros.punch.curve)

comp_threshold_db = p × (-6.0)  dB   [max -6dB  — more compression]
comp_ratio        = p × 1.5          [max +1.5]
comp_attack_ms    = p × (-15.0) ms   [max -15ms — faster attack]
comp_release_ms   = p × (-50.0) ms   [max -50ms — faster release]
```

---

### 5.3 Forwardness → EQ presence

```
f = apply_curve(forwardness, persona.macros.forwardness.curve)

high_shelf_gain_db += f × 2.0   dB   [max +2dB   — presence boost]
high_shelf_freq_hz  = f × 150.0 Hz   [max +150Hz  — raises shelf start]
```

**Authority decision:** Positive frequency shift (+150Hz) raises the
shelf corner upward, adding air and clarity. Negative shift was rejected
as it risks muddiness in the mid-presence range.

Note: high_shelf_gain combines warmth (-1.5×w) and forwardness (+2.0×f).
Clamped after combination.

---

### 5.4 Smoothness → Dynamics + Saturation

```
s = apply_curve(smoothness, persona.macros.smoothness.curve)

comp_release_ms += s × 60.0  ms  [max +60ms  — slower release]
comp_attack_ms  += s × 10.0  ms  [max +10ms  — slightly slower]
sat_drive       -= s × 0.1       [max -0.1   — less drive]
```

Smoothness adds to punch contributions. Clamped after combination.

---

## 6. Delta Combination & Clamping

```rust
pub fn map(persona: &PersonaConfig, macros: &MacroControls) -> MicroDelta {
    let w = apply_curve(macros.warmth,      &persona.macros.warmth.curve);
    let p = apply_curve(macros.punch,       &persona.macros.punch.curve);
    let f = apply_curve(macros.forwardness, &persona.macros.forwardness.curve);
    let s = apply_curve(macros.smoothness,  &persona.macros.smoothness.curve);

    MicroDelta {
        eq: EqDelta {
            low_shelf_gain_db:  (w * 3.0)
                .clamp(EQ_GAIN_DELTA_MIN_DB,  EQ_GAIN_DELTA_MAX_DB),
            low_shelf_freq_hz:  (w * -100.0)
                .clamp(EQ_FREQ_DELTA_MIN_HZ,  EQ_FREQ_DELTA_MAX_HZ),
            high_shelf_gain_db: (w * -1.5 + f * 2.0)
                .clamp(EQ_GAIN_DELTA_MIN_DB,  EQ_GAIN_DELTA_MAX_DB),
            high_shelf_freq_hz: (f * 150.0)
                .clamp(EQ_FREQ_DELTA_MIN_HZ,  EQ_FREQ_DELTA_MAX_HZ),
        },
        dynamics: DynamicsDelta {
            comp_threshold_db: (p * -6.0)
                .clamp(COMP_THRESHOLD_DELTA_MIN_DB, COMP_THRESHOLD_DELTA_MAX_DB),
            comp_ratio:        (p * 1.5)
                .clamp(COMP_RATIO_DELTA_MIN, COMP_RATIO_DELTA_MAX),
            comp_attack_ms:    (p * -15.0 + s * 10.0)
                .clamp(COMP_ATTACK_DELTA_MIN_MS, COMP_ATTACK_DELTA_MAX_MS),
            comp_release_ms:   (p * -50.0 + s * 60.0)
                .clamp(COMP_RELEASE_DELTA_MIN_MS, COMP_RELEASE_DELTA_MAX_MS),
        },
        sat: SaturationDelta {
            drive: (w * 0.2 - s * 0.1).clamp(SAT_DRIVE_DELTA_MIN, SAT_DRIVE_DELTA_MAX),
            mix:   (w * 0.15)          .clamp(SAT_MIX_DELTA_MIN,   SAT_MIX_DELTA_MAX),
        },
        stereo: StereoDelta { width: 0.0 },
    }
}
```

---

## 7. Stereo Width

Stereo width delta is **not driven by macros** at this layer.
Set exclusively by **S-006 (Chaos Engine)** on top of
`PersonaDspBase.stereo_width`. `StereoDelta.width` is always `0.0`.

---

## 8. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same persona + macros → same delta | ✅ Pure function |
| No randomness | ✅ No chaos at this layer |
| libm only | ✅ log10f, powf |
| All outputs bounded | ✅ Clamped after every combination |
| Bit-identical across platforms | ✅ libm guarantees |

---

## 9. Performance Targets

| Metric | Target |
|--------|--------|
| `map()` latency | < 50μs |
| Memory | < 1 KB |
| Allocations | Zero — pure stack |

---

## 10. Contract Tests

```rust
#[test]
fn mapping_deterministic() {
    let p = PersonaManager::get("warm_analog").unwrap();
    let m = MacroControls::default();
    assert_eq!(MacroMicroMapper::map(&p, &m), MacroMicroMapper::map(&p, &m));
}

#[test]
fn mapping_zero_macros_zero_delta() {
    let p = PersonaManager::get("warm_analog").unwrap();
    let m = MacroControls { warmth:0.0, punch:0.0, forwardness:0.0, smoothness:0.0 };
    let d = MacroMicroMapper::map(&p, &m);
    assert_eq!(d, MicroDelta::zero());
}

#[test]
fn mapping_all_bounds_respected() {
    for id in PersonaManager::list() {
        let p = PersonaManager::get(id).unwrap();
        let m = MacroControls { warmth:1.0, punch:1.0, forwardness:1.0, smoothness:1.0 };
        let d = MacroMicroMapper::map(&p, &m);
        assert!(d.eq.low_shelf_gain_db  >= EQ_GAIN_DELTA_MIN_DB);
        assert!(d.eq.low_shelf_gain_db  <= EQ_GAIN_DELTA_MAX_DB);
        assert!(d.eq.low_shelf_freq_hz  >= EQ_FREQ_DELTA_MIN_HZ);
        assert!(d.eq.low_shelf_freq_hz  <= EQ_FREQ_DELTA_MAX_HZ);
        assert!(d.eq.high_shelf_gain_db >= EQ_GAIN_DELTA_MIN_DB);
        assert!(d.eq.high_shelf_gain_db <= EQ_GAIN_DELTA_MAX_DB);
        assert!(d.dynamics.comp_ratio   >= COMP_RATIO_DELTA_MIN);
        assert!(d.dynamics.comp_ratio   <= COMP_RATIO_DELTA_MAX);
        assert!(d.sat.drive >= SAT_DRIVE_DELTA_MIN);
        assert!(d.sat.drive <= SAT_DRIVE_DELTA_MAX);
    }
}

#[test]
fn mapping_warmth_increases_low_shelf_gain() {
    let p  = PersonaManager::get("warm_analog").unwrap();
    let lo = MacroControls { warmth:0.2, punch:0.5, forwardness:0.5, smoothness:0.5 };
    let hi = MacroControls { warmth:0.8, punch:0.5, forwardness:0.5, smoothness:0.5 };
    assert!(MacroMicroMapper::map(&p, &hi).eq.low_shelf_gain_db >
            MacroMicroMapper::map(&p, &lo).eq.low_shelf_gain_db);
}

#[test]
fn mapping_warmth_lowers_low_shelf_freq() {
    let p  = PersonaManager::get("warm_analog").unwrap();
    let lo = MacroControls { warmth:0.2, punch:0.5, forwardness:0.5, smoothness:0.5 };
    let hi = MacroControls { warmth:0.8, punch:0.5, forwardness:0.5, smoothness:0.5 };
    assert!(MacroMicroMapper::map(&p, &hi).eq.low_shelf_freq_hz <
            MacroMicroMapper::map(&p, &lo).eq.low_shelf_freq_hz);
}

#[test]
fn mapping_punch_tightens_compressor() {
    let p  = PersonaManager::get("clean_punch").unwrap();
    let lo = MacroControls { warmth:0.5, punch:0.2, forwardness:0.5, smoothness:0.5 };
    let hi = MacroControls { warmth:0.5, punch:0.9, forwardness:0.5, smoothness:0.5 };
    assert!(MacroMicroMapper::map(&p, &hi).dynamics.comp_attack_ms <
            MacroMicroMapper::map(&p, &lo).dynamics.comp_attack_ms);
}

#[test]
fn mapping_combined_high_shelf_gain() {
    // hybrid_hifi: all linear curves
    // warmth=0.5, forwardness=0.5
    // expected: 0.5*(-1.5) + 0.5*2.0 = 0.25
    let p = PersonaManager::get("hybrid_hifi").unwrap();
    let m = MacroControls { warmth:0.5, punch:0.5, forwardness:0.5, smoothness:0.5 };
    let d = MacroMicroMapper::map(&p, &m);
    assert!((d.eq.high_shelf_gain_db - 0.25).abs() < 1e-4);
}

#[test]
fn curve_apply_boundary_values() {
    for curve in [CurveShape::Linear, CurveShape::Log, CurveShape::Exp] {
        assert!(apply_curve(0.0, &curve).abs() < 1e-5);
        assert!((apply_curve(1.0, &curve) - 1.0).abs() < 1e-5);
    }
}

#[test]
fn mapping_stereo_width_always_zero() {
    let p = PersonaManager::get("cinematic_wide").unwrap();
    let m = MacroControls { warmth:1.0, punch:1.0, forwardness:1.0, smoothness:1.0 };
    assert_eq!(MacroMicroMapper::map(&p, &m).stereo.width, 0.0);
}

#[test]
fn mapping_serializable() {
    let p = PersonaManager::default();
    let m = MacroControls::default();
    let d = MacroMicroMapper::map(&p, &m);
    let json = serde_json::to_string(&d).unwrap();
    let d2: MicroDelta = serde_json::from_str(&json).unwrap();
    assert_eq!(d, d2);
}
```

---

## 11. Error Handling

| Condition | Behavior |
|-----------|----------|
| NaN/Inf in macro input | Clamp to [0.0,1.0], log warning |
| All outputs | Clamped — never panics |

---

## 12. Implementation Path

```
aether/mapping/
├── mod.rs     ← pub use mapper::MacroMicroMapper
├── types.rs   ← MicroDelta, EqDelta, DynamicsDelta,
│                 SaturationDelta, StereoDelta, constants
├── mapper.rs  ← MacroMicroMapper::map()
└── curves.rs  ← apply_curve() (CurveShape re-export from S-003)
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. |
| 0.2 | 2026-05-27 | Critical fix + M2+M5+M6 addressed. |
| 0.1 | 2026-05-27 | Initial draft |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-005_macro_micro_mapping.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Same persona + same macros → same delta. Always.*
*Stereo width belongs to Chaos. Always.*
