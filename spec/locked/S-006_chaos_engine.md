# S-006 — Chaos Modulation Engine

**Document:** `spec/locked/S-006_chaos_engine.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Aether
**Depends on:** S-005 (Macro → Micro Mapping), S-007 (Semantic Zones)
**Used by:** S-009 (Integration Firewall)
**Grounded in:** RFC-001 (Deterministic Chaotic Parameter Modulation)
**Execution:** After S-007 (Semantic Zones) per RFC-002 data flow
**Audit:** DeepSeek v0.2 → PENDING · v0.3 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. |
| 0.3 | 2026-05-27 | C1: compound seed. C2: ChaosProfile → S-003 v1.1. M1: warmup. M2: air shimmer clarified. |
| 0.2 | 2026-05-27 | RFC-001 grounding. Per-persona profiles. modulate() pattern. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Chaos Modulation Engine adds deterministic micro-variation to the
combined DSP delta (MicroDelta from S-005 + ZoneAdjustments from S-007).

It produces organic, "living" colour — the digital equivalent of tape
wow/flutter, transformer saturation drift, and tube harmonic bloom.

**Key principle (RFC-001):** NOT randomness. Deterministic chaotic
behaviour: same compound seed → same sequence, always.

**Seed:** SHA-256(project_id + track_id + persona_id)[0..8] as u64.

**One sentence:** Given a deterministic compound seed and persona chaos
profile, modulate DSP parameters to produce human-perceived analog
imperfection without randomness or ML.

---

## 2. Research Foundation (RFC-001)

```
x[n+1] = r × x[n] × (1 - x[n])
r = 3.9  (fully chaotic: Lyapunov exponent > 0)
x ∈ (0.0, 1.0) — invariant interval
```

- Strogatz — Nonlinear Dynamics and Chaos
- Ott — Chaos in Dynamical Systems
- Zölzer DAFX — wow/flutter, time-varying filters
- Julius O. Smith — Physical Audio Signal Processing

---

## 3. Constitutional Position

```
MicroDelta (S-005) + ZoneAdjustments (S-007)
    ↓
ChaosEngine (S-006)
    │  seed = SHA-256(project_id + track_id + persona_id)[0..8]
    │  logistic map (r=3.9, deterministic)
    │  ChaosProfile from PersonaConfig (S-003 v1.1)
    ↓
ModulatedDelta (MicroDelta — same type, modulated)
    ↓
S-009 (Integration Firewall)
```

**Rules:**
- No rand crate — logistic map ONLY
- Seed = SHA-256(project_id + track_id + persona_id) — compound
- ChaosProfile lives in PersonaConfig (S-003 v1.1)
- All modulated values clamped to S-005 constitutional bounds
- chaos_intensity = 0.0 → MicroDelta passes through unchanged
- Chaos modulates PARAMETERS only — never audio directly
- Executes AFTER S-007 per RFC-002

---

## 4. Interface

### Constants

```rust
pub const CHAOS_R:            f32 = 3.9;   // logistic map — chaotic regime
pub const CHAOS_WARMUP_ITERS: u32 = 100;   // decorrelates state from seed bias
pub const CHAOS_WIDTH_MAX:    f32 = 0.10;  // ±10% stereo width
pub const CHAOS_DRIVE_MAX_DB: f32 = 1.0;   // ±1 dB saturation drive
pub const CHAOS_RELEASE_MAX:  f32 = 10.0;  // ±10 ms compressor release
pub const CHAOS_ATTACK_MAX:   f32 = 2.0;   // ±2 ms compressor attack
pub const CHAOS_SHIMMER_MAX:  f32 = 0.5;   // ±0.5 dB high-shelf GAIN only
```

### ChaosProfile (defined in S-003 v1.1)

```rust
// Lives in PersonaConfig — reproduced here for reference only
pub struct ChaosProfile {
    pub width_depth:   f32,  // [0.0, 1.0]
    pub drive_depth:   f32,  // [0.0, 1.0]
    pub release_depth: f32,  // [0.0, 1.0]
    pub attack_depth:  f32,  // [0.0, 1.0]
    pub air_depth:     f32,  // [0.0, 1.0]
}
```

### ChaosDelta

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChaosDelta {
    pub stereo_width_mod: f32,  // [-CHAOS_WIDTH_MAX,    +CHAOS_WIDTH_MAX]
    pub sat_drive_mod_db: f32,  // [-CHAOS_DRIVE_MAX_DB, +CHAOS_DRIVE_MAX_DB]
    pub comp_release_mod: f32,  // [-CHAOS_RELEASE_MAX,  +CHAOS_RELEASE_MAX]
    pub comp_attack_mod:  f32,  // [-CHAOS_ATTACK_MAX,   +CHAOS_ATTACK_MAX]
    pub air_shimmer_db:   f32,  // [-CHAOS_SHIMMER_MAX,  +CHAOS_SHIMMER_MAX]
                                // HIGH-SHELF GAIN ONLY — not freq
}
```

### ChaosEngine

```rust
pub struct ChaosEngine { state: f32 }

impl ChaosEngine {
    pub fn new(seed: u64) -> Self;
    pub fn build_seed(project_id: &str, track_id: &str,
                      persona_id: &str) -> u64;
    pub fn next_delta(&mut self, chaos_intensity: f32,
                      profile: &ChaosProfile) -> ChaosDelta;
    pub fn apply(base: &MicroDelta, delta: &ChaosDelta) -> MicroDelta;
    pub fn modulate(base: f32, depth: f32, chaos_val: f32,
                    min: f32, max: f32) -> f32;
}
```

---

## 5. Logistic Map

```rust
#[inline]
fn logistic_next(x: f32) -> f32 {
    CHAOS_R * x * (1.0_f32 - x)
}
```

r=3.9: fully chaotic regime without approaching degenerate r=4.0.

---

## 6. Seed Derivation

```rust
pub fn build_seed(project_id: &str, track_id: &str,
                  persona_id: &str) -> u64 {
    let compound = format!("{}:{}:{}", project_id, track_id, persona_id);
    let hash     = sha256(compound.as_bytes());
    u64::from_be_bytes(hash[0..8].try_into().unwrap())
}

fn seed_to_x0(seed: u64) -> f32 {
    (seed as f32 / u64::MAX as f32).clamp(0.001, 0.999)
}

pub fn new(seed: u64) -> Self {
    let mut x = seed_to_x0(seed);
    // 100 warmup iterations decorrelate chaotic state from seed bias
    for _ in 0..CHAOS_WARMUP_ITERS { x = logistic_next(x); }
    Self { state: x }
}
```

---

## 7. Delta Generation

```rust
pub fn next_delta(&mut self, ci: f32, p: &ChaosProfile) -> ChaosDelta {
    let ci = ci.clamp(0.0, 1.0);
    if ci == 0.0 { return ChaosDelta::zero(); }

    macro_rules! step {
        ($max:expr, $depth:expr) => {{
            self.state = logistic_next(self.state);
            Self::modulate(0.0, $max * $depth * ci,
                           self.state, -$max, $max)
        }}
    }

    ChaosDelta {
        stereo_width_mod: step!(CHAOS_WIDTH_MAX,    p.width_depth),
        sat_drive_mod_db: step!(CHAOS_DRIVE_MAX_DB, p.drive_depth),
        comp_release_mod: step!(CHAOS_RELEASE_MAX,  p.release_depth),
        comp_attack_mod:  step!(CHAOS_ATTACK_MAX,   p.attack_depth),
        air_shimmer_db:   step!(CHAOS_SHIMMER_MAX,  p.air_depth),
    }
}

/// RFC-001 Appendix pattern:
/// output = clamp(base + (chaos_val - 0.5) × 2.0 × depth, min, max)
pub fn modulate(base: f32, depth: f32, chaos_val: f32,
                min: f32, max: f32) -> f32 {
    (base + (chaos_val - 0.5) * 2.0 * depth).clamp(min, max)
}
```

---

## 8. Delta Application

```rust
pub fn apply(base: &MicroDelta, delta: &ChaosDelta) -> MicroDelta {
    MicroDelta {
        eq: EqDelta {
            low_shelf_gain_db:  base.eq.low_shelf_gain_db,
            low_shelf_freq_hz:  base.eq.low_shelf_freq_hz,
            // Air shimmer = ±0.5 dB HIGH-SHELF GAIN modulation only
            // Does NOT affect high_shelf_freq_hz
            high_shelf_gain_db: (base.eq.high_shelf_gain_db
                                 + delta.air_shimmer_db)
                .clamp(EQ_GAIN_DELTA_MIN_DB, EQ_GAIN_DELTA_MAX_DB),
            high_shelf_freq_hz: base.eq.high_shelf_freq_hz,
        },
        dynamics: DynamicsDelta {
            comp_threshold_db: base.dynamics.comp_threshold_db,
            comp_ratio:        base.dynamics.comp_ratio,
            comp_attack_ms:    (base.dynamics.comp_attack_ms
                                + delta.comp_attack_mod)
                .clamp(COMP_ATTACK_DELTA_MIN_MS, COMP_ATTACK_DELTA_MAX_MS),
            comp_release_ms:   (base.dynamics.comp_release_ms
                                + delta.comp_release_mod)
                .clamp(COMP_RELEASE_DELTA_MIN_MS, COMP_RELEASE_DELTA_MAX_MS),
        },
        sat: SaturationDelta {
            drive: (base.sat.drive + delta.sat_drive_mod_db)
                .clamp(SAT_DRIVE_DELTA_MIN, SAT_DRIVE_DELTA_MAX),
            mix: base.sat.mix,
        },
        stereo: StereoDelta {
            width: delta.stereo_width_mod
                .clamp(STEREO_WIDTH_DELTA_MIN, STEREO_WIDTH_DELTA_MAX),
        },
    }
}
```

---

## 9. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same project+track+persona → same sequence | ✅ SHA-256 compound seed |
| Same seed → same chaos sequence | ✅ Pure function |
| No rand crate | ✅ Pure arithmetic |
| chaos_intensity=0.0 → zero delta | ✅ Early return |
| profile depth=0.0 → zero delta | ✅ depth × 0 = 0 |
| All outputs bounded | ✅ S-005 bounds |
| Bit-identical | ✅ Pure f32 arithmetic |
| Different persona → different sequence | ✅ persona_id in seed |

---

## 10. Performance Targets

| Metric | Target |
|--------|--------|
| `new()` | < 100μs |
| `next_delta()` | < 1μs |
| `apply()` | < 1μs |
| Memory | < 64 bytes |
| Allocations | Zero |

---

## 11. Contract Tests

```rust
#[test]
fn chaos_same_seed_same_sequence() {
    let p = test_profile();
    let (mut e1, mut e2) = (ChaosEngine::new(42), ChaosEngine::new(42));
    assert_eq!(e1.next_delta(0.5, &p), e2.next_delta(0.5, &p));
}
#[test]
fn chaos_compound_seed_persona_differentiates() {
    let s1 = ChaosEngine::build_seed("p1", "t1", "warm_analog");
    let s2 = ChaosEngine::build_seed("p1", "t1", "clean_punch");
    assert_ne!(s1, s2);
}
#[test]
fn chaos_compound_seed_reproducible() {
    assert_eq!(ChaosEngine::build_seed("p1","t1","warm_analog"),
               ChaosEngine::build_seed("p1","t1","warm_analog"));
}
#[test]
fn chaos_zero_intensity_zero_delta() {
    assert_eq!(ChaosEngine::new(42).next_delta(0.0, &full_profile()),
               ChaosDelta::zero());
}
#[test]
fn chaos_zero_profile_zero_delta() {
    let zero_p = ChaosProfile {
        width_depth:0.0, drive_depth:0.0, release_depth:0.0,
        attack_depth:0.0, air_depth:0.0 };
    assert_eq!(ChaosEngine::new(42).next_delta(1.0, &zero_p),
               ChaosDelta::zero());
}
#[test]
fn chaos_bounds_at_full_intensity() {
    let mut e = ChaosEngine::new(12345);
    for _ in 0..1000 {
        let d = e.next_delta(1.0, &full_profile());
        assert!(d.stereo_width_mod.abs() <= CHAOS_WIDTH_MAX    + 1e-5);
        assert!(d.sat_drive_mod_db.abs() <= CHAOS_DRIVE_MAX_DB + 1e-5);
        assert!(d.comp_release_mod.abs() <= CHAOS_RELEASE_MAX  + 1e-5);
        assert!(d.comp_attack_mod.abs()  <= CHAOS_ATTACK_MAX   + 1e-5);
        assert!(d.air_shimmer_db.abs()   <= CHAOS_SHIMMER_MAX  + 1e-5);
    }
}
#[test]
fn chaos_apply_zero_passthrough() {
    let base = test_base_delta();
    assert_eq!(ChaosEngine::apply(&base, &ChaosDelta::zero()), base);
}
#[test]
fn chaos_apply_air_shimmer_gain_only() {
    let base = test_base_delta();
    let freq_before = base.eq.high_shelf_freq_hz;
    let delta = ChaosDelta { air_shimmer_db: 0.3, ..ChaosDelta::zero() };
    let result = ChaosEngine::apply(&base, &delta);
    assert_eq!(result.eq.high_shelf_freq_hz, freq_before);
}
#[test]
fn chaos_apply_bounds_respected() {
    let base = test_base_delta_maxed();
    let mut e = ChaosEngine::new(99999);
    for _ in 0..100 {
        let r = ChaosEngine::apply(&base, &e.next_delta(1.0, &full_profile()));
        assert!(r.eq.high_shelf_gain_db >= EQ_GAIN_DELTA_MIN_DB);
        assert!(r.eq.high_shelf_gain_db <= EQ_GAIN_DELTA_MAX_DB);
        assert!(r.stereo.width >= STEREO_WIDTH_DELTA_MIN);
        assert!(r.stereo.width <= STEREO_WIDTH_DELTA_MAX);
    }
}
#[test]
fn logistic_unit_interval() {
    let mut x = 0.5_f32;
    for _ in 0..10_000 {
        x = CHAOS_R * x * (1.0 - x);
        assert!(x > 0.0 && x < 1.0);
    }
}
#[test]
fn modulate_symmetric_at_midpoint() {
    assert!((ChaosEngine::modulate(0.3, 0.1, 0.5, -1.0, 1.0) - 0.3).abs() < 1e-6);
}
#[test]
fn chaos_serializable() {
    let d = ChaosDelta {
        stereo_width_mod:0.05, sat_drive_mod_db:0.3,
        comp_release_mod:5.0, comp_attack_mod:1.0, air_shimmer_db:0.2
    };
    assert_eq!(d, serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap());
}
```

---

## 12. Error Handling

| Condition | Behavior |
|-----------|----------|
| seed = 0 or MAX | Clamped to [0.001, 0.999] |
| chaos_intensity out of [0,1] | Clamped silently |
| profile depth out of [0,1] | Clamped silently |
| NaN in base delta | Propagated — S-009 catches |

---

## 13. Implementation Path

```
aether/chaos/
├── mod.rs     ← pub use engine::ChaosEngine
├── engine.rs  ← ChaosEngine, logistic_next(), modulate()
├── delta.rs   ← ChaosDelta, constants
└── seed.rs    ← build_seed() (sha2, pure Rust, MIT)

ChaosProfile lives in:
aether/personas/config.rs  ← S-003 v1.1
```

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-006_chaos_engine.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Same seed → same chaos. Always.*
*Chaos modulates parameters. Never audio.*
