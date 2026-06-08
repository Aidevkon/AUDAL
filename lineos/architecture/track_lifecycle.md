# Audio Track Lifecycle — LineOS M1

**Document:** `lineos/architecture/track_lifecycle.md`
**Version:** 1.0
**Date:** 2026-06-08
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.5
**Owner:** Lead Architect (Anestis)
**Status:** 🔒 LOCKED — poc-v3.2

---

## Purpose

Authoritative map of every stage a stereo audio file passes through
inside LineOS M1 — from raw bits to cryptographic certificate.
Reference for all implementation and audit work.

---

## Pipeline Overview

```
Raw stereo file
    ↓  Stage 0  — Ingest & decode
    ↓  Stage 1  — Pre-analysis & autotune
    ↓  Stage 2  — Stem separation (FiveStems NMF)
    ↓  Stage 3  — Mask refinement
    ↓  Stage 4  — The Forge (per-stem + Aether Black Markov)
    ↓  Stage 5  — Spatial engine (offline, stem-aware)
    ↓  Stage 6  — Reconstruction + energy compensation
    ↓  Stage 7  — DspGraph (live block processing)
    ↓  Stage 8  — LUFS correction + true peak enforcement
    ↓  Stage 9  — Telemetry, corpus & export
    ↓  Stage 10 — Cryptographic certification
    ↓
Certified master + PDF/PNG certificate + Golden Blob
```

---

## Stage 0 — Ingest & Decode

**Crate:** `m0-daemon/handlers/decode.rs` (symphonia — M0 only)
**Constitutional rule:** sp314-dsp never receives compressed audio.

Decodes WAV, FLAC, MP3, AIFF → 48 kHz stereo f32 PCM.
Resampling via rubato SincFixedIn (deterministic, fixed params).
Channel normalization: mono → stereo duplicate, N>2 → average pairs.

Guards:
- RMS silence check (< −60 dBFS → rejected)
- Overflow guard: rough LUFS estimate — if normalization gain > 32×
  (30 dB), track too quiet for safe normalization → rejected

Output: `AudioPcm { samples: Vec<f32>, sample_rate: 48000, channels: 2 }`

---

## Stage 1 — Pre-Analysis & Autotune

**Crate:** `sp314-dsp::analysis::PreAnalyzer`
**Constitutional rule:** libm only — no std::f32 math methods.

Measures full-track:
- Integrated LUFS (ITU-R BS.1770-4, K-weighted)
- True Peak (4-phase polyphase FIR, 18 taps/phase)
- Loudness Range (LRA, EBU Tech 3342)
- Transient density (spikes/sec)
- Global phase correlation
- Stereo width
- Dynamic range (dB)
- 6-band spectral profile
- Zone flags: sub_rumble, boxiness, cymbal_harsh, phase_issue, harsh_resonance

Autotune:
- Calculates exact pre-gain to reach target LUFS
- Applies linearly to entire raw PCM — deterministic, no iteration
- `pipeline/autotune.rs`

Also used in Album Cohesion pre-pass:
- `Intent::RunAnalysis` → Executor → decode + PreAnalyzer only
- Returns `integrated_lufs` per track for Anchor Track algorithm

---

## Stage 2 — Stem Separation

**Crate:** `sp314-dsp::stft::stem_renderer::FiveStemRenderer`

Separates stereo mix → 5 discrete stems:

| Stem | Content | Assignment rule |
|------|---------|-----------------|
| Voice | Vocals, speech | High-mid centroid + low transient |
| Drums | Percussive transients | HPSS percussive mask |
| Bass | Low-frequency | Low centroid (< 250 Hz) |
| Harmonics | Mid-range harmonic | Spectral centroid mid |
| Ambience | Reverb tails, room | High flatness score |

Implementation:
- STFT: FFT_SIZE=2048, HOP_SIZE=512, zero-padded Hann window
- HPSS: extracts percussive mask → Drums stem
- Multi-rate NMF: 11.9× speedup via Rayon + downsampled frames
- INV-OT-1: STFT reconstruction MSE < 1e-5

---

## Stage 3 — Mask Refinement

**Crate:** `sp314-dsp::stft`
**Milestone:** poc-v2.9

Two-step deterministic cleanup:
1. Spectral gating: mask < 0.05 → 0.0 (removes cross-stem bleed)
2. FIR smoothing: 3-frame pure moving average (transient attack preserved)

---

## Stage 4 — The Forge (Per-Stem Processing)

**Crates:** `sp314-dsp` (multiple), `aether/markov/`

### POX Voice Chain
- NoiseGateNode: attack/hold 50ms / release FSM
- DeHumNode: 50/100/150 Hz notch cascade
- AutoLevelNode: sliding window RMS, smoothed gain
- DeEsserNode: HP sidechain de-essing
- Active when `flavour_id == "broadcast"`

### Aether Black Markov

Per-stem state classification + predictive control:

| Classifier | States |
|------------|--------|
| Voice | Silence / Breath / Consonant / Vowel / Tail |
| Drums | Quiet / BuildUp / Transient / Decay / Sustain |
| Bass | Silent / Sustained / Walking / Punchy / Rumble |
| Harmonics | Spectral centroid-based |
| Ambience | Tail energy-based |

`PredictiveController`:
- Markov transition matrix (corpus-derived v2)
- SimulationLayer: predicted_peak, crest_risk, isp_risk
- ChaosLayer: LCG deterministic seed (from chaos_seed param), 30% max deviation
- IntegrationFirewall: clamps all deltas

Output: `DspConfig` with per-stem processing parameters.

---

## Stage 5 — Spatial Engine

**Crate:** `sp314-dsp::spatial`
**Runs:** offline in `m0-daemon/handlers/master.rs` with all 5 stems

### SpatialPreAnalysis (v3.1 — corrected)

Direct M/S energy measurement — mathematically exact:

```
mid  = (L+R)/2  →  mid_rms  = RMS(mid)
side = (L-R)/2  →  side_rms = RMS(side)
ms_ratio = side_rms / (mid_rms + side_rms)
```

PCA eigendecomposition retained for:
- `depth_score` = 1 − pc1_ratio (correlated = front, uncorrelated = deep)
- `transient_direction` = sin(ms_angle_rad) — L/R transient asymmetry

Sub energy: IIR LP at 80 Hz (libm::expf, constitutional).

Previous implementation used `pca.cov_ll` / `pca.cov_rr` for
mid/side energy — incorrect (L/R variances, not M/S RMS).
Fixed in v3.1. `pca_spatial_oracle.json` archived.

### StemChannelAssignments

| Stem | Primary channels | Adaptive rule |
|------|-----------------|---------------|
| Voice | Center (C) | center_weight adaptive to ms_ratio |
| Drums | Front L+R | fixed 0.9 front |
| Bass | Front L+R + LFE | LFE if centroid < 80 Hz |
| Harmonics | Front L+R | fixed 0.8 front |
| Ambience | Rear Ls+Rs | rear_weight adaptive to ms_ratio + depth_score |

INV-SP-8: Voice center_weight always > 0.

### Internal 5.1 Stage
Always active regardless of output renderer (INV-SP-5).
SpatialFirewall: max_rear=0.4 (INV-SP-7), max_lfe=−6 dB (INV-SP-6).

### StereoRenderer
ITU-R BS.775 downmix → mono-compatible (INV-SP-2).

### UserSpatialProfile + Markov Prediction (SP-P7)
- Tail state → rear_decay +0.2
- Consonant state → width_tendency +0.15

---

## Stage 6 — Reconstruction + Energy Compensation

**Milestone:** poc-v2.8

```
Mix = Voice(POX) + Bass + Harmonics + Ambience + Drums
gain = (original_rms / mix_rms).clamp(0.5, 2.0)
```

INV-AB-1: deterministic — same input → same gain always.

---

## Stage 7 — DspGraph (Live Block Processing)

**Crate:** `sp314-nodes::graph::DspGraph`
**Block size:** 512 samples
**Topology:** built by `Pipelineforge` from `MasteringIntent`

| Node | Purpose | Version |
|------|---------|---------|
| Compressor | Broadband RMS | v3.0 |
| MultibandCompressor | 3-band LR4 + independent GR | v3.1 |
| Limiter | BrickwallLimiter, 5ms lookahead | v3.0 |
| MsMatrix / InverseMsMatrix | M/S encode/decode | v3.0 |
| Width | Stereo width | v3.0 |
| NoiseGate | Gate with FSM | v3.0 |
| DeEsser | HP sidechain | v3.0 |
| DeHum | 50/100/150 Hz notch | v3.0 |
| AutoLevel | RMS-based leveller | v3.0 |
| Reverb | Algorithmic reverb | v3.0 |
| Harmonic | ADAA 2× polyphase waveshaper | v3.1 |

### MultibandCompressor (v3.1)
`CrossoverLR4x3`: cascade of two `CrossoverLR4` instances.
f_low ≈ 200 Hz, f_high ≈ 3000 Hz → Low / Mid / High bands.
Independent envelope + GR per band.
Flat sum ±0.2 dB — Oracle-TDD certified (crossover3_reference.json).

### Harmonic Node (v3.1)
`HarmonicEngine`: 2× polyphase oversampled waveshaper.
Internal M/S: L/R → M/S → waveshape → M/S → L/R.
`y_odd = tanhf(drive × x)` — odd harmonics (warmth).
`y_even = x + x·|x|·0.5` — even harmonics (analog character).
K_harmonic ≈ 1.995 — Oracle-TDD certified (k_harmonic_results.json).

---

## Stage 8 — LUFS Correction + True Peak Enforcement

**Fixed:** v3.1 (INV-AB-1 violation corrected)

### LUFS Correction
```
correction_db  = target_lufs − output_lufs  (clamped ±18 dB)
correction_lin = libm::powf(10.0, correction_db / 20.0)
audio         *= correction_lin
```

### ISP Enforcement (v3.1)
Second `BrickwallLimiter` pass after correction multiply.
Ceiling from `intent.target.max_true_peak_db` (default −1.0 dBTP).
Release 15 ms — transparent, no pumping.

### True Peak Measurement (v3.1 — corrected)
`TruePeakDetector`: 4-phase polyphase FIR, 18-sample flush.
Reports ITU-R BS.1770-4 true peak — not digital sample maximum.

---

## Stage 9 — Telemetry, Corpus & Export

EBU R128 windowed analysis (lineos-telemetry):
- LRA via `LraCalculator` (EBU Tech 3342)
- Momentary LUFS (400ms), Short-term LUFS (3s)

Corpus generator (lineos-corpus):
- session_*.corpus.json (900-JSON contract)
- Per-stem SHA-256 fingerprints

**Golden Blob — current state: Phase 6 — in-memory only.**
Lost on daemon restart. Phase 7 will add file persistence.

---

## Stage 10 — Cryptographic Certification

- BLAKE3: raw PCM hashed per stage
- Per-stem SHA-256 fingerprints (StemFingerprints)
- Ed25519 JWS: deterministic offline key, signs blob_id + pcm_blake3 + lufs + fingerprints
- Certificate: PDF (printpdf) + PNG (imageproc + ab_glyph) + QR codes

---

## Constitutional Agent Routing (v3.1)

```
HTTP POST /master
    ↓ Intent::ExecuteMastering
Operator (logged)
    ↓
Conductor (R2) — builds ExecutionPlan
    ↓ Intent::RunDsp
Executor (R3) — spawn_blocking → DspAdapter::master()
    ↓
DspOutput { blob_id, lufs, true_peak }
```

**Album/Batch (POST /master/batch):**

```
Conductor — Cohesion Pre-Pass:
    Intent::RunAnalysis per track → PreAnalyzer only
    Anchor = loudest track → global_target_lufs
    Per-track offset = global_target - (anchor - track_lufs)
    ↓
Sequential loop: one track at a time (peak RAM = ~800 MB)
QUEUED → CERTIFIED per track
```

---

## Invariants

| ID | Invariant |
|----|-----------|
| INV-AB-1 | Same input → same output always |
| INV-AB-3 | No randomness in production pipeline |
| INV-OT-1 | STFT reconstruction MSE < 1e-5 |
| INV-OT-3 | All transforms deterministic |
| INV-SP-1 | Spatial Engine deterministic |
| INV-SP-2 | StereoRenderer mono-compatible |
| INV-SP-5 | Internal 5.1 stage always active |
| INV-SP-6 | LFE never exceeds −6.0 dBFS |
| INV-SP-7 | Rear energy ≤ 40% of front |
| INV-SP-8 | Voice always routed through Center |

---

## Open Items (Phase 7+)

| Item | Status | Notes |
|------|--------|-------|
| Golden Blob file persistence | ⏸ Phase 7 | In-memory only — lost on restart |
| A/B/C Tree lineage | ⏸ Phase 7 | Requires file storage first |
| presence_energy measurement | ⏸ Future | Hardcoded 0.0 — 1–4 kHz band |
| air_energy measurement | ⏸ Future | Hardcoded 0.0 — >8 kHz band |
| WizardAgent NMF findings | ⏸ Future | Stem masking analysis |
| Spatial Telemetry UI (MFD 2) | ⏸ Future | cockpit-dioxus component |

---

## Git Tag History

```
poc-v1.0   First mastering run
poc-v1.1   FiveStems + Voice extraction
poc-v1.2   POX Voice chain
poc-v1.3   Aether Black Markov
poc-v1.4   Cut+Heal + Simulation Layer
poc-v1.5   Full stem profiles
poc-v2.0   Corpus Pipeline (900-JSON)
poc-v2.1   Spatial Engine (SP-P1→P7)
poc-v2.2   Train Me engine
poc-v2.3   Real voice DNA (Level 3 windowed)
poc-v2.4   Spatial Engine in production
poc-v2.5   Orthogonal Transforms (STFT+Hadamard+PCA)
poc-v2.6   Adaptive PCA Spatial
poc-v2.7   NMF 11.9× speedup
poc-v2.8   Level 1 energy-preserving reconstruction
poc-v2.9   Deterministic Mask Refinement
poc-v3.0   Certificate System + 4/5 stems
poc-v3.1   True Peak · 3-band LR4 · ADAA Harmonic
           Constitutional Agent Architecture (R1/R2/R3)
           M/S Dynamic Spatial (corrected RMS)
poc-v3.2   Album/Batch Mastering · Loudness Cohesion
           album_flavour — engineer's sonic identity
```

---

**Lead Architect:** Anestis
**Strategist:** Claude
**System:** Creator OS — LineOS M1
**Version:** 1.0
**Date:** 2026-06-08
**Status:** 🔒 LOCKED — poc-v3.2

*Raw bits in. Certified master out.*
*Every step proven. Every hash signed.*
*Privacy by architecture. Zero cloud.*
