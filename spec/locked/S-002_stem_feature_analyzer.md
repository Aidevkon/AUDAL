# S-002 — Stem Feature Analyzer

**Document:** `spec/locked/S-002_stem_feature_analyzer.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔍 REVIEW
**Authority:** Aether Constitution v1.0 · Creator OS Constitution v2.5
**Owner:** Core / LineOS M1
**Depends on:** S-001 (NMF Stem Separator)
**Used by:** S-003 (Persona Schema), S-007 (Semantic Zones), S-008 (Auto-Tuning)
**Audit:** DeepSeek v0.1 → REJECT. v0.2 addresses all findings.

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | R1: linear averaging clarification. R3: stereo_width_range test. Promoted to LOCKED. |
| 0.2 | 2026-05-27 | Critical: stereo interleaved input (Option A). M1–M6 addressed. |
| 0.1 | 2026-05-27 | Initial draft |

---

## 1. Purpose

The Stem Feature Analyzer extracts deterministic, typed audio features
from the 4 stems produced by S-001 (NMF Stem Separator).

Its output — a `StemFeatures` struct — is the primary data source
for all Aether creative decisions. Personas, chaos, semantic zones,
and auto-tuning all depend on this data.

**One sentence:** Given 4 stereo stems, produce a deterministic feature
vector that describes the acoustic characteristics of each stem.

---

## 2. Constitutional Position

```
E14 (S-001)
    ↓ Bass, Vocals, Drums, Other (Vec<f32> stereo interleaved)
StemFeatureAnalyzer (S-002)
    ↓ StemFeatures (typed struct)
    ↓ validate against stem_features.schema.json
[ADAPTER BOUNDARY]
    ↓
Aether (S-003, S-007, S-008)
```

**Rules:**
- No ML weights — pure deterministic math (libm only)
- No std::f32 methods — libm::sqrtf, libm::log10f, libm::fabsf only
- Same stems → same features (bit-identical)
- Output must validate against `contracts/stem_features.schema.json`
- Lives in LineOS M1 (pure algorithmic — ML Origin Rule)

---

## 3. Interface

### Constants

```rust
pub const ANALYSIS_FFT_SIZE:      usize = 2048;   // matches E11 StftEngine
pub const ANALYSIS_HOP_SIZE:      usize = 512;    // 75% overlap
pub const ANALYSIS_SAMPLE_RATE:   u32   = 48_000;
pub const LRA_BLOCK_MS:           u32   = 400;    // ITU-R BS.1770-4
pub const LRA_HOP_MS:             u32   = 100;
pub const DYNAMIC_RANGE_BLOCK_MS: u32   = 100;
pub const ENERGY_RATIO_EPSILON:   f32   = 1e-4;   // tolerance for sum check
```

### Input

```rust
/// Stereo interleaved stems from S-001 (NMF Stem Separator).
/// Format: [L0, R0, L1, R1, ...] at 48kHz.
/// All stems have the same length as the original mix.
pub struct StemInput {
    pub bass:        Vec<f32>,  // stereo interleaved (L,R,L,R,...)
    pub vocals:      Vec<f32>,  // stereo interleaved
    pub drums:       Vec<f32>,  // stereo interleaved
    pub other:       Vec<f32>,  // stereo interleaved
    pub sample_rate: u32,       // always 48000
}
```

### Output

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StemFeatures {
    pub bass:   StemMetrics,
    pub vocals: StemMetrics,
    pub drums:  StemMetrics,
    pub other:  StemMetrics,
    pub mix:    MixMetrics,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StemMetrics {
    // Spectral (averaged across L+R channels)
    pub spectral_centroid_hz:  f32,  // Hz [0.0, 24000.0]
    pub spectral_flatness:     f32,  // [0.0, 1.0] — 0=tonal, 1=noise
    pub spectral_crest_factor: f32,  // dB [0.0, 60.0]

    // Loudness (computed per channel, averaged)
    pub integrated_lufs:       f32,  // LUFS [-144.0, 0.0]
    pub true_peak_dbtp:        f32,  // dBTP [-144.0, 0.0]
    pub loudness_range:        f32,  // LU [0.0, 60.0]
    pub rms_db:                f32,  // dBFS [-144.0, 0.0]

    // Stereo (computed from L+R of the stem)
    pub stereo_correlation:    f32,  // [-1.0, 1.0]
    pub stereo_width:          f32,  // [0.0, 1.0]

    // Dynamics
    pub crest_factor_db:       f32,  // dB [0.0, 60.0]
    pub dynamic_range_db:      f32,  // dB [0.0, 60.0]

    // Energy
    pub energy_ratio:          f32,  // [0.0, 1.0] ratio vs full mix
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MixMetrics {
    pub integrated_lufs:       f32,
    pub true_peak_dbtp:        f32,
    pub loudness_range:        f32,
    pub stereo_correlation:    f32,
    pub stereo_width:          f32,
    pub dynamic_range_db:      f32,
    pub stem_energy_ratios:    [f32; 4],  // [bass, vocals, drums, other]
}
```

---

## 4. Feature Definitions

### Channel Handling Strategy (Option A — per DeepSeek audit)

For all spectral and loudness features:
1. Deinterleave stem into L and R channels
2. Compute feature independently for L and R
3. Return average: `feature = (feature_L + feature_R) * 0.5`

For stereo features (correlation, width):
- Computed directly from L and R of the stem


**Channel averaging rule:** All spectral features are computed independently
for L and R channels, then averaged linearly (not in dB).
Loudness features (LUFS, RMS) are computed on the stereo pair directly.
```rust
fn deinterleave(stereo: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let l: Vec<f32> = stereo.iter().step_by(2).copied().collect();
    let r: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();
    (l, r)
}
```

---

### 4.1 Spectral Centroid (Hz)

Center of mass of the spectrum — indicates brightness.

```
centroid_hz = Σ(f[k] × |X[k]|) / Σ(|X[k]|)

f[k] = k × (sample_rate / FFT_SIZE)
X[k] = complex FFT coefficient at bin k
```

Computed per channel via `StftEngine::forward()` (same instance as S-001,
FFT_SIZE=2048). Averaged across frames and channels.

**Typical values:**
| Stem | Expected Range |
|------|---------------|
| Bass | 60–300 Hz |
| Vocals | 800–3000 Hz |
| Drums | 200–5000 Hz |
| Other | 1000–8000 Hz |

---

### 4.2 Spectral Flatness

Ratio of geometric to arithmetic mean of spectrum.
0.0 = pure tone, 1.0 = white noise.

```
flatness = exp( (1/N) × Σ log(|X[k]| + ε) ) / ( (1/N) × Σ |X[k]| )
ε = 1e-10
```

**libm:** `libm::expf`, `libm::logf`
Computed per channel, averaged.

---

### 4.3 Crest Factor (dB)

Peak-to-RMS ratio — indicates transient density.
High = percussive. Low = compressed/sustained.

```
crest_factor_db = 20 × log10( peak / rms )
peak = max(|x[n]|)
rms  = sqrt( (1/N) × Σ x[n]² )
```

**libm:** `libm::log10f`, `libm::sqrtf`, `libm::fabsf`
Computed per channel, averaged.

---

### 4.4 Integrated LUFS

Reuse `sp314_dsp::metering::measure_integrated_lufs(left, right)`.
Pass L and R of the stem directly.
ITU-R BS.1770-4 compliant (K-weighting + gating).

---

### 4.5 True Peak (dBTP)

Reuse `sp314_dsp::metering::true_peak(left, right)` from E11.

---

### 4.6 Loudness Range (LU)

```
lra = loud_percentile_95 - loud_percentile_10
```

Per-block LUFS (LRA_BLOCK_MS=400ms, LRA_HOP_MS=100ms).
Reuse `lineos_telemetry::lra::LraCalculator`.
Pass interleaved stereo.

---

### 4.7 RMS (dBFS)

```
rms_db = 10 × log10( (1/N) × Σ x[n]² )
```

Computed on full stereo (L+R together).
**libm:** `libm::log10f`

---

### 4.8 Stereo Correlation

```
correlation = Σ(L[n] × R[n]) / sqrt( Σ(L[n]²) × Σ(R[n]²) )
```

Range: [-1.0, 1.0].
1.0 = mono, 0.0 = uncorrelated, -1.0 = inverted.
**libm:** `libm::sqrtf`

---

### 4.9 Stereo Width

```
width = 1.0 - |correlation|
```

Range: [0.0, 1.0]. 0.0 = mono, 1.0 = full stereo.

---

### 4.10 Dynamic Range (dBFS)

```
dynamic_range = loud_percentile_95_db - loud_percentile_5_db
```

Measured on DYNAMIC_RANGE_BLOCK_MS=100ms blocks.

---

### 4.11 Energy Ratio

```
energy_ratio = Σ(stem_L[n]² + stem_R[n]²) / Σ(mix_L[n]² + mix_R[n]²)
```

Constraint: `Σ(energy_ratios) ≤ 1.0 + ENERGY_RATIO_EPSILON`
(ENERGY_RATIO_EPSILON = 1e-4 — tolerance for NMF mask overlap)

---

## 5. Processing Pipeline

```
StemInput (4 × Vec<f32> stereo interleaved)
    ↓
[Deinterleave each stem → (L, R)]
    ↓
[Per-stem STFT analysis]  ← reuse StftEngine (FFT_SIZE=2048, HOP=512)
    │  ├── spectral_centroid_hz  (avg L+R)
    │  ├── spectral_flatness     (avg L+R)
    │  └── spectral_crest_factor (avg L+R)
    ↓
[Per-stem loudness analysis]  ← reuse E11 metering
    │  ├── integrated_lufs  (stereo L+R)
    │  ├── true_peak_dbtp   (stereo L+R)
    │  ├── loudness_range   (stereo L+R)
    │  └── rms_db           (stereo L+R)
    ↓
[Per-stem stereo analysis]
    │  ├── stereo_correlation (L vs R)
    │  └── stereo_width       (derived)
    ↓
[Per-stem dynamics analysis]
    │  ├── crest_factor_db
    │  └── dynamic_range_db
    ↓
[Mix aggregate]
    │  ├── stereo_correlation (full mix L+R)
    │  ├── stereo_width       (full mix)
    │  └── stem_energy_ratios [4]
    ↓
StemFeatures (typed, serializable)
    ↓
validate_against_schema("stem_features.schema.json")
    ↓
[ADAPTER BOUNDARY] → Aether
```

**Note:** Only aggregate features are exposed in the public API.
Per-frame vectors are internal to the analyzer and not part of the output.
Per-frame exposure is deferred to v2.0.

---

## 6. Determinism Guarantees

| Property | Guarantee |
|----------|-----------|
| Same stems → same features | ✅ Pure functions throughout |
| No randomness | ✅ No rand, no thread_rng |
| No ML weights | ✅ Pure algorithmic |
| libm only | ✅ No std::f32 methods |
| Bit-identical across platforms | ✅ libm guarantees |

**Golden test:** Given stems from a fixed test WAV,
`StemFeatures` output must be bit-identical across runs.

---

## 7. Performance Targets

| Metric | Target |
|--------|--------|
| Latency (3-min track) | < 2 seconds |
| Memory overhead | < 50 MB |
| CPU (analysis only) | < 1 core |
| Allocation | Pre-allocated buffers, no per-sample alloc |

**StftEngine reuse:** The same `StftEngine` instance (FFT_SIZE=2048, HOP=512)
is reused from S-001. No separate FFT plan is created.

---

## 8. Contract Tests

```rust
#[test]
fn stem_features_deterministic() {
    let stems = load_test_stems();
    let f1 = StemFeatureAnalyzer::analyze(&stems);
    let f2 = StemFeatureAnalyzer::analyze(&stems);
    assert_eq!(f1, f2);
}

#[test]
fn stem_features_bass_centroid_low() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    assert!(f.bass.spectral_centroid_hz < 500.0,
        "Bass centroid: {}", f.bass.spectral_centroid_hz);
}

#[test]
fn stem_features_drums_high_crest() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    assert!(f.drums.crest_factor_db > f.bass.crest_factor_db,
        "Drums crest: {}, Bass crest: {}",
        f.drums.crest_factor_db, f.bass.crest_factor_db);
}

#[test]
fn stem_features_energy_ratios_bounded() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    let sum: f32 = f.mix.stem_energy_ratios.iter().sum();
    assert!(sum <= 1.0 + ENERGY_RATIO_EPSILON,
        "Energy ratio sum: {}", sum);
}

#[test]
fn stem_features_validates_schema() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    let json = serde_json::to_string(&f).unwrap();
    assert!(validate_against_schema(&json, "stem_features.schema.json"));
}

#[test]
fn stem_features_stereo_correlation_range() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    assert!(f.bass.stereo_correlation >= -1.0);
    assert!(f.bass.stereo_correlation <=  1.0);
}


#[test]
fn stem_features_stereo_width_range() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    // Width must be in [0.0, 1.0] for all stems
    for w in [f.bass.stereo_width, f.vocals.stereo_width,
              f.drums.stereo_width, f.other.stereo_width] {
        assert!(w >= 0.0 && w <= 1.0,
            "Stereo width out of range: {}", w);
    }
}

#[test]
fn stem_features_serializable() {
    let stems = load_test_stems();
    let f = StemFeatureAnalyzer::analyze(&stems);
    let json = serde_json::to_string(&f).unwrap();
    let f2: StemFeatures = serde_json::from_str(&json).unwrap();
    assert_eq!(f, f2);
}
```

---

## 9. Error Handling

| Condition | Behavior |
|-----------|----------|
| Empty stem (all zeros) | Return default metrics, log warning |
| Stem length mismatch | Return `Err(StemLengthMismatch)` |
| Odd-length stereo buffer | Return `Err(InvalidStereoBuffer)` |
| NaN/Inf in output | Clamp to valid range, log error |
| Schema validation fail | Return `Err(SchemaValidationFailed)` |

---

## 10. Implementation Path

```
lineos/m1/sp314-dsp/src/
└── analysis/
    ├── mod.rs       ← pub mod analysis; pub use analyzer::StemFeatureAnalyzer
    ├── features.rs  ← StemFeatures, StemMetrics, MixMetrics, constants
    ├── spectral.rs  ← centroid, flatness, crest (per-channel)
    ├── dynamics.rs  ← crest_factor, dynamic_range
    └── analyzer.rs  ← StemFeatureAnalyzer::analyze()
```

**Reuses:**
- `crate::stft::StftEngine` — STFT (FFT_SIZE=2048, HOP=512)
- `crate::metering::measure_integrated_lufs` — LUFS
- `lineos_telemetry::lra::LraCalculator` — LRA

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-05-27 | R1: linear averaging clarification. R3: stereo_width_range test. Promoted to LOCKED. |
| 0.2 | 2026-05-27 | Critical fix: stereo interleaved input (Option A per audit). M1: channel averaging strategy. M2: ENERGY_RATIO_EPSILON=1e-4. M3: Serialize derived. M4: per-frame internal only. M5: StftEngine reuse noted. M6: const block added. +2 contract tests. |
| 0.1 | 2026-05-27 | Initial draft |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `spec/locked/S-002_stem_feature_analyzer.md`
**Version:** 1.0
**Date:** 2026-05-27
**Status:** 🔍 REVIEW
