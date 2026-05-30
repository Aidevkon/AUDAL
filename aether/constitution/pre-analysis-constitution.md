# Pre-Analysis Constitution v1.3
# Creator OS — Cockpit Constitution Layer

**Document:** `aether/constitution/pre-analysis-constitution.md`
**Version:** 1.3
**Date:** 2026-05-30
**Authority:** Creator OS Constitution v2.5 · Aether Constitution v1.0 · S-002 StemFeatureAnalyzer
**Owner:** Lead Architect (Anestis)
**Status:** 🔒 LOCKED — pending Lead Architect approval

---

## 0. Purpose

The Pre-Analysis Engine runs **once per mastering session**, on the full
stereo file, **before** NMF stem separation and **before** the Aether layer.
It is a static, session-start measurement — not a real-time processor.

These features serve two purposes:

**Purpose 1 — Feed Aether:**
- Activate semantic EQ zones (cymbal harshness, sub rumble, boxiness)
- Inform persona macro defaults
- Seed Chaos Engine (via SHA-256)

**Purpose 2 — Drive MFD Intelligence (Tier 3):**
- MFD 1 Signal Analyzer: LUFS, true peak, dynamic range, spectral balance
- MFD 2 Spatial Telemetry: phase correlation, stereo width, mono-maker default
- MFD 3 DSP Chain: compressor attack defaults, resonant peak nodes

The Pre-Analysis Engine does NOT:
- Modify DSP
- Run NMF stem separation
- Replace per-stem analysis (runs after NMF)
- Interact with Wizard or JINI
- Produce per-stem metrics (those come only from NMF + per-stem analysis after S-001 wiring)
- Run in real-time on audio chunks
- Control DSP parameters dynamically during processing

---

## 1. Position in Pipeline

```
AUDIO IN (raw stereo PCM)
        ↓
PRE-ANALYSIS ENGINE ← this document
        ↓
NMF STEM SEPARATION
        ↓
PER-STEM ANALYSIS
        ↓
AETHER LAYER
```

Pre-Analysis runs once per mastering session, on the full stereo mix,
before any DSP processing begins.

---

## 2. What Already Exists

The following features are already implemented in
`sp314-dsp/src/analysis/` and produce real data:

| Feature | Location | Status |
|---------|----------|--------|
| `integrated_lufs` | `metering/lufs.rs` | ✅ Real |
| `stereo_correlation` | `analysis/stereo.rs` | ✅ Real |
| `stereo_width` | `analysis/stereo.rs` | ✅ Real |
| `dynamic_range_db` | `analysis/dynamics.rs` | ✅ Real |
| `spectral_centroid_hz` | `analysis/spectral.rs` | ✅ Real |
| `spectral_flatness` | `analysis/spectral.rs` | ✅ Real |
| `spectral_crest_factor_db` | `analysis/spectral.rs` | ✅ Real |
| `crest_factor_db` | `analysis/dynamics.rs` | ✅ Real |
| `rms_db` | `analysis/dynamics.rs` | ✅ Real |
| `true_peak_dbtp` | hardcoded `-1.0` | 🔨 MUST IMPLEMENT (PA-04) |
| `loudness_range` | hardcoded `0.0` | 🔨 MUST IMPLEMENT (PA-05) |

The Pre-Analysis Engine builds on these. It does not replace them.

---

## 3. PreAnalysisData Struct (Full Output Contract)

> **Oracle-TDD note:** This struct is defined first as a Python dataclass
> in the oracle script. The JSON fixture serialized from Python becomes the
> serde contract. The Rust struct below must deserialize from that JSON
> identically. No field additions or removals without constitution amendment.

```rust
pub struct PreAnalysisData {

    // ── Dynamics Baseline ──────────────────────────────────────────
    pub integrated_lufs:         f32,   // ITU-R BS.1770-4
    pub true_peak_dbtp:          f32,   // 4x oversampling polyphase FIR
    pub loudness_range:          f32,   // LRA — dynamic movement
    pub dynamic_range_db:        f32,   // p95-p5 block RMS
    pub global_crest_factor_db:  f32,   // peak / RMS ratio

    // ── Tonal Balance (6-band RMS, normalised to total RMS, dB) ───
    // [Sub, Bass, LowMid, Mid, HighMid, Air]
    // Sub:      20–80 Hz
    // Bass:     80–250 Hz
    // LowMid:   250–500 Hz     ← mud detection zone
    // Mid:      500–2000 Hz    ← vocal presence zone
    // HighMid:  2000–8000 Hz   ← harshness detection zone
    // Air:      8000–20000 Hz  ← high shelf / air zone
    pub spectral_profile_db:     [f32; 6],

    // ── Spectral Shape ─────────────────────────────────────────────
    pub spectral_rolloff_hz:     f32,   // freq below which 85% energy

    // ── Transient Profile ──────────────────────────────────────────
    pub transient_density:       f32,   // events/sec above threshold

    // ── Spatial Health ─────────────────────────────────────────────
    pub global_phase_correlation: f32,  // -1.0 → +1.0
    pub side_mid_ratio_db:        f32,  // Side RMS - Mid RMS in dB
    pub stereo_width:             f32,  // 0.0 → 1.0

    // ── Per-Band Phase Correlation (for Mono-Maker intelligence) ───
    // Same 6 bands as spectral_profile_db
    pub band_phase_correlation:  [f32; 6],

    // ── Resonant Peak Flags (for MFD frequency audition nodes) ─────
    // Frequencies where amplitude > +3σ above local moving average
    // Sorted ascending. Max 16 peaks (capped to prevent unbounded alloc).
    pub resonant_peaks_hz:       Vec<f32>,

    // ── Zone Activation Flags ──────────────────────────────────────
    pub zone_flags:              ZoneActivationFlags,
}
```

### 3.1 Why 6 Bands (Not 5)

v1.2 used 5 bands with `LowMid(250–2kHz)` as a single band. This was
revised in v1.3 because:

1. **Mud vs vocal presence are different problems.** The Ambience spec's
   `ambience_mud` finding references 250–500Hz specifically. Vocal masking
   occurs at 500Hz–2kHz. A single 250–2k band cannot distinguish them.

2. **Zone activation needs granularity.** `zone_boxiness` triggers on
   elevated LowMid (250–500Hz) energy. `zone_vocal_recessed` (future)
   would trigger on depressed Mid (500–2kHz) energy. Same band = cannot
   detect both independently.

3. **Consistency with Ambience stem spec.** The Ambience spec §5 references
   `DSP_NoVocalMask` (500Hz–2kHz) and `DSP_NoMudAccumulation` (250–500Hz)
   as separate DSP rules with separate frequency ranges.

### 3.2 Resonant Peaks — Vec<f32> Rationale

`resonant_peaks_hz` is dynamically sized because the number of resonant
peaks is signal-dependent (0 for clean masters, 5+ for problematic sources).
Capped at 16 entries to prevent unbounded allocation. `lineos-types` already
depends on `serde` + `serde_json` (which implies `alloc`), so `Vec<f32>`
adds no new dependency.

---

## 4. Feature Definitions and Algorithms

### 4.1 Integrated LUFS
- **Algorithm:** ITU-R BS.1770-4
- **Existing:** `metering/lufs.rs` — use as-is
- **Range:** -24 (dynamic) to -6 (dense EDM/metal)
- **Feeds:** Limiter ceiling default, MFD 1

### 4.2 True Peak
- **Algorithm:** 4x oversampling polyphase FIR (deterministic)
- **Existing:** Hardcoded -1.0 — MUST implement (PA-04)
- **Range:** -3.0 to 0.0 dBTP
- **Feeds:** MFD 1 ceiling display
- **Oracle:** Python computes via `scipy.signal.resample_poly(signal, 4, 1)` → `max(abs())`

### 4.3 Loudness Range (LRA)
- **Algorithm:** ITU-R BS.1770-4 LRA measurement
  (400ms blocks, 100ms hop, gated relative -20 LU)
- **Existing:** Hardcoded 0.0 — MUST implement (PA-05)
- **Range:** 3 LU (compressed) to 14+ LU (dynamic)
- **Feeds:** Boxiness zone activation, MFD 1

### 4.4 N-Band Spectral RMS (6 bands)
- **Algorithm:** Cascaded Butterworth IIR filters (4th order)
  at crossover points: [80, 250, 500, 2000, 8000] Hz
- **Existing:** Not implemented (PA-06)
- **Bands:**

| Index | Name | Range | Zone reference |
|-------|------|-------|----------------|
| 0 | Sub | 20–80 Hz | Ambience §5 `zone_sub_rumble` |
| 1 | Bass | 80–250 Hz | Harmonics §6 |
| 2 | LowMid | 250–500 Hz | Ambience §5 `ambience_mud`, §5 `DSP_NoMudAccumulation` |
| 3 | Mid | 500–2000 Hz | Ambience §5 `DSP_NoVocalMask` |
| 4 | HighMid | 2000–8000 Hz | Harmonics §6 `harmonics_harshness` |
| 5 | Air | 8000–20000 Hz | Ambience §4.1 "high shelf" |

- **Output:** RMS energy per band normalised to total RMS (dB)
- **Feeds:** Semantic zone activation, Aether tonal decisions
- **Oracle:** Python uses `scipy.signal.butter` + `sosfilt` for each band

### 4.5 Global Crest Factor
- **Algorithm:** `CF = 20 × log10(peak / rms)`
- **Existing:** `analysis/dynamics.rs` — use as-is
- **Range:** 8 dB (squashed) to 14+ dB (percussive)
- **Feeds:** Compressor attack defaults on MFD 3

### 4.6 Transient Density
- **Algorithm:** Fast MA envelope (~10ms) vs slow MA envelope (~100ms).
  Count events where fast > slow by +6 dB threshold.
  Output: events per second.
- **Existing:** Not implemented (PA-07)
- **Range:** 0 (pad/drone) to 8+ (heavy percussion)
- **Feeds:** Aether dynamics decisions
- **Oracle:** Python computes dual MA on `np.abs(signal)`, counts crossings

### 4.7 Global Phase Correlation
- **Algorithm:** Normalised dot product of L and R channels
  `r = Σ(Li × Ri) / √(ΣLi² × ΣRi²)`
- **Existing:** `analysis/stereo.rs` — use as-is
- **Range:** -1.0 (full cancellation) to +1.0 (mono)
  Healthy range: +0.4 to +0.8
- **Feeds:** MFD 2, mono-maker default

### 4.8 Side/Mid Ratio
- **Algorithm:**
  `M = (L + R) / 2`, `S = (L - R) / 2`
  `ratio_db = 20 × log10(RMS_linear(S) / RMS_linear(M))` in dB
  (compute RMS in linear domain, then convert to dB)
- **Existing:** Not implemented (PA-08)
- **Range:** -12 dB (narrow) to 0 dB (equal M/S)
  Healthy range: -6 dB to -12 dB
- **Feeds:** M/S trim suggestion on MFD 2

### 4.9 Per-Band Phase Correlation (6 bands)
- **Algorithm:** Same as §4.7 but applied per band after
  Butterworth IIR split (same crossovers as §4.4)
- **Existing:** Not implemented (PA-09)
- **Range:** Same as global correlation, per band
- **Feeds:** Mono-Maker auto-snap frequency

### 4.10 Resonant Peak Detection
- **Algorithm:** FFT over full signal.
  For each bin: compute local moving average (±50 bins).
  Flag bins where amplitude > local_avg + 3σ.
  Convert flagged bins to Hz. Sort ascending. Cap at 16.
- **Existing:** Not implemented (PA-10)
- **Output:** Vec<f32> of flagged frequencies in Hz (max 16)
- **Feeds:** MFD 1 frequency audition glow nodes

### 4.11 Spectral Rolloff (85%)
- **Algorithm:** Cumulative sum of STFT magnitude bins.
  Find frequency bin where cumulative sum reaches 85% of total.
  Convert bin index to Hz.
- **Existing:** Not implemented (PA-11)
- **Range:** 2000 Hz (dark/bass-heavy) to 16000+ Hz (bright/airy)
- **Feeds:** Aether tonal balance decisions
- **Oracle:** Python uses `librosa.feature.spectral_rolloff` or manual
  cumulative sum on `np.fft.rfft` magnitudes

---

## 5. Zone Activation Logic

The Pre-Analysis output activates semantic EQ zones in Aether
via `SemanticZoneResolver` (S-007).

| Zone | Condition | Constitution ref | Used By |
|------|-----------|-----------------|---------|
| `zone_cymbal_harsh` | HighMid (band 4) RMS > -18 dBFS AND crest factor < 8 dB | §4.4, §4.5 | Aether EQ (2–8kHz cut) |
| `zone_sub_rumble` | Sub (band 0) RMS > -30 dBFS | §4.4 | Aether high-pass filter |
| `zone_boxiness` | LowMid (band 2) RMS > -20 dBFS AND LRA < 5 LU | §4.4, §4.3 | Aether dynamic EQ (250–500Hz) |
| `zone_phase_issue` | Global phase correlation < 0.3 | §4.7 | Wizard Finding (HUD) |
| `zone_harsh_resonance` | Any resonant_peaks_hz in HighMid band (2k–8kHz) | §4.10 | Wizard Finding + MFD 1 glow |

**Output flags to Aether (boolean):**

```rust
pub struct ZoneActivationFlags {
    pub zone_cymbal_harsh:    bool,
    pub zone_sub_rumble:      bool,
    pub zone_boxiness:        bool,
    pub zone_phase_issue:     bool,
    pub zone_harsh_resonance: bool,
}
```

**Zone threshold constants (compile-time only, INV-PA-11):**

```rust
pub const ZONE_SUB_RUMBLE_THRESHOLD_DB:      f32 = -30.0;
pub const ZONE_CYMBAL_HARSH_RMS_DB:          f32 = -18.0;
pub const ZONE_CYMBAL_HARSH_CREST_DB:        f32 = 8.0;
pub const ZONE_BOXINESS_RMS_DB:              f32 = -20.0;
pub const ZONE_BOXINESS_LRA_LU:              f32 = 5.0;
pub const ZONE_PHASE_ISSUE_CORRELATION:      f32 = 0.3;
```

---

## 6. MFD Intelligence Defaults

Pre-Analysis data drives intelligent default states for Tier 3 MFD controls.
All defaults are **suggestions only** — user may override.

### MFD 1 — Signal Analyzer
| Control | Pre-Analysis Input | Intelligence |
|---------|-------------------|--------------|
| Target LUFS offset | `integrated_lufs` | Default offset = distance to -14 LUFS |
| Limiter ceiling | `true_peak_dbtp` | Auto-set to prevent clipping |
| Frequency audition nodes | `resonant_peaks_hz` | Glowing nodes at +3σ peaks |

### MFD 2 — Spatial Telemetry
| Control | Pre-Analysis Input | Intelligence |
|---------|-------------------|--------------|
| Mono-Maker frequency | `band_phase_correlation` | Auto-snap to highest freq where correlation < 0.3 |
| M/S trims | `side_mid_ratio_db` | Highlight fader if delta > 4dB from persona target |
| M/S/L/R solo | `stereo_width` | Indicate if side energy is unhealthy |

### MFD 3 — DSP Chain
| Control | Pre-Analysis Input | Intelligence |
|---------|-------------------|--------------|
| Compressor attack | `global_crest_factor_db` | High CF (>12dB) → slow attack (30ms). Low CF (<8dB) → fast (1ms), ratio 1.2:1 |
| Compressor ratio | `transient_density` | High density → lower ratio to preserve transients |

---

## 7. Implementation Notes

### What to build (not yet implemented)
- True peak measurement (4x oversampling FIR) — PA-04
- LRA calculator (BS.1770-4 gated) — PA-05
- N-band spectral RMS (6-band Butterworth IIR) — PA-06
- Transient density (dual MA envelope) — PA-07
- Side/Mid ratio — PA-08
- Per-band phase correlation (6 bands) — PA-09
- Resonant peak detection (FFT + 3σ) — PA-10
- Spectral rolloff (85%) — PA-11

### What to reuse (already implemented)
- `integrated_lufs` → `metering/lufs.rs`
- `stereo_correlation` → `analysis/stereo.rs`
- `stereo_width` → `analysis/stereo.rs`
- `dynamic_range_db` → `analysis/dynamics.rs`
- `global_crest_factor_db` → `analysis/dynamics.rs`

### Entry point
```rust
pub fn run(left: &[f32], right: &[f32], sample_rate: u32) -> PreAnalysisData
```
Called once per session from M0, before NMF and before Aether.

---

## 8. Invariants (Unbreakable)

| ID | Invariant |
|----|-----------|
| INV-PA-1 | Pre-Analysis is deterministic — same input = same output always, including bit-exact floating point results across platforms (same compiler flags, same FPU control word, no FMA nondeterminism). CI must enforce `-C target-cpu=baseline` or equivalent. |
| INV-PA-2 | Pre-Analysis runs before NMF — never after |
| INV-PA-3 | Pre-Analysis runs before Aether — never inside it |
| INV-PA-4 | Pre-Analysis does not modify DSP |
| INV-PA-5 | Pre-Analysis does not call Wizard or JINI |
| INV-PA-6 | Pre-Analysis output is a typed struct — never a generic map |
| INV-PA-7 | Zone activation logic is deterministic — no ML, no randomness |
| INV-PA-8 | Resonant peak detection uses fixed σ threshold — not adaptive |
| INV-PA-9 | MFD intelligence defaults are suggestions — user may override |
| INV-PA-10 | Pre-Analysis runs on raw stereo mix — not on stems |
| INV-PA-11 | All feature extraction algorithms (thresholds, window sizes, filter coefficients) are fixed at compile time — no runtime configuration, no adaptive parameters |
| INV-PA-12 | Resonant peaks capped at 16 entries — no unbounded allocation |

---

## 9. Data Flow Summary

```
Raw stereo PCM
      ↓
PreAnalyzer::run()
      ↓
PreAnalysisData + ZoneActivationFlags
      ↓
    ┌─────────────────────────────────────────┐
    │                                         │
    ▼                                         ▼
Aether Layer                          MFD Intelligence
(zone activation,                     (Tier 3 controls,
 macro defaults,                       defaults, nodes,
 chaos seed)                           suggestions)
```

---

## 10. Amendment Process

Changes to feature definitions, zone activation logic, or the
`PreAnalysisData` struct require:
- Clear justification
- Impact analysis on Aether zone activation
- Version bump
- Lead Architect sign-off

---

## 11. Oracle-TDD Verification Contract

All features in `PreAnalysisData` are verified via the four-step
Oracle-TDD workflow established in sp314-dsp PHILOSOPHY.md:

```
Step 1: PYTHON — define the struct as a Python dataclass.
        Compute all features using NumPy/SciPy.
        Output: generate_pre_analysis_fixture.py

Step 2: JSON — serialize Python output to golden fixture.
        Output: pre_analysis_reference.json

Step 3: RUST TEST — load fixture, deserialize into PreAnalysisData,
        assert all fields match within defined tolerance.
        Output: pre_analysis_contract.rs (RED first)

Step 4: RUST SRC — implement PreAnalyzer::run() in src/.
        libm only. Pure Rust. No exceptions.
        Output: analysis/pre_analysis.rs (GREEN)
```

The Python oracle and the Rust implementation cannot share bugs.
If they agree, the math is correct.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.3 | 2026-05-30 | **Breaking:** 5 bands → 6 bands (split LowMid 250-2k into LowMid 250-500 + Mid 500-2k). Rationale in §3.1. Added `spectral_rolloff_hz` (PA-11). Added §11 Oracle-TDD verification contract. Zone threshold constants made explicit in §5. Resonant peaks capped at 16 (INV-PA-12). Array sizes updated: `spectral_profile_db: [f32; 6]`, `band_phase_correlation: [f32; 6]`. |
| 1.2 | 2026-05-29 | Side/Mid formula clarified (linear RMS then log). Per-stem exclusion note added to Purpose. CI FP determinism enforcement added to INV-PA-1. |
| 1.1 | 2026-05-29 | INV-PA-1 expanded for FP determinism. true_peak/LRA marked MUST IMPLEMENT. Zone activation output contract added (ZoneActivationFlags). INV-PA-11 added (compile-time only). Purpose expanded (no real-time, no dynamic DSP control). |
| 1.0 | 2026-05-29 | Initial lock |

---

**Lead Architect:** Anestis
**Strategist:** Claude
**System:** Creator OS
**Document:** `aether/constitution/pre-analysis-constitution.md`
**Version:** 1.3
**Date:** 2026-05-30
**Status:** 🔒 LOCKED — pending Lead Architect approval

---

*Pre-Analysis is the ears of the system before the brain starts thinking.
Same input. Same output. Always.*
