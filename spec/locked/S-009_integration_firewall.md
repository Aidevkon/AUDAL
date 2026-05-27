# S-009 — Integration Firewall (Aether → DSP)

**Document:** `spec/locked/S-009_integration_firewall.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Integration
**Depends on:** S-003, S-004, S-005, S-006, S-007, S-008
**Used by:** S-010 (Execution Proof), E11 DSP (LineOS M1)
**Grounded in:** RFC-002 (Aether Layer Architecture §6, §8)
**Audit:** DeepSeek v0.1 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. N1: ChaosDelta schema validation added. N2: ProofLog note added. N3: zone band order comment. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Integration Firewall is the **only** bridge between Aether
(stochastic/creative) and LineOS (deterministic/DSP). It:

1. Collects all Aether outputs
2. Validates each against its JSON schema
3. Combines them into a single `DspConfig`
4. Clamps all values to constitutional bounds
5. Logs all decisions for S-010 (Execution Proof)
6. Passes `DspConfig` to OpenClaw → M0 → E11

No Aether component may bypass this layer.
No DSP component may receive unvalidated Aether output.

**One sentence:** Collect, validate, combine, clamp, log, and dispatch
all Aether outputs into a single constitutional `DspConfig`.

---

## 2. Constitutional Position

```
Aether outputs:
  PersonaConfig (S-003)    ─┐
  MacroControls (S-004)    ─┤
  MicroDelta (S-005)       ─┤→ IntegrationFirewall (S-009)
  ZoneAdjustments (S-007)  ─┤      │ validate each (schema)
  ChaosDelta (S-006)       ─┘      │ combine
                                   │ clamp (constitutional)
                                   │ log → ProofLog (S-010)
                                   ↓
                             DspConfig (validated, immutable)
                                   ↓
                          OpenClaw → M0 → E11 (LineOS)
```

**Rules (RFC-002 §6, §8):**
- No Aether component may call DSP directly
- Every Aether output validated against JSON schema before use
- All values clamped to constitutional bounds (second line of defence)
- All decisions logged for execution proof (S-010)
- Failure modes: E_MACRO_OUT_OF_RANGE, E_SCHEMA_FAIL, E_FIREWALL_CLAMP
- One `DspConfig` per render — immutable after dispatch

---

## 3. Interface

### Constitutional DSP Bounds

```rust
pub const CFW_EQ_GAIN_MIN_DB:        f32 = -18.0;
pub const CFW_EQ_GAIN_MAX_DB:        f32 =  12.0;
pub const CFW_EQ_FREQ_MIN_HZ:        f32 =  20.0;
pub const CFW_EQ_FREQ_MAX_HZ:        f32 =  20_000.0;
pub const CFW_EQ_Q_MIN:              f32 =  0.1;
pub const CFW_EQ_Q_MAX:              f32 =  10.0;
pub const CFW_COMP_THRESHOLD_MIN_DB: f32 = -60.0;
pub const CFW_COMP_THRESHOLD_MAX_DB: f32 =  0.0;
pub const CFW_COMP_RATIO_MIN:        f32 =  1.0;
pub const CFW_COMP_RATIO_MAX:        f32 =  20.0;
pub const CFW_COMP_ATTACK_MIN_MS:    f32 =  0.1;
pub const CFW_COMP_ATTACK_MAX_MS:    f32 =  200.0;
pub const CFW_COMP_RELEASE_MIN_MS:   f32 =  10.0;
pub const CFW_COMP_RELEASE_MAX_MS:   f32 =  2000.0;
pub const CFW_SAT_DRIVE_MIN:         f32 =  0.0;
pub const CFW_SAT_DRIVE_MAX:         f32 =  1.0;
pub const CFW_SAT_MIX_MIN:           f32 =  0.0;
pub const CFW_SAT_MIX_MAX:           f32 =  1.0;
pub const CFW_STEREO_WIDTH_MIN:      f32 =  0.5;
pub const CFW_STEREO_WIDTH_MAX:      f32 =  1.5;
```

### DspConfig

```rust
/// Final DSP configuration passed to E11.
/// All values guaranteed within constitutional bounds.
/// Immutable after creation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspConfig {
    pub eq:         DspEqConfig,
    pub dynamics:   DspDynamicsConfig,
    pub sat:        DspSatConfig,
    pub stereo:     DspStereoConfig,
    pub persona_id: String,   // for execution proof (S-010)
    pub chaos_seed: u64,      // for execution proof (S-010)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspEqConfig {
    pub low_shelf_gain_db:  f32,
    pub low_shelf_freq_hz:  f32,
    pub high_shelf_gain_db: f32,
    pub high_shelf_freq_hz: f32,
    /// Zone adjustment bands from S-007.
    /// Preserved in center_hz-sorted order (S-007 guarantees sort).
    pub zone_bands: Vec<ZoneBand>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ZoneBand {
    pub center_hz: f32,
    pub gain_db:   f32,
    pub q:         f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspDynamicsConfig {
    pub comp_threshold_db: f32,
    pub comp_ratio:        f32,
    pub comp_attack_ms:    f32,
    pub comp_release_ms:   f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspSatConfig {
    pub drive: f32,
    pub mix:   f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DspStereoConfig {
    pub width: f32,
}
```

### FirewallError

```rust
#[derive(Debug, serde::Serialize)]
pub enum FirewallError {
    /// E_MACRO_OUT_OF_RANGE — macro value outside [0,1]
    MacroOutOfRange { field: String, value: f32 },
    /// E_SCHEMA_FAIL — output failed JSON schema validation
    SchemaFail { source: String, detail: String },
    /// E_FIREWALL_CLAMP — value clamped (non-fatal, logged only)
    FirewallClamp { field: String, original: f32, clamped: f32 },
}
```

Note: `E_FIREWALL_CLAMP` is non-fatal — logged, execution continues.
`MacroOutOfRange` and `SchemaFail` are fatal — render aborted.

Note: `E_INTENT_PARSE` and `E_PERSONA_INVALID` are upstream errors
(S-004, S-003). They are not raised by the firewall itself.

### ProofLog

```rust
// ProofLog is defined in S-010 (Execution Proof).
// This spec assumes its interface:
//   proof_log.record_clamp(FirewallError::FirewallClamp { ... })
//   proof_log.record_dsp_config(...)
//   proof_log.clamp_count() -> usize
pub struct ProofLog { /* defined in S-010 */ }
```

### IntegrationFirewall

```rust
pub struct IntegrationFirewall;

impl IntegrationFirewall {
    pub fn build(
        persona:     &PersonaConfig,
        macros:      &MacroControls,
        micro_delta: &MicroDelta,
        zone_adj:    &ZoneAdjustments,
        chaos_delta: &ChaosDelta,
        chaos_seed:  u64,
        proof_log:   &mut ProofLog,
    ) -> Result<DspConfig, FirewallError>;

    pub fn validate_schema<T: Serialize>(
        value:       &T,
        schema_name: &str,
    ) -> Result<(), FirewallError>;

    pub fn clamp_log(value: f32, min: f32, max: f32,
                     field: &str,
                     proof_log: &mut ProofLog) -> f32;
}
```

---

## 4. Build Pipeline

```rust
pub fn build(
    persona:     &PersonaConfig,
    macros:      &MacroControls,
    micro_delta: &MicroDelta,
    zone_adj:    &ZoneAdjustments,
    chaos_delta: &ChaosDelta,
    chaos_seed:  u64,
    proof_log:   &mut ProofLog,
) -> Result<DspConfig, FirewallError> {

    // Step 1: Validate all Aether outputs against schemas
    Self::validate_schema(persona,     "persona.schema.json")?;
    Self::validate_schema(micro_delta, "dsp_config.schema.json")?;
    Self::validate_schema(zone_adj,    "zone.schema.json")?;
    // ChaosDelta validated against its own schema
    Self::validate_schema(chaos_delta, "chaos.schema.json")?;

    // Step 2: Validate macro controls [0.0, 1.0]
    for (field, val) in [
        ("warmth",      macros.warmth),
        ("punch",       macros.punch),
        ("forwardness", macros.forwardness),
        ("smoothness",  macros.smoothness),
    ] {
        if !(0.0_f32..=1.0_f32).contains(&val) {
            return Err(FirewallError::MacroOutOfRange {
                field: field.into(), value: val
            });
        }
    }

    // Step 3: Combine PersonaDspBase + MicroDelta + ChaosDelta
    let base = &persona.dsp_base;

    let raw_low_gain  = base.low_shelf_gain_db
                      + micro_delta.eq.low_shelf_gain_db;
    let raw_low_freq  = base.low_shelf_freq_hz
                      + micro_delta.eq.low_shelf_freq_hz;
    let raw_high_gain = base.high_shelf_gain_db
                      + micro_delta.eq.high_shelf_gain_db
                      + chaos_delta.air_shimmer_db;
    let raw_high_freq = base.high_shelf_freq_hz
                      + micro_delta.eq.high_shelf_freq_hz;

    let raw_threshold = base.comp_threshold_db
                      + micro_delta.dynamics.comp_threshold_db;
    let raw_ratio     = base.comp_ratio
                      + micro_delta.dynamics.comp_ratio;
    let raw_attack    = base.comp_attack_ms
                      + micro_delta.dynamics.comp_attack_ms
                      + chaos_delta.comp_attack_mod;
    let raw_release   = base.comp_release_ms
                      + micro_delta.dynamics.comp_release_ms
                      + chaos_delta.comp_release_mod;

    let raw_drive     = base.saturation_drive
                      + micro_delta.sat.drive
                      + chaos_delta.sat_drive_mod_db;
    let raw_mix       = base.saturation_mix
                      + micro_delta.sat.mix;

    let raw_width     = base.stereo_width
                      + micro_delta.stereo.width  // always 0.0 from S-005
                      + chaos_delta.stereo_width_mod;

    // Step 4: Constitutional clamp — log any violations
    let eq = DspEqConfig {
        low_shelf_gain_db:  Self::clamp_log(raw_low_gain,
            CFW_EQ_GAIN_MIN_DB,  CFW_EQ_GAIN_MAX_DB,  "low_shelf_gain",  proof_log),
        low_shelf_freq_hz:  Self::clamp_log(raw_low_freq,
            CFW_EQ_FREQ_MIN_HZ,  CFW_EQ_FREQ_MAX_HZ,  "low_shelf_freq",  proof_log),
        high_shelf_gain_db: Self::clamp_log(raw_high_gain,
            CFW_EQ_GAIN_MIN_DB,  CFW_EQ_GAIN_MAX_DB,  "high_shelf_gain", proof_log),
        high_shelf_freq_hz: Self::clamp_log(raw_high_freq,
            CFW_EQ_FREQ_MIN_HZ,  CFW_EQ_FREQ_MAX_HZ,  "high_shelf_freq", proof_log),
        // Zone bands preserved in center_hz-sorted order (S-007 guarantees sort)
        zone_bands: zone_adj.bands.iter().map(|b| ZoneBand {
            center_hz: Self::clamp_log(b.center_hz,
                CFW_EQ_FREQ_MIN_HZ, CFW_EQ_FREQ_MAX_HZ, "zone_center", proof_log),
            gain_db:   Self::clamp_log(b.gain_db,
                CFW_EQ_GAIN_MIN_DB, CFW_EQ_GAIN_MAX_DB, "zone_gain",   proof_log),
            q:         Self::clamp_log(b.q,
                CFW_EQ_Q_MIN,       CFW_EQ_Q_MAX,       "zone_q",      proof_log),
        }).collect(),
    };

    let dynamics = DspDynamicsConfig {
        comp_threshold_db: Self::clamp_log(raw_threshold,
            CFW_COMP_THRESHOLD_MIN_DB, CFW_COMP_THRESHOLD_MAX_DB,
            "comp_threshold", proof_log),
        comp_ratio:        Self::clamp_log(raw_ratio,
            CFW_COMP_RATIO_MIN, CFW_COMP_RATIO_MAX,
            "comp_ratio", proof_log),
        comp_attack_ms:    Self::clamp_log(raw_attack,
            CFW_COMP_ATTACK_MIN_MS, CFW_COMP_ATTACK_MAX_MS,
            "comp_attack", proof_log),
        comp_release_ms:   Self::clamp_log(raw_release,
            CFW_COMP_RELEASE_MIN_MS, CFW_COMP_RELEASE_MAX_MS,
            "comp_release", proof_log),
    };

    let sat = DspSatConfig {
        drive: Self::clamp_log(raw_drive,
            CFW_SAT_DRIVE_MIN, CFW_SAT_DRIVE_MAX, "sat_drive", proof_log),
        mix:   Self::clamp_log(raw_mix,
            CFW_SAT_MIX_MIN,   CFW_SAT_MIX_MAX,   "sat_mix",   proof_log),
    };

    let stereo = DspStereoConfig {
        width: Self::clamp_log(raw_width,
            CFW_STEREO_WIDTH_MIN, CFW_STEREO_WIDTH_MAX,
            "stereo_width", proof_log),
    };

    // Step 5: Log final config to proof log (S-010)
    proof_log.record_dsp_config(&eq, &dynamics, &sat, &stereo);

    Ok(DspConfig {
        eq, dynamics, sat, stereo,
        persona_id: persona.id.clone(),
        chaos_seed,
    })
}
```

---

## 5. Clamp + Log

```rust
pub fn clamp_log(value: f32, min: f32, max: f32,
                  field: &str, proof_log: &mut ProofLog) -> f32 {
    let clamped = value.clamp(min, max);
    if (clamped - value).abs() > 1e-6 {
        proof_log.record_clamp(FirewallError::FirewallClamp {
            field:    field.to_string(),
            original: value,
            clamped,
        });
    }
    clamped
}
```

---

## 6. Combination Logic

```
Final DSP value = PersonaDspBase + MicroDelta + ChaosDelta
Zone bands      = ZoneAdjustments.bands (additive, separate EQ bands)
```

Applied in order:
1. `PersonaDspBase` (S-003) — persona's base values
2. `+ MicroDelta` (S-005) — macro-driven adjustments
3. `+ ChaosDelta` (S-006) — micro-variation
4. Zone bands passed through as separate EQ bands in `DspEqConfig.zone_bands`

---

## 7. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same inputs → same DspConfig | ✅ Pure function |
| No hidden state | ✅ Stateless |
| All outputs bounded | ✅ Constitutional clamp |
| Schema validated | ✅ Every input |
| All decisions logged | ✅ ProofLog |
| Clamp non-fatal | ✅ Logged, continues |

---

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| `build()` | < 1ms |
| Schema validation | < 200μs per schema |
| Memory | < 50 KB |

---

## 9. Contract Tests

```rust
#[test]
fn firewall_build_deterministic() {
    let i = test_firewall_inputs();
    let (mut l1, mut l2) = (ProofLog::new(), ProofLog::new());
    assert_eq!(
        IntegrationFirewall::build(&i.persona, &i.macros, &i.micro,
            &i.zones, &i.chaos, 42, &mut l1).unwrap(),
        IntegrationFirewall::build(&i.persona, &i.macros, &i.micro,
            &i.zones, &i.chaos, 42, &mut l2).unwrap()
    );
}
#[test]
fn firewall_constitutional_bounds_respected() {
    let persona = PersonaManager::default();
    let mut log = ProofLog::new();
    let cfg = IntegrationFirewall::build(
        &persona, &MacroControls { warmth:1.0, punch:1.0,
                                   forwardness:1.0, smoothness:1.0 },
        &extreme_micro_delta(), &ZoneAdjustments::empty(),
        &extreme_chaos_delta(), 0, &mut log
    ).unwrap();
    assert!(cfg.eq.low_shelf_gain_db  >= CFW_EQ_GAIN_MIN_DB);
    assert!(cfg.eq.low_shelf_gain_db  <= CFW_EQ_GAIN_MAX_DB);
    assert!(cfg.dynamics.comp_ratio   >= CFW_COMP_RATIO_MIN);
    assert!(cfg.dynamics.comp_ratio   <= CFW_COMP_RATIO_MAX);
    assert!(cfg.sat.drive             >= CFW_SAT_DRIVE_MIN);
    assert!(cfg.sat.drive             <= CFW_SAT_DRIVE_MAX);
    assert!(cfg.stereo.width          >= CFW_STEREO_WIDTH_MIN);
    assert!(cfg.stereo.width          <= CFW_STEREO_WIDTH_MAX);
}
#[test]
fn firewall_clamp_logged_not_fatal() {
    let mut log = ProofLog::new();
    let result = IntegrationFirewall::build(
        &PersonaManager::default(), &MacroControls::default(),
        &extreme_micro_delta(), &ZoneAdjustments::empty(),
        &ChaosDelta::zero(), 0, &mut log
    );
    assert!(result.is_ok());
    assert!(log.clamp_count() > 0);
}
#[test]
fn firewall_invalid_macro_fatal() {
    let mut log = ProofLog::new();
    let result = IntegrationFirewall::build(
        &PersonaManager::default(),
        &MacroControls { warmth:2.0, punch:0.5,
                         forwardness:0.5, smoothness:0.5 },
        &MicroDelta::zero(), &ZoneAdjustments::empty(),
        &ChaosDelta::zero(), 0, &mut log
    );
    assert!(matches!(result, Err(FirewallError::MacroOutOfRange { .. })));
}
#[test]
fn firewall_zone_bands_passed_through() {
    let mut log = ProofLog::new();
    let zones   = ZoneAdjustments {
        bands: vec![ZoneAdjustment { center_hz:3000.0, gain_db:1.5, q:0.7 }]
    };
    let cfg = IntegrationFirewall::build(
        &PersonaManager::default(), &MacroControls::default(),
        &MicroDelta::zero(), &zones, &ChaosDelta::zero(), 0, &mut log
    ).unwrap();
    assert_eq!(cfg.eq.zone_bands.len(), 1);
    assert!((cfg.eq.zone_bands[0].center_hz - 3000.0).abs() < 1e-4);
}
#[test]
fn firewall_persona_id_and_seed_in_config() {
    let persona = PersonaManager::get("warm_analog").unwrap();
    let mut log = ProofLog::new();
    let cfg = IntegrationFirewall::build(
        &persona, &MacroControls::default(),
        &MicroDelta::zero(), &ZoneAdjustments::empty(),
        &ChaosDelta::zero(), 42, &mut log
    ).unwrap();
    assert_eq!(cfg.persona_id, "warm_analog");
    assert_eq!(cfg.chaos_seed, 42);
}
#[test]
fn firewall_combination_order_correct() {
    let persona    = PersonaManager::get("warm_analog").unwrap();
    let base_gain  = persona.dsp_base.low_shelf_gain_db;
    let mut micro  = MicroDelta::zero();
    micro.eq.low_shelf_gain_db = 1.0;
    let mut log    = ProofLog::new();
    let cfg = IntegrationFirewall::build(
        &persona, &MacroControls::default(),
        &micro, &ZoneAdjustments::empty(),
        &ChaosDelta::zero(), 0, &mut log
    ).unwrap();
    let expected = (base_gain + 1.0)
        .clamp(CFW_EQ_GAIN_MIN_DB, CFW_EQ_GAIN_MAX_DB);
    assert!((cfg.eq.low_shelf_gain_db - expected).abs() < 1e-4);
}
#[test]
fn firewall_config_serializable() {
    let mut log = ProofLog::new();
    let cfg = IntegrationFirewall::build(
        &PersonaManager::default(), &MacroControls::default(),
        &MicroDelta::zero(), &ZoneAdjustments::empty(),
        &ChaosDelta::zero(), 0, &mut log
    ).unwrap();
    let _: DspConfig = serde_json::from_str(
        &serde_json::to_string(&cfg).unwrap()).unwrap();
}
```

---

## 10. Error Handling

| Error | Fatal? | Action |
|-------|--------|--------|
| E_MACRO_OUT_OF_RANGE | ✅ Yes | Abort render |
| E_SCHEMA_FAIL | ✅ Yes | Abort render |
| E_FIREWALL_CLAMP | ❌ No | Log + continue |

---

## 11. Implementation Path

```
integration/
├── mod.rs       ← pub use firewall::IntegrationFirewall
├── firewall.rs  ← build(), validate_schema(), clamp_log()
├── config.rs    ← DspConfig, all sub-structs, constants
├── error.rs     ← FirewallError
└── proof_log.rs ← ProofLog stub (full impl in S-010)
```

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-009_integration_firewall.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*Validate everything. Clamp everything. Log everything.*
*No Aether output reaches DSP without passing through here.*
