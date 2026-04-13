# LineOS — Architecture Reference

**Document:** `lineos/architecture/lineos-architecture.md`
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
**Authority:** LineOS Constitution v2.0 · Creator OS Constitution v2.5
**Supersedes:** LineOS Architecture v1.0

---

## 1. System Overview

LineOS is the deterministic execution substrate of Creator OS. It enforces
the laws, runs the computation, and produces the output — deterministically,
offline, and securely.

```
┌──────────────────────────────────────────────────────────────┐
│  Apps (Cockpit) / Aether                                     │
│  communicate via M0 only — never directly to M1              │
└──────────────────────────┬───────────────────────────────────┘
                           │ localhost only
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  M0 — Trust Boundary                                         │
│  Caddy proxy · CDN · marketplace gatekeeper · audit · policy │
│  m0-api.schema.json ← sole public interface                  │
└──────┬───────────────────┬──────────────────────────────────┘
       │                   │
       ▼                   ▼
  M1 services         WASM engines
  (Podman pod)        (served via M0 CDN)
  ├── sp314-dsp       ├── speakforge.wasm    (E1)
  ├── av-core         ├── video-engine.wasm  (E12)
  ├── telemetry       └── av-forge.wasm      (E13)
  ├── metadata
  ├── insights
  ├── rule-engine
  └── M1.6 sync
```

---

## 2. Modules

### 2.1 M0

See authoritative spec: `lineos/m0/constitution/m0-constitution.md`

M0 is the trust boundary. All traffic passes through it. Key subsystems:
- Caddy reverse proxy (`127.0.0.1` only)
- Local CDN — serves WASM artifacts + schemas
- Marketplace gatekeeper — Ed25519 + blake3 verification
- Policy engine — deny-by-default outbound
- Audit log — append-only NDJSON

**API freeze boundary:** `lineos/m0/api/m0-api.schema.json`

### 2.2 sp314-dsp (M1 Audio)

The single source of DSP truth. Immutable between phase releases.

**Execution modes:**

| Mode | Target | Build |
|------|--------|-------|
| WASM | `wasm32-unknown-unknown` | `cargo build --target wasm32-unknown-unknown` + wasm-opt |
| Native | `x86_64-unknown-linux-gnu` | `cargo build --release` |

Both modes: identical binary output for identical inputs and seeds.

**Pipeline stages:**
```
Input → Stage 1: Normalization + Anomaly Detection
      → Stage 2: EQ (Biquad filters)
      → Stage 3: Sidechain De-esser
      → Stage 4: RMS Compressor
      → Stage 5: Saturation (libm::tanhf)
      → Stage 6: Mid/Side Processing
      → Stage 7: Limiting
      → Stage 8: True Peak Ceiling
      → Golden Blob (audio)
```

**Math:** `libm` only. `std::f32::tanh()` and `std::f64` methods forbidden.

### 2.3 av-core (M1 AV)

Deterministic AV orchestration. Orchestrates E12 and E13 via M0 IPC.
Produces the Golden Blob (AV type).

**AV IPC batching:** All dispatch to E12/E13 is batched — multiple frames
per IPC call. Per-frame IPC is forbidden (performance invariant).

### 2.4 M1 Services

All run in the Podman pod. All accessible via M0 only.

| Service | Input | Output | Rule |
|---------|-------|--------|------|
| telemetry | Golden Blob | EBU R128 + video metrics | Never re-measures |
| metadata | Golden Blob | BMR-128 report, EBU report, manifest | Never re-measures |
| insights | QualityMetrics | Pass/fail + recommendation refs | Comparator only |
| rule-engine | InsightReport | Deterministic hints | No LLM, no Aether |
| M1.6 sync | Project manifest + reports + hash | Cloud sync | Opt-in, policy-gated |

### 2.5 WASM Engines

Served via M0 CDN. Accessed via M0 IPC. Never accessed directly.

| Engine | File | Purpose |
|--------|------|---------|
| E1 SpeakForge | `speakforge.wasm` | Voice synthesis (Aether device) |
| E12 VideoEngine | `video-engine.wasm` | Deterministic frame operations |
| E13 AV Forge | `av-forge.wasm` | AV composition + sync |

---

## 3. Golden Blob

The canonical output artifact for both audio and AV processing.
Contract: `creator-os/contracts/golden-blob.schema.json`

```
type GoldenBlob = AudioBlob | AvBlob

AudioBlob:
  flac_bytes        Vec<u8>
  quality_metrics   QualityMetrics (LUFS, TP, LRA, correlation, phase)
  pipeline_params   PipelineParams
  seed              u64
  input_hash        [u8; 32]

AvBlob:
  video_frames      ContentAddressableChunks
  audio_track       Vec<u8>
  timeline_metadata AvTimeline
  quality_metrics   AvQualityMetrics
  seed              u64
  input_hash        [u8; 32]
```

**The Golden Blob is immutable once written.**
All downstream modules read from it. None re-process or re-measure.

---

## 4. Aether Boundary

LineOS receives Aether output only as validated feature vectors.

```
Aether (stochastic)
    │  validate_output() → feature-vector.schema.json
    ▼
[ADAPTER BOUNDARY]
    │  typed feature vectors only
    ▼
LineOS (deterministic)
```

Raw ML output entering LineOS is a constitutional violation.
The rule-engine is **not** Aether — it is deterministic rule evaluation.

---

## 5. Data Flow — Audio Mastering

```
1.  File drop → Cockpit → M0 IPC → sp314-dsp WASM
2.  sp314-dsp → Golden Blob (audio)
3.  Golden Blob → telemetry → EBU R128 measurements
4.  EBU R128 → metadata → BMR-128 report + project manifest
5.  QualityMetrics → insights → compliance (pass/fail)
6.  Compliance → rule-engine → deterministic hints
7.  Hints → Cockpit Coach Island

    ── Aether path (parallel, non-blocking) ──────
8.  QualityMetrics → Aether feature-adapter → feature-vector
9.  Feature vector → Aether coach-adapter → LLM → CoachNarrative
10. CoachNarrative → Cockpit Coach Island
    ──────────────────────────────────────────────

11. Export → FLAC + reports (via M0 policy)
12. (Opt-in) → M0 policy gate → M1.6 → cloud sync
```

---

## 6. Data Flow — AV Mastering

```
1.  AV file → Cockpit → M0 IPC → av-core
2.  av-core → M0 dispatch (parallel):
    ├── Audio: sp314-dsp WASM → DSP pipeline
    └── Video: video-engine (E12) + av-forge (E13)  [batch IPC]
3.  av-core → Golden Blob (AV)
4.  Golden Blob → telemetry (EBU R128 + video metrics)
5.  Metrics → insights → compliance
6.  Compliance → rule-engine → hints
7.  Hints → Cockpit Coach Island
    ── Aether path (parallel, non-blocking) ──────
8.  AV metrics → av-adapter → AV feature-vector → LLM → CoachNarrative
    ──────────────────────────────────────────────
9.  Export → AV container + reports (via M0 policy)
```

---

## 7. Deployment

```
systemd
    └── m0d.service → M0 (host)
            └── (on M0 healthy) → Podman pod
                    ├── telemetry  :internal
                    ├── metadata   :internal
                    ├── insights   :internal
                    ├── rule-engine :internal
                    └── M1.6 sync  :internal

Tauri (user-launched)
    └── Cockpit WebView
            └── sp314-dsp WASM (loaded from M0 CDN)
```

All pod-internal ports are inaccessible from host.
M0 reverse proxy is the only entry point.

---

## 8. Technology Stack

| Component | Technology | Constraint |
|-----------|-----------|-----------|
| DSP engine | Rust `no_std + alloc` | `libm` only — never `std::f32` |
| WASM target | `wasm32-unknown-unknown` + wasm-opt | Output → `m0/assets/wasm/` |
| Desktop shell | Tauri 2.x + Leptos | WASM boundary enforced |
| UI bundler | Trunk | `trunk build` → `m0/assets/cockpit/` |
| Reverse proxy | Caddy | `127.0.0.1` only |
| Container runtime | Podman rootless | No `--network=host` |
| Audio decoding | symphonia (MPL-2.0, accepted) | All formats |
| Resampling | rubato | Deterministic |
| Video frame ops | image-rs, imageproc, fastimageresize | Via E12 |
| Container parsing | mp4parse, matroska-rs | Deterministic |
| Concurrency (AV) | crossbeam, parking_lot | Deterministic |
| Signing | ed25519-dalek + blake3 | Pure Rust — no ring, no C FFI |

---

**Lead Architect:** Anestis
**System:** LineOS
**Version:** 2.0
**Date:** 2026-04-09
**Status:** 🔒 LOCKED
