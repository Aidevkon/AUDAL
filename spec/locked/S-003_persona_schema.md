# S-003 — Persona Schema & Manager

**Document:** `spec/locked/S-003_persona_schema.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** none (pure compile-time data — no runtime dependencies)
**Used by:** S-004 (Intent Parser), S-005 (Mapping), S-007 (Semantic Zones), S-008 (Auto-Tuning)
**Audit:** DeepSeek v0.1 → REJECT · v0.2 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | Promoted to LOCKED. R6: override clamping uses handle-specific min/max. R7: idempotence note added. |
| 0.2 | 2026-05-27 | Critical: removed S-002 dependency. R1: CurveShape enum. R2: stereo_width [0.5,1.5]. R4: PERSONA_OVERRIDE_DELTA_MAX. R5: exact bounds test. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Persona Schema & Manager defines the structure of Aether personas
and provides a compile-time manager to load, validate, and resolve them.

A persona is a named, bounded configuration that controls the creative
behaviour of the Aether layer. It maps macro controls (warmth, punch,
forwardness, smoothness) to DSP parameter curves, defines chaos intensity,
and sets default semantic zone priorities.

**One sentence:** A persona is a deterministic creative profile that
translates user intent into DSP parameter space.

---

## 2. Constitutional Position

```
PersonaManager (S-003) — compile-time static data
    ↓ PersonaConfig (typed, validated)
    ↓ validate against persona.schema.json
[used by S-004, S-005, S-007, S-008]
```

**Rules:**
- Personas are compile-time constants (embedded TOML)
- No runtime file loading — all personas embedded at build time
- Persona definitions are pure data — no ML, no randomness
- Macro ranges are bounded: [0.0, 1.0] for all handles
- DSP parameter bounds enforced at persona level (first line of defence)
- Session overrides are ephemeral — original TOML unchanged
- Override clamping respects each macro handle's individual min/max
- `apply_override()` is idempotent — same override applied twice yields same result
- Validated against `contracts/persona.schema.json`

---

## 3. Built-in Personas

| ID | Name | Character | Use Case |
|----|------|-----------|----------|
| `warm_analog` | Warm Analog | Vintage tape warmth, gentle compression | Singer-songwriter, acoustic, jazz |
| `clean_punch` | Clean Punch | Tight transients, forward presence | Electronic, hip-hop, pop |
| `hybrid_hifi` | Hybrid Hi-Fi | Balanced, wide stereo, air | Classical, orchestral, audiophile |
| `cinematic_wide` | Cinematic Wide | Deep low-end, wide field, drama | Film score, ambient, cinematic |

---

## 4. Interface

### Constants

```rust
pub const MACRO_MIN:                  f32 = 0.0;
pub const MACRO_MAX:                  f32 = 1.0;
pub const MACRO_DEFAULT:              f32 = 0.5;
pub const CHAOS_INTENSITY_MIN:        f32 = 0.0;
pub const CHAOS_INTENSITY_MAX:        f32 = 1.0;
pub const DSP_GAIN_MIN_DB:            f32 = -12.0;
pub const DSP_GAIN_MAX_DB:            f32 =  12.0;
pub const DSP_FREQ_MIN_HZ:            f32 =  20.0;
pub const DSP_FREQ_MAX_HZ:            f32 =  20_000.0;
pub const DSP_Q_MIN:                  f32 =  0.1;
pub const DSP_Q_MAX:                  f32 =  10.0;
pub const DSP_RATIO_MIN:              f32 =  1.0;
pub const DSP_RATIO_MAX:              f32 =  20.0;
pub const DSP_TIME_MIN_MS:            f32 =  0.1;
pub const DSP_TIME_MAX_MS:            f32 =  500.0;
pub const PERSONA_OVERRIDE_DELTA_MAX: f32 =  0.5;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CurveShape { Linear, Log, Exp }
```

### PersonaConfig

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PersonaConfig {
    pub id:              String,
    pub name:            String,
    pub description:     String,
    pub macros:          MacroRanges,
    pub dsp_base:        PersonaDspBase,
    pub chaos_intensity: f32,
    pub zone_priorities: ZonePriorities,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroRanges {
    pub warmth:      MacroHandle,
    pub punch:       MacroHandle,
    pub forwardness: MacroHandle,
    pub smoothness:  MacroHandle,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroHandle {
    pub default: f32,
    pub min:     f32,
    pub max:     f32,
    pub curve:   CurveShape,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PersonaDspBase {
    pub low_shelf_gain_db:  f32,  // [-12.0, 12.0]
    pub low_shelf_freq_hz:  f32,  // [20.0, 500.0]
    pub high_shelf_gain_db: f32,  // [-12.0, 12.0]
    pub high_shelf_freq_hz: f32,  // [2000.0, 20000.0]
    pub comp_threshold_db:  f32,  // [-40.0, 0.0]
    pub comp_ratio:         f32,  // [1.0, 20.0]
    pub comp_attack_ms:     f32,  // [0.1, 100.0]
    pub comp_release_ms:    f32,  // [10.0, 500.0]
    pub saturation_drive:   f32,  // [0.0, 1.0]
    pub saturation_mix:     f32,  // [0.0, 1.0]
    pub stereo_width:       f32,  // [0.5, 1.5] — 1.0 = unchanged
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZonePriorities {
    pub dialogue: u8,  // [0, 10]
    pub bass:     u8,  // [0, 10]
    pub air:      u8,  // [0, 10]
    pub presence: u8,  // [0, 10]
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MacroControls {
    pub warmth:      f32,  // [0.0, 1.0]
    pub punch:       f32,  // [0.0, 1.0]
    pub forwardness: f32,  // [0.0, 1.0]
    pub smoothness:  f32,  // [0.0, 1.0]
}

impl Default for MacroControls {
    fn default() -> Self {
        Self {
            warmth:      MACRO_DEFAULT,
            punch:       MACRO_DEFAULT,
            forwardness: MACRO_DEFAULT,
            smoothness:  MACRO_DEFAULT,
        }
    }
}

/// Ephemeral session-level persona adjustment.
/// Produced by S-008 (Auto-Tuning Engine).
/// Never persisted — discarded on session end.
/// apply_override() is idempotent: same override → same result.
/// Clamping respects each handle's individual min/max bounds.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersonaOverride {
    pub warmth_delta:      f32,  // [-PERSONA_OVERRIDE_DELTA_MAX, +PERSONA_OVERRIDE_DELTA_MAX]
    pub punch_delta:       f32,
    pub forwardness_delta: f32,
    pub smoothness_delta:  f32,
}

pub struct PersonaManager;

impl PersonaManager {
    /// Returns a persona by ID. None if unknown.
    pub fn get(id: &str) -> Option<PersonaConfig>;

    /// Returns all built-in persona IDs.
    pub fn list() -> &'static [&'static str];

    /// Returns the default persona (warm_analog).
    pub fn default() -> PersonaConfig;

    /// Applies a session override to a persona.
    /// Original TOML is unchanged.
    /// Delta clamped to [-PERSONA_OVERRIDE_DELTA_MAX, +PERSONA_OVERRIDE_DELTA_MAX].
    /// Result clamped to each handle's individual [min, max] bounds.
    /// Idempotent: applying same override twice yields same result.
    pub fn apply_override(
        base:     &PersonaConfig,
        override: &PersonaOverride,
    ) -> PersonaConfig;
}
```

---

## 5. Built-in Persona Definitions (TOML)

```toml
# config/personas.toml — embedded at compile time via include_str!()
# LOCKED — changes require version bump and Lead Architect sign-off

[[personas]]
id              = "warm_analog"
name            = "Warm Analog"
description     = "Vintage tape warmth, gentle compression"
chaos_intensity = 0.3

[personas.macros.warmth]
default = 0.7
min     = 0.0
max     = 1.0
curve   = "log"

[personas.macros.punch]
default = 0.4
min     = 0.0
max     = 0.8
curve   = "linear"

[personas.macros.forwardness]
default = 0.5
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.macros.smoothness]
default = 0.6
min     = 0.2
max     = 1.0
curve   = "exp"

[personas.dsp_base]
low_shelf_gain_db  =  1.5
low_shelf_freq_hz  =  200.0
high_shelf_gain_db = -0.5
high_shelf_freq_hz =  10000.0
comp_threshold_db  = -18.0
comp_ratio         =  2.5
comp_attack_ms     =  15.0
comp_release_ms    =  120.0
saturation_drive   =  0.4
saturation_mix     =  0.25
stereo_width       =  1.0

[personas.zone_priorities]
dialogue = 6
bass     = 7
air      = 4
presence = 5

[[personas]]
id              = "clean_punch"
name            = "Clean Punch"
description     = "Tight transients, forward presence"
chaos_intensity = 0.2

[personas.macros.warmth]
default = 0.3
min     = 0.0
max     = 0.7
curve   = "linear"

[personas.macros.punch]
default = 0.8
min     = 0.3
max     = 1.0
curve   = "exp"

[personas.macros.forwardness]
default = 0.7
min     = 0.2
max     = 1.0
curve   = "linear"

[personas.macros.smoothness]
default = 0.3
min     = 0.0
max     = 0.7
curve   = "linear"

[personas.dsp_base]
low_shelf_gain_db  =  0.0
low_shelf_freq_hz  =  100.0
high_shelf_gain_db =  1.0
high_shelf_freq_hz =  8000.0
comp_threshold_db  = -20.0
comp_ratio         =  3.5
comp_attack_ms     =  5.0
comp_release_ms    =  60.0
saturation_drive   =  0.2
saturation_mix     =  0.1
stereo_width       =  1.0

[personas.zone_priorities]
dialogue = 8
bass     = 5
air      = 6
presence = 7

[[personas]]
id              = "hybrid_hifi"
name            = "Hybrid Hi-Fi"
description     = "Balanced, wide stereo, air"
chaos_intensity = 0.15

[personas.macros.warmth]
default = 0.5
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.macros.punch]
default = 0.5
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.macros.forwardness]
default = 0.5
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.macros.smoothness]
default = 0.5
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.dsp_base]
low_shelf_gain_db  =  0.5
low_shelf_freq_hz  =  120.0
high_shelf_gain_db =  1.5
high_shelf_freq_hz =  12000.0
comp_threshold_db  = -16.0
comp_ratio         =  2.0
comp_attack_ms     =  20.0
comp_release_ms    =  150.0
saturation_drive   =  0.1
saturation_mix     =  0.05
stereo_width       =  1.15

[personas.zone_priorities]
dialogue = 5
bass     = 5
air      = 8
presence = 5

[[personas]]
id              = "cinematic_wide"
name            = "Cinematic Wide"
description     = "Deep low-end, wide field, drama"
chaos_intensity = 0.4

[personas.macros.warmth]
default = 0.6
min     = 0.2
max     = 1.0
curve   = "log"

[personas.macros.punch]
default = 0.6
min     = 0.0
max     = 1.0
curve   = "linear"

[personas.macros.forwardness]
default = 0.4
min     = 0.0
max     = 0.8
curve   = "linear"

[personas.macros.smoothness]
default = 0.7
min     = 0.3
max     = 1.0
curve   = "exp"

[personas.dsp_base]
low_shelf_gain_db  =  2.5
low_shelf_freq_hz  =  80.0
high_shelf_gain_db =  0.5
high_shelf_freq_hz =  14000.0
comp_threshold_db  = -22.0
comp_ratio         =  2.8
comp_attack_ms     =  25.0
comp_release_ms    =  200.0
saturation_drive   =  0.35
saturation_mix     =  0.2
stereo_width       =  1.25

[personas.zone_priorities]
dialogue = 4
bass     = 9
air      = 5
presence = 4
```

---

## 6. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same persona ID → same config | ✅ Compile-time embedded TOML |
| No runtime file loading | ✅ `include_str!()` at build time |
| No randomness | ✅ Pure data, no rand |
| Bounded values | ✅ All ranges enforced at load |
| Session overrides ephemeral | ✅ Never persisted to disk |
| apply_override() idempotent | ✅ Pure function, same input → same output |

---

## 7. Performance Targets

| Metric | Target |
|--------|--------|
| `PersonaManager::get()` | < 1μs (static lookup) |
| `PersonaManager::apply_override()` | < 10μs |
| Memory (all 4 personas) | < 10 KB |

---

## 8. Contract Tests

```rust
#[test]
fn persona_manager_loads_all_builtins() {
    for id in PersonaManager::list() {
        assert!(PersonaManager::get(id).is_some());
    }
}

#[test]
fn persona_macro_ranges_valid() {
    for id in PersonaManager::list() {
        let p = PersonaManager::get(id).unwrap();
        for handle in [&p.macros.warmth, &p.macros.punch,
                       &p.macros.forwardness, &p.macros.smoothness] {
            assert!(handle.min >= MACRO_MIN);
            assert!(handle.max <= MACRO_MAX);
            assert!(handle.default >= handle.min);
            assert!(handle.default <= handle.max);
        }
    }
}

#[test]
fn persona_dsp_bounds_valid() {
    for id in PersonaManager::list() {
        let p = PersonaManager::get(id).unwrap();
        let d = &p.dsp_base;
        assert!(d.low_shelf_gain_db  >= DSP_GAIN_MIN_DB);
        assert!(d.low_shelf_gain_db  <= DSP_GAIN_MAX_DB);
        assert!(d.comp_ratio         >= DSP_RATIO_MIN);
        assert!(d.comp_ratio         <= DSP_RATIO_MAX);
        assert!(d.stereo_width       >= 0.5);
        assert!(d.stereo_width       <= 1.5);
    }
}

#[test]
fn persona_chaos_bounded() {
    for id in PersonaManager::list() {
        let p = PersonaManager::get(id).unwrap();
        assert!(p.chaos_intensity >= CHAOS_INTENSITY_MIN);
        assert!(p.chaos_intensity <= CHAOS_INTENSITY_MAX);
    }
}

#[test]
fn persona_override_clamped() {
    let base = PersonaManager::default();
    let result = PersonaManager::apply_override(&base, &PersonaOverride {
        warmth_delta: 99.0, punch_delta: -99.0,
        forwardness_delta: 0.1, smoothness_delta: -0.1,
    });
    assert!(result.macros.warmth.default <= base.macros.warmth.max);
    assert!(result.macros.punch.default  >= base.macros.punch.min);
}

#[test]
fn persona_override_at_exact_bounds_not_clamped() {
    let base = PersonaManager::default();
    let original = base.macros.warmth.default;
    let result = PersonaManager::apply_override(&base, &PersonaOverride {
        warmth_delta: PERSONA_OVERRIDE_DELTA_MAX,
        punch_delta: -PERSONA_OVERRIDE_DELTA_MAX,
        forwardness_delta: 0.0,
        smoothness_delta: 0.0,
    });
    let expected = (original + PERSONA_OVERRIDE_DELTA_MAX)
        .clamp(base.macros.warmth.min, base.macros.warmth.max);
    assert!((result.macros.warmth.default - expected).abs() < 1e-6);
}

#[test]
fn persona_override_idempotent() {
    let base = PersonaManager::default();
    let override_ = PersonaOverride {
        warmth_delta: 0.2, punch_delta: -0.1,
        forwardness_delta: 0.0, smoothness_delta: 0.0,
    };
    let r1 = PersonaManager::apply_override(&base, &override_);
    let r2 = PersonaManager::apply_override(&base, &override_);
    assert_eq!(r1, r2);
}

#[test]
fn persona_serializable() {
    let p = PersonaManager::default();
    let json = serde_json::to_string(&p).unwrap();
    let p2: PersonaConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(p, p2);
}

#[test]
fn persona_validates_schema() {
    let p = PersonaManager::default();
    let json = serde_json::to_string(&p).unwrap();
    assert!(validate_against_schema(&json, "persona.schema.json"));
}
```

---

## 9. Error Handling

| Condition | Behavior |
|-----------|----------|
| Unknown persona ID | Return `None` from `get()` |
| TOML parse error | Compile-time error (embedded) |
| Override out of bounds | Clamp to handle's min/max, log warning |
| Schema validation fail | Return `Err(SchemaValidationFailed)` |

---

## 10. Implementation Path

```
aether/personas/
├── mod.rs      ← pub use manager::PersonaManager
├── config.rs   ← structs + constants + CurveShape
├── manager.rs  ← PersonaManager impl
└── builtin.rs  ← include_str!("../../config/personas.toml")

config/
└── personas.toml  ← embedded at compile time
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. R6: clamping uses handle-specific min/max. R7: idempotence guaranteed and tested. |
| 0.2 | 2026-05-27 | Critical: removed S-002 dep. R1–R5 addressed. |
| 0.1 | 2026-05-27 | Initial draft |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-003_persona_schema.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Personas are deterministic creative profiles.*
*Same ID → same config. Always.*
