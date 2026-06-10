# DSP Pipeline Refactor — Spec v1.0 (Skeleton)
# lineos/docs/dsp-pipeline-refactor-spec-v1_0.md

**Document:** `lineos/docs/dsp-pipeline-refactor-spec-v1_0.md`
**Version:** 1.1 — SKELETON (Gemini audit applied)
**Date:** 2026-06-10
**Status:** 📋 BACKLOG — Implement when next feature touches pipeline

---

## 0. Why This Exists

`dsp_pipeline.rs` is 844 lines — one giant function `run_dsp()`.
It handles: decode, NMF scout, Maestro, DSP, corpus, model training, telemetry, certificate.
This is correct behavior but wrong structure.

---

## 1. Current Structure (Monolith)

```
run_dsp() — 844 lines:
  1.  Schema load + LUFS target
  2.  Decode audio (symphonia)
  3.  Silence guard
  4.  Normalization overflow guard
  5.  AudioChunk build
  6.  TwoPassEngine scout (NMF)
  7.  Maestro AutoTuningController
  8.  process_chunks_with_params (stem render + spatial)
  9.  AetherBridge DSP config
  10. Corpus build_timeline
  11. Corpus JSON write
  12. UserMarkovModel update
  13. Global preset snapshot (every 10 sessions)
  14. DspAdapter master (LUFS/limiting)
  15. Certificate generation
  16. BlobStore write
  17. Telemetry
```

---

## 2. Target Structure (Nodes)

```
run_dsp()  ← thin orchestrator, <100 lines
  │
  ├── decode_node::run(req) → DecodedAudio
  │     decode + silence guard + overflow guard
  │
  ├── scout_node::run(audio) → ScoutResult
  │     TwoPassEngine::scout + Maestro compute_render_params
  │
  ├── render_node::run(audio, scout) → RenderedStems
  │     process_chunks_with_params + spatial render
  │     writes to mmap
  │     CONSUMES audio (move semantics) — frees original buffer
  │
  ├── dsp_node::run(stems, intent) → MasteringResult
  │     DspAdapter::master on RENDERED mix (not original audio!)
  │     LUFS correction + true peak limiting
  │
  ├── corpus_node::run(stems, scout) → CorpusEnvelope
  │     build_timeline + write corpus.json
  │     UserMarkovModel update + global snapshot
  │
  └── certificate_node::run(mastered, corpus) → Certificate
        SHA-256 + ExecutionCertificate + BlobStore write
```

---

## 3. Corrected Data Flow (Gemini Audit v1.1)

```rust
pub async fn run_dsp(req: &MasterRequest, start: Instant)
    -> Result<(GoldenBlob, AudioChunk, f32), String>
{
    // Move semantics: each node consumes previous output
    // → only one large buffer alive at a time → INV-ST-3 preserved
    let audio    = decode_node::run(req)?;
    let scout    = scout_node::run(&audio)?;
    // render_node MOVES audio → original buffer freed from RAM
    let stems    = render_node::run(audio, &scout)?;
    // dsp_node runs on RENDERED mix, not original audio
    // Maestro ducking already applied in render_node
    let mastered = dsp_node::run(&stems.mastered_mix, &scout.intent)?;
    // corpus_node uses stems + scout (unified signature)
    let corpus   = corpus_node::run(&stems, &scout)?;
    let blob     = certificate_node::run(&mastered, &corpus)?;
    Ok((blob, stems.chunk, scout.target_lufs))
}
```

**Critical fix (Gemini audit):**
- `dsp_node` receives `stems.mastered_mix` — the NMF-ducked, spatially-rendered output
- NOT the original audio — that would bypass Maestro entirely
- `audio` is moved (not borrowed) into `render_node` → freed after render

---

## 4. Node Signatures (Unified)

```rust
decode_node::run(req: &MasterRequest) -> Result<DecodedAudio, String>
scout_node::run(audio: &DecodedAudio) -> Result<ScoutResult, String>
render_node::run(audio: DecodedAudio, scout: &ScoutResult) -> Result<RenderedStems, String>
dsp_node::run(mix: &[f32], intent: &MasteringIntent) -> Result<MasteringResult, String>
corpus_node::run(stems: &RenderedStems, scout: &ScoutResult) -> Result<CorpusEnvelope, String>
certificate_node::run(mastered: &MasteringResult, corpus: &CorpusEnvelope) -> Result<GoldenBlob, String>
```

---

## 5. New Files

```
lineos/m0/m0-daemon/src/domain/
  dsp_pipeline.rs       ← thin orchestrator (<100 lines)
  nodes/
    mod.rs
    decode_node.rs      ← steps 1-4
    scout_node.rs       ← steps 5-7
    render_node.rs      ← steps 8-9
    dsp_node.rs         ← step 14
    corpus_node.rs      ← steps 10-13
    certificate_node.rs ← steps 15-17
```

---

## 6. Memory Safety (INV-ST-3)

```
Move semantics chain:
  DecodedAudio    → moved into render_node → freed after render
  RenderedStems   → borrowed by dsp_node, corpus_node, certificate_node
  MasteringResult → borrowed by certificate_node

At any point only ONE large audio buffer is alive:
  Before render: DecodedAudio (~50MB for 3min track)
  During render: DecodedAudio being consumed
  After render:  RenderedStems (mmap-backed, ~0MB RAM)

Peak RAM: same or lower than current pipeline ✅
```

---

## 7. Invariants Preserved

```
INV-AB-1: Same input → same output (zero behavioral change)
INV-ST-3: Peak RAM ≤ 50MB (move semantics guarantee)
All existing tests pass without modification
No API changes to handlers or HTTP layer
```

---

## 8. When to Execute

```
Trigger: next feature that requires changes to dsp_pipeline.rs
Process:
  1. Refactor commit (no behavior change, all tests green)
  2. Feature commit (uses new node structure)

DO NOT refactor speculatively.
Only when a feature naturally requires touching the pipeline.
```

---

## 9. Implementation Phases

| Phase | Task | Gate |
|-------|------|------|
| R-P1 | decode_node.rs extracted | cargo test --workspace green |
| R-P2 | corpus_node.rs extracted | cargo test --workspace green |
| R-P3 | scout_node.rs + render_node.rs | cargo test --workspace green |
| R-P4 | dsp_node.rs + certificate_node.rs | cargo test --workspace green |
| R-P5 | run_dsp() thin orchestrator | E2E test green |

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-10 | Initial skeleton |
| 1.1 | 2026-06-10 | Gemini audit: dsp_node bypass fix, unified signatures, move semantics |

---

**Status:** 📋 BACKLOG
**Trigger:** Next feature touching dsp_pipeline.rs
