# LineOS — Phase 3 Task Decomposition

**Document:** `lineos/plan/phase-3/task-decomposition.md`
**Version:** 1.0
**Phase:** 3 — Telemetry + Metadata + Insights
**Status:** 🔒 LOCKED
**Authority:** Phase 3 Master Prompt · LineOS Constitution v2.0

---

## Context: What Phase 2 already gave us

The `sp314-dsp` Golden Blob already contains:
- `bs1770_integrated` — ITU-R BS.1770-4 integrated gated loudness (LUFS)
- `bs1770_true_peak` — BS.1770-4 true peak (dBTP)
- `stereo_correlation` — windowed 300ms average
- `dynamic_range_db` — peak-to-RMS

**Phase 3 adds:**
- `loudness_range_lu` (LRA) — requires short-term window analysis
- `momentary_lufs` — 400ms ungated window (last value)
- `short_term_lufs` — 3s sliding window (last value)
- Structured JSON reports
- Pass/fail per platform preset

---

## Task Order

```
P3-001  EBU R128 schema + LRA types
P3-002  telemetry crate (LRA + momentary + short-term)
P3-003  metadata crate (report generation)
P3-004  insights crate (compliance comparator)
P3-005  Add to workspace + CI gates
P3-006  Integration gate + tag
```

---

## P3-001 — EBU R128 Schema + LRA Types

**Goal:** Define the full EBU R128 measurement schema.

Create `lineos/shared/schema/ebu-r128.schema.json`:
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "EBU R128 Full Measurement",
  "version": "1.0",
  "properties": {
    "integrated_lufs":   { "type": "number", "description": "BS.1770-4 integrated gated (LUFS)" },
    "true_peak_dbtp":    { "type": "number", "description": "BS.1770-4 true peak (dBTP)" },
    "loudness_range_lu": { "type": "number", "description": "EBU R128 LRA (LU)" },
    "momentary_lufs":    { "type": "number", "description": "400ms ungated window, last value" },
    "short_term_lufs":   { "type": "number", "description": "3s sliding window, last value" },
    "stereo_correlation":{ "type": "number", "minimum": -1.0, "maximum": 1.0 },
    "dynamic_range_db":  { "type": "number" },
    "sample_rate":       { "type": "integer" },
    "channels":          { "type": "integer" },
    "duration_seconds":  { "type": "number" }
  },
  "required": [
    "integrated_lufs", "true_peak_dbtp", "loudness_range_lu",
    "momentary_lufs", "short_term_lufs"
  ]
}
```

Add `Ebu128Measurement` struct to sp314-dsp types:
```rust
// lineos/m1/sp314-dsp/src/types/metrics.rs — extend existing
#[derive(Debug, Clone)]
pub struct Ebu128Measurement {
    pub integrated_lufs: f32,    // from Golden Blob bs1770_integrated
    pub true_peak_dbtp: f32,     // from Golden Blob bs1770_true_peak
    pub loudness_range_lu: f32,  // computed by telemetry (LRA)
    pub momentary_lufs: f32,     // last 400ms window
    pub short_term_lufs: f32,    // last 3s window
    pub stereo_correlation: f32,
    pub dynamic_range_db: f32,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_seconds: f32,
}
```

**DoD P3-001:**
```bash
python3 -c "import json; json.load(open('lineos/shared/schema/ebu-r128.schema.json')); print('✅ ebu-r128.schema.json valid')"
just validate-schemas
```

---

## P3-002 — Telemetry Crate (LRA + Momentary + Short-term)

**Goal:** Compute the three measurements missing from Phase 2.
Reads Golden Blob samples — never touches raw input audio.

Create `lineos/m1/telemetry/Cargo.toml`:
```toml
[package]
name = "lineos-telemetry"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
sp314-dsp = { path = "../../m1/sp314-dsp" }
libm      = "0.2"
serde     = { version = "1", features = ["derive"] }
serde_json = "1"
```

Create `lineos/m1/telemetry/src/lib.rs`:
```rust
//! LineOS Telemetry — EBU R128 full measurement
//! Reads from Golden Blob only — never re-measures raw audio.
//! Adds LRA, momentary, short-term to existing bs1770 values.

pub mod lra;
pub mod windows;
pub mod report;
```

### LRA Algorithm (EBU R128 §3.4)

```rust
// lineos/m1/telemetry/src/lra.rs
//
// LRA = difference between 95th and 10th percentile of short-term loudness
// Short-term window = 3s, hop = 1s
// Gate: -70 LUFS absolute + relative gate -20 LU below ungated mean

pub struct LraCalculator {
    short_term_values: Vec<f32>,   // all 3s window values above gate
    sample_rate: u32,
}

impl LraCalculator {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            short_term_values: Vec::new(),
            sample_rate,
        }
    }

    /// Feed processed samples from Golden Blob (already K-weighted by sp314-dsp)
    /// Window: 3s = sample_rate * 3 samples per channel
    pub fn feed_samples(&mut self, samples: &[f32], channels: u16) {
        let window_size = self.sample_rate as usize * 3 * channels as usize;
        let hop_size = self.sample_rate as usize * 1 * channels as usize;

        let mut pos = 0;
        while pos + window_size <= samples.len() {
            let window = &samples[pos..pos + window_size];
            let ms = mean_square(window);
            let lufs = ms_to_lufs(ms);
            if lufs > -70.0 {  // absolute gate
                self.short_term_values.push(lufs);
            }
            pos += hop_size;
        }
    }

    /// Compute LRA after all samples fed.
    /// Returns LU value (difference between 95th and 10th percentile).
    pub fn compute(&self) -> f32 {
        if self.short_term_values.len() < 2 {
            return 0.0;
        }
        let mut sorted = self.short_term_values.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // Relative gate: -20 LU below ungated mean
        let ungated_mean = sorted.iter().map(|&v| v as f64).sum::<f64>()
            / sorted.len() as f64;
        let gate = ungated_mean as f32 - 20.0;

        let gated: Vec<f32> = sorted.iter()
            .copied()
            .filter(|&v| v > gate)
            .collect();

        if gated.len() < 2 {
            return 0.0;
        }

        let p10 = percentile(&gated, 0.10);
        let p95 = percentile(&gated, 0.95);
        (p95 - p10).max(0.0)
    }
}

fn mean_square(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    sum / samples.len() as f32
}

fn ms_to_lufs(ms: f32) -> f32 {
    // LUFS = -0.691 + 10 * log10(ms)
    -0.691 + 10.0 * libm::log10f(ms.max(1e-10))
}

fn percentile(sorted: &[f32], p: f32) -> f32 {
    let idx = (p * (sorted.len() - 1) as f32) as usize;
    sorted[idx.min(sorted.len() - 1)]
}
```

### Momentary + Short-term Windows

```rust
// lineos/m1/telemetry/src/windows.rs

/// Momentary loudness: last 400ms window (EBU R128 §2.2)
pub fn momentary_lufs(samples: &[f32], sample_rate: u32, channels: u16) -> f32 {
    let window = sample_rate as usize / 10 * 4 * channels as usize; // 400ms
    if samples.len() < window {
        return -f32::INFINITY;
    }
    let last = &samples[samples.len() - window..];
    ms_to_lufs(mean_square(last))
}

/// Short-term loudness: last 3s window (EBU R128 §2.3)
pub fn short_term_lufs(samples: &[f32], sample_rate: u32, channels: u16) -> f32 {
    let window = sample_rate as usize * 3 * channels as usize;
    if samples.len() < window {
        return -f32::INFINITY;
    }
    let last = &samples[samples.len() - window..];
    ms_to_lufs(mean_square(last))
}

fn mean_square(samples: &[f32]) -> f32 {
    let sum: f32 = samples.iter().map(|&s| s * s).sum();
    sum / samples.len() as f32
}

fn ms_to_lufs(ms: f32) -> f32 {
    -0.691 + 10.0 * libm::log10f(ms.max(1e-10))
}
```

### Report

```rust
// lineos/m1/telemetry/src/report.rs
use sp314_dsp::types::{golden_blob::GoldenBlob, metrics::Ebu128Measurement};
use crate::{lra::LraCalculator, windows};

pub fn measure(blob: &GoldenBlob) -> Ebu128Measurement {
    // Extract processed samples from Golden Blob
    // Phase 3: flac_bytes contains raw f32 PCM (Phase 4 adds proper FLAC decode)
    let samples = pcm_from_blob(blob);
    let sr = blob.quality_metrics.sample_rate;
    let ch = blob.quality_metrics.channels;

    let mut lra_calc = LraCalculator::new(sr);
    lra_calc.feed_samples(&samples, ch);

    Ebu128Measurement {
        // BS.1770-4 canonical values — already computed by sp314-dsp
        integrated_lufs:    blob.quality_metrics.bs1770_integrated,
        true_peak_dbtp:     blob.quality_metrics.bs1770_true_peak,
        stereo_correlation: blob.quality_metrics.stereo_correlation,
        dynamic_range_db:   blob.quality_metrics.dynamic_range_db,
        // New values computed here
        loudness_range_lu:  lra_calc.compute(),
        momentary_lufs:     windows::momentary_lufs(&samples, sr, ch),
        short_term_lufs:    windows::short_term_lufs(&samples, sr, ch),
        sample_rate: sr,
        channels: ch,
        duration_seconds: samples.len() as f32 / (sr as f32 * ch as f32),
    }
}

fn pcm_from_blob(blob: &GoldenBlob) -> Vec<f32> {
    // Phase 3 stub: interpret flac_bytes as raw f32 LE
    // Phase 4 replaces with proper FLAC decode via symphonia
    blob.flac_bytes.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}
```

**DoD P3-002:**
```bash
cargo check -p lineos-telemetry
cargo test -p lineos-telemetry
```

---

## P3-003 — Metadata Crate (Report Generation)

**Goal:** Generate structured JSON reports from `Ebu128Measurement`.
Reads from measurement — never from raw audio.

Create `lineos/m1/metadata/Cargo.toml`:
```toml
[package]
name = "lineos-metadata"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
sp314-dsp        = { path = "../../m1/sp314-dsp" }
lineos-telemetry = { path = "../../m1/telemetry" }
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
chrono           = { version = "0.4", features = ["serde"] }
```

Create `lineos/m1/metadata/src/lib.rs`:
```rust
pub mod bmr128;
pub mod ebu_report;
pub mod project_manifest;
```

### BMR-128 Report

```rust
// lineos/m1/metadata/src/bmr128.rs
use serde::{Deserialize, Serialize};
use sp314_dsp::types::metrics::Ebu128Measurement;

#[derive(Debug, Serialize, Deserialize)]
pub struct Bmr128Report {
    pub version: &'static str,
    pub preset: String,
    pub target_lufs: Option<f32>,
    pub true_peak_ceiling: f32,
    pub measured: MeasuredValues,
    pub compliance: ComplianceResult,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MeasuredValues {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub loudness_range_lu: f32,
    pub stereo_correlation: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComplianceResult {
    pub passes: bool,
    pub lufs_delta: Option<f32>,   // actual - target (None for raw preset)
    pub peak_headroom: f32,         // ceiling - actual (positive = ok)
    pub violations: Vec<String>,
}

impl Bmr128Report {
    pub fn generate(
        measurement: &Ebu128Measurement,
        preset: &str,
        target_lufs: Option<f32>,
        true_peak_ceiling: f32,
    ) -> Self {
        let mut violations = Vec::new();

        let lufs_delta = target_lufs.map(|target| {
            measurement.integrated_lufs - target
        });

        if let Some(delta) = lufs_delta {
            if delta.abs() > 0.5 {
                violations.push(alloc::format!(
                    "LUFS out of tolerance: {:.1} (target {:.1}, delta {:.2})",
                    measurement.integrated_lufs,
                    target_lufs.unwrap(),
                    delta
                ));
            }
        }

        let peak_headroom = true_peak_ceiling - measurement.true_peak_dbtp;
        if peak_headroom < 0.0 {
            violations.push(alloc::format!(
                "True peak exceeds ceiling: {:.2} dBTP (ceiling {:.1})",
                measurement.true_peak_dbtp, true_peak_ceiling
            ));
        }

        Bmr128Report {
            version: "1.0",
            preset: preset.to_string(),
            target_lufs,
            true_peak_ceiling,
            measured: MeasuredValues {
                integrated_lufs:    measurement.integrated_lufs,
                true_peak_dbtp:     measurement.true_peak_dbtp,
                loudness_range_lu:  measurement.loudness_range_lu,
                stereo_correlation: measurement.stereo_correlation,
            },
            compliance: ComplianceResult {
                passes: violations.is_empty(),
                lufs_delta,
                peak_headroom,
                violations,
            },
        }
    }
}
```

**DoD P3-003:**
```bash
cargo check -p lineos-metadata
cargo test -p lineos-metadata
```

---

## P3-004 — Insights Crate (Compliance Comparator)

**Goal:** Evaluate pass/fail across all presets. Pure comparator — no DSP.

Create `lineos/m1/insights/Cargo.toml`:
```toml
[package]
name = "lineos-insights"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
sp314-dsp        = { path = "../../m1/sp314-dsp" }
lineos-telemetry = { path = "../../m1/telemetry" }
lineos-metadata  = { path = "../../m1/metadata" }
serde            = { version = "1", features = ["derive"] }
serde_json       = "1"
```

Create `lineos/m1/insights/src/lib.rs`:
```rust
//! Insights — compliance comparator.
//! Reads QualityMetrics + Ebu128Measurement.
//! Never re-measures. Never touches raw audio.
//! Evaluates against all presets from bmr-128.schema.json.

pub mod evaluator;
pub mod report;
```

```rust
// lineos/m1/insights/src/evaluator.rs
use sp314_dsp::types::{config::Bmr128Schema, metrics::Ebu128Measurement};
use lineos_metadata::bmr128::Bmr128Report;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct InsightsReport {
    pub preset_results: Vec<PresetResult>,
    pub recommended_preset: Option<String>,
    pub rule_engine_hints: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PresetResult {
    pub preset: String,
    pub passes: bool,
    pub report: Bmr128Report,
}

pub fn evaluate_all(
    measurement: &Ebu128Measurement,
    schema: &Bmr128Schema,
) -> InsightsReport {
    let mut results = Vec::new();

    for (preset_name, thresholds) in &schema.presets {
        let report = Bmr128Report::generate(
            measurement,
            preset_name,
            thresholds.target_lufs,
            thresholds.true_peak_ceiling_dbfs,
        );
        let passes = report.compliance.passes;
        results.push(PresetResult {
            preset: preset_name.clone(),
            passes,
            report,
        });
    }

    // Recommend the strictest passing preset
    let recommended = results.iter()
        .filter(|r| r.passes)
        .min_by(|a, b| {
            let a_lufs = a.report.target_lufs.unwrap_or(-14.0);
            let b_lufs = b.report.target_lufs.unwrap_or(-14.0);
            a_lufs.partial_cmp(&b_lufs).unwrap()
        })
        .map(|r| r.preset.clone());

    let hints = generate_hints(measurement, &results);

    InsightsReport {
        preset_results: results,
        recommended_preset: recommended,
        rule_engine_hints: hints,
    }
}

fn generate_hints(
    m: &Ebu128Measurement,
    results: &[PresetResult],
) -> Vec<String> {
    let mut hints = Vec::new();
    let passing = results.iter().filter(|r| r.passes).count();

    if passing == 0 {
        hints.push("No platform presets pass. Consider re-mastering.".into());
    }
    if m.true_peak_dbtp > -1.0 {
        hints.push(alloc::format!(
            "True peak {:.2} dBTP exceeds -1.0 ceiling. Apply limiting.",
            m.true_peak_dbtp
        ));
    }
    if m.stereo_correlation < 0.5 {
        hints.push("Low stereo correlation. Check for phase issues.".into());
    }
    if m.loudness_range_lu > 14.0 {
        hints.push(alloc::format!(
            "High LRA ({:.1} LU). Dynamic content may be reduced on streaming.",
            m.loudness_range_lu
        ));
    }
    hints
}
```

**DoD P3-004:**
```bash
cargo check -p lineos-insights
cargo test -p lineos-insights
```

---

## P3-005 — Add to Workspace + CI Gates

Add to root `Cargo.toml`:
```toml
members = [
    "lineos/m0/m0-daemon",
    "lineos/m1/sp314-dsp",
    "lineos/m1/telemetry",
    "lineos/m1/metadata",
    "lineos/m1/insights",
]
```

**DoD P3-005:**
```bash
cargo check --workspace
just ci
just deny
```

---

## P3-006 — Integration Gate + Tag

```bash
cargo test --workspace
just ci
just deny
just validate-schemas

git add -A
git commit -m "feat(telemetry): Phase 3 — telemetry + metadata + insights

- lineos-telemetry: EBU R128 LRA, momentary, short-term
- lineos-metadata: BMR-128 report generation
- lineos-insights: compliance comparator, all presets
- Reads Golden Blob only — never re-measures raw audio
- All thresholds from bmr-128.schema.json
- ebu-r128.schema.json added to shared/schema

Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6"

git tag v0.3.0-telemetry
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 3 — Telemetry + Metadata + Insights — COMPLETE

lineos-telemetry:   LRA + momentary + short-term ✅
lineos-metadata:    BMR-128 report generation ✅
lineos-insights:    compliance comparator, all presets ✅
Golden Blob only:   never re-measures raw audio ✅
Thresholds:         from bmr-128.schema.json ✅
ebu-r128.schema:    ✅
just ci:            ✅

Tag: v0.3.0-telemetry ✅

Ready for: Phase 4 — rule-engine + Cockpit
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 3
**Version:** 1.0
**Status:** 🔒 LOCKED
