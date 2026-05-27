# S-011a — Multimodal Control Surface

**Document:** `spec/locked/S-011a_multimodal_control.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** UI
**Depends on:** S-003 (Persona Schema), S-004 (Intent Parser), S-005 (Macro → Micro Mapping)
**Used by:** S-009 (Integration Firewall)
**Grounded in:** RFC-002 §1, Pricing Spec v1.1 §2
**Audit:** DeepSeek v0.1 → PASS → LOCKED v1.0

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | LOCKED. N1: MacroDelta application clarified. N2: DspOverrides scope note. N3: Orb mapping independence note. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Multimodal Control Surface defines the Creator OS interaction model:
**Linked Controls** — not Split-Mode.

The engineer operates at two depths simultaneously:
- **Left hand:** Intent Handles (macro) — drives Aether personas
- **Right hand:** Micro knobs (DSP precision) — surgical override
- **Both:** Spatial Orb — stereo/depth/tilt

No mode switching. No cognitive friction. One cockpit, two depths.

**One sentence:** A three-tier control surface where macro intent handles
and micro DSP knobs coexist — always visible, always accessible,
hardware-ready.

---

## 2. Tier Definitions (per Pricing Spec v1.1)

| Tier | Name | UI Elements | Aether Role |
|------|------|-------------|-------------|
| Tier 1 | Black Box | 1 button + 6 mode presets | None — preset only |
| Tier 2 | Medium | Intent Handles + Persona selector + Spatial Orb | Macro → Micro mapping |
| Tier 3 | Pro | Full parameter knobs + Semantic zones + full Orb | Full Aether pipeline |

**Constitutional rule:** Tier only changes UI density — NOT the DSP path.
Same DSP engine runs at all tiers. No audio quality difference between tiers.

---

## 3. Interface

### Tier 1 — Black Box

```rust
pub enum BlackBoxMode {
    Clean, Warm, Punch, Air, Film, Broadcast,
}

pub struct BlackBoxControl { pub mode: BlackBoxMode }

impl BlackBoxControl {
    /// Maps mode to PersonaConfig + default MacroControls.
    /// Compile-time constant mapping — deterministic.
    /// No user-adjustable parameters at this tier.
    pub fn to_intent(&self) -> (PersonaConfig, MacroControls);
}
```

**Mode → Persona mapping (compile-time):**

| Mode | Persona | warmth | punch | forwardness | smoothness |
|------|---------|--------|-------|-------------|------------|
| Clean | `hybrid_hifi` | 0.3 | 0.5 | 0.5 | 0.7 |
| Warm | `warm_analog` | 0.8 | 0.4 | 0.4 | 0.6 |
| Punch | `clean_punch` | 0.3 | 0.9 | 0.7 | 0.3 |
| Air | `hybrid_hifi` | 0.4 | 0.4 | 0.8 | 0.6 |
| Film | `cinematic_wide` | 0.7 | 0.5 | 0.4 | 0.7 |
| Broadcast | `hybrid_hifi` | 0.5 | 0.5 | 0.5 | 0.6 |

---

### Tier 2 — Medium (Linked Controls)

```rust
pub struct MediumControl {
    pub persona_id: String,
    pub macros:     MacroControls,
    pub orb:        OrbPosition,
    pub gain_match: bool,  // A/B gain match (default ON)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OrbPosition {
    /// x: horizontal — stereo width hint [−1.0, +1.0]
    pub x: f32,
    /// y: vertical — perceived depth/forwardness [−1.0, +1.0]
    pub y: f32,
}

impl OrbPosition {
    pub fn center() -> Self { Self { x: 0.0, y: 0.0 } }

    /// Map orb position to macro deltas.
    ///
    /// NOTE: The orb's mapping to forwardness/smoothness is a creative
    /// UI decision and is INDEPENDENT of the persona's macro curves.
    /// It does not reuse S‑005 curve shapes — it is a simple linear
    /// delta applied on top of the current MacroControls values.
    ///
    /// x → smoothness_delta: right = wider/smoother (UI concept only)
    /// y → forwardness_delta: push up = more forward/dry
    pub fn to_macro_delta(&self) -> MacroDelta {
        MacroDelta {
            forwardness_delta: -self.y * 0.3,
            smoothness_delta:   self.x * 0.2,
        }
    }
}

/// Small delta applied on top of MacroControls.
/// Applied by adding to current macro values, then clamping to
/// the active persona's handle bounds before passing to S‑004.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroDelta {
    pub forwardness_delta: f32,  // [−0.3, +0.3]
    pub smoothness_delta:  f32,  // [−0.2, +0.2]
}

impl MacroDelta {
    /// Apply delta to MacroControls.
    /// Result clamped to persona's macro handle bounds (per S‑004 §6).
    pub fn apply(&self, macros: &MacroControls,
                  persona: &PersonaConfig) -> MacroControls {
        MacroControls {
            warmth:      macros.warmth,
            punch:       macros.punch,
            forwardness: (macros.forwardness + self.forwardness_delta)
                .clamp(persona.macros.forwardness.min,
                       persona.macros.forwardness.max),
            smoothness:  (macros.smoothness + self.smoothness_delta)
                .clamp(persona.macros.smoothness.min,
                       persona.macros.smoothness.max),
        }
    }
}
```

**Linked Control behaviour:**

Moving a macro handle adjusts a group of micro parameters (via S-005).
The engineer can unlock any individual parameter for surgical adjustment.

```
Warmth ↑  → low_shelf_gain+, sat_drive+, high_shelf_gain-
Punch ↑   → comp_threshold-, comp_attack-, comp_ratio+
Forward ↑ → high_shelf_gain+, high_shelf_freq+
Smooth ↑  → comp_release+, sat_drive-

[Any parameter can be unlocked for individual override in Tier 3]
```

---

### Tier 3 — Pro (Full Parameters)

```rust
pub struct ProControl {
    pub medium:        MediumControl,
    pub overrides:     DspOverrides,
    pub zones_enabled: bool,  // S-011b
}

/// Direct DSP parameter overrides — applied AFTER Aether processing.
///
/// Overrides are applied after all Aether outputs (persona, macro,
/// chaos, zones) have been combined by S-009. They replace only the
/// specified fields; all other DSP parameters remain as determined
/// by Aether. Chaos and zone adjustments are NOT disabled by overrides.
///
/// None = use Aether value. Some(v) = replace with v (clamped).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DspOverrides {
    pub low_shelf_gain_db:  Option<f32>,
    pub low_shelf_freq_hz:  Option<f32>,
    pub high_shelf_gain_db: Option<f32>,
    pub high_shelf_freq_hz: Option<f32>,
    pub comp_threshold_db:  Option<f32>,
    pub comp_ratio:         Option<f32>,
    pub comp_attack_ms:     Option<f32>,
    pub comp_release_ms:    Option<f32>,
    pub sat_drive:          Option<f32>,
    pub sat_mix:            Option<f32>,
    pub stereo_width:       Option<f32>,
}

impl DspOverrides {
    /// Apply overrides to validated DspConfig from S-009.
    /// Some(v) → replace with v, clamped to constitutional bounds.
    /// None → keep Aether value unchanged.
    pub fn apply(&self, config: &DspConfig) -> DspConfig;
}
```

---

## 4. Control Resolution Order

```
Tier 1:
  BlackBoxMode → (PersonaConfig, MacroControls)
      → S-005 → S-007 → S-006 → S-009 → DspConfig

Tier 2:
  PersonaSelector + MacroControls
      + MacroDelta::apply() [orb]     ← clamped to handle bounds
      → S-004 → S-005 → S-007 → S-006 → S-009 → DspConfig

Tier 3:
  PersonaSelector + MacroControls + OrbPosition
      → S-004 → S-005 → S-007 → S-006 → S-009 → DspConfig
      → DspOverrides::apply() → final DspConfig
```

---

## 5. Spatial Orb — Physics Model

```rust
/// Resistance coefficient [0.0, 1.0] for haptic feedback.
/// 0.0 = free movement, 1.0 = wall (maximum resistance).
pub fn orb_resistance(pos: &OrbPosition) -> f32 {
    let r = libm::sqrtf(pos.x * pos.x + pos.y * pos.y);
    if r < 0.3 {
        0.0
    } else if r < 0.7 {
        (r - 0.3) / 0.4
    } else {
        1.0_f32.min((r - 0.7) / 0.3 + 1.0)
    }
}
```

---

## 6. Hardware Readiness

| UI Element | Hardware Equivalent |
|-----------|---------------------|
| Macro handles | Motorized faders / encoders |
| Micro knobs | Rotary encoders with push-select |
| Spatial Orb | 2D joystick / trackball |
| Persona selector | Rotary switch |
| Mode button (T1) | Single illuminated button |

Instrument-style UI — not plugin-style.

---

## 7. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Tier does not affect DSP | ✅ Same engine all tiers |
| Same macro values → same DspConfig | ✅ Via S-005, S-009 |
| DspOverrides bounded | ✅ Constitutional clamp |
| OrbPosition → deterministic delta | ✅ Pure function |
| BlackBoxMode → deterministic persona | ✅ Compile-time mapping |
| MacroDelta application | ✅ Deterministic add + clamp |

---

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| Handle → DspConfig | < 5ms |
| Orb → delta | < 100μs |
| Override apply | < 100μs |

---

## 9. Contract Tests

```rust
#[test]
fn blackbox_mode_deterministic() {
    let (p1,m1) = BlackBoxControl{mode:BlackBoxMode::Warm}.to_intent();
    let (p2,m2) = BlackBoxControl{mode:BlackBoxMode::Warm}.to_intent();
    assert_eq!(p1.id, p2.id);
    assert_eq!(m1.warmth, m2.warmth);
}
#[test]
fn blackbox_all_modes_valid() {
    for mode in [BlackBoxMode::Clean, BlackBoxMode::Warm,
                 BlackBoxMode::Punch, BlackBoxMode::Air,
                 BlackBoxMode::Film,  BlackBoxMode::Broadcast] {
        let (persona, macros) = BlackBoxControl{mode}.to_intent();
        assert!(PersonaManager::get(&persona.id).is_some());
        assert!((0.0..=1.0).contains(&macros.warmth));
    }
}
#[test]
fn orb_center_zero_delta() {
    let d = OrbPosition::center().to_macro_delta();
    assert_eq!(d.forwardness_delta, 0.0);
    assert_eq!(d.smoothness_delta,  0.0);
}
#[test]
fn orb_delta_apply_clamped() {
    let persona = PersonaManager::get("clean_punch").unwrap();
    let macros  = MacroControls::default();
    let delta   = MacroDelta { forwardness_delta: 99.0, smoothness_delta: 99.0 };
    let result  = delta.apply(&macros, &persona);
    assert!(result.forwardness <= persona.macros.forwardness.max);
    assert!(result.smoothness  <= persona.macros.smoothness.max);
}
#[test]
fn orb_resistance_center_zero() {
    assert_eq!(orb_resistance(&OrbPosition{x:0.0, y:0.0}), 0.0);
}
#[test]
fn orb_resistance_wall_max() {
    assert!(orb_resistance(&OrbPosition{x:1.0, y:0.0}) >= 1.0);
}
#[test]
fn tier_same_macros_same_delta() {
    let persona = PersonaManager::get("warm_analog").unwrap();
    let macros  = MacroControls{warmth:0.8,punch:0.4,
                                 forwardness:0.4,smoothness:0.6};
    assert_eq!(MacroMicroMapper::map(&persona, &macros),
               MacroMicroMapper::map(&persona, &macros));
}
#[test]
fn overrides_none_passthrough() {
    let cfg    = test_dsp_config();
    let result = DspOverrides::default().apply(&cfg);
    assert_eq!(result.eq.low_shelf_gain_db, cfg.eq.low_shelf_gain_db);
}
#[test]
fn overrides_clamped() {
    let mut ov = DspOverrides::default();
    ov.comp_ratio = Some(99.0);
    let result = ov.apply(&test_dsp_config());
    assert!(result.dynamics.comp_ratio <= CFW_COMP_RATIO_MAX);
}
```

---

## 10. Error Handling

| Condition | Behavior |
|-----------|----------|
| Unknown persona in selector | Fall back to default persona |
| Orb position out of [−1,1] | Clamped silently |
| Override value out of bounds | Clamped to constitutional limits |
| MacroDelta out of handle bounds | Clamped to handle min/max |

---

## 11. Implementation Path

```
apps/stillair/cockpit-dioxus/src/
├── components/
│   ├── intent_bay/       ← macro handles (Tier 2/3)
│   ├── transport_bar.rs  ← A/B (existing)
│   └── spatial_orb.rs    ← 2D orb (new)
├── panels/
│   └── tier_selector.rs  ← tier activation
└── control/
    ├── blackbox.rs       ← Tier 1
    ├── medium.rs         ← Tier 2 + MacroDelta
    └── pro.rs            ← Tier 3 + DspOverrides
```

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-011a_multimodal_control.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔒 LOCKED

---

*One cockpit. Two depths. No mode switching.*
*Instrument-style. Hardware-ready. Always.*
