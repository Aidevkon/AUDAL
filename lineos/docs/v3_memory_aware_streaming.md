# V3.0 Memory-Aware Streaming Architecture — Spec v1.0
# Creator OS — DSP Foundation Layer

**Document:** `lineos/docs/v3_memory_aware_streaming.md`
**Version:** 1.0
**Date:** 2026-06-04
**Authority:** Creator OS Constitution v2.5
**Owner:** Lead Architect (Anestis)
**Status:** 📝 DRAFT — implementation deferred to v3.0
**Depends on:** WavChunkReader/Writer (✅ done, poc-v3.0-rc1)

---

## 0. Why This Exists

The current NMF pipeline loads the entire audio file into RAM.

```
2h podcast @ 48kHz stereo f32:
  Raw audio:           2.7 GB
  STFT frames:         ~4.0 GB
  5× NMF stem masks:   ~8.0 GB
  Intermediate buffers:~6.0 GB
  Total peak:          ~20 GB → OS OOMKill at minute 3
```

The Podcast Engine (first spinoff) works with 1-3h files.
This architecture is the prerequisite for shipping.

---

## 1. The Permutation Problem (Why Simple Chunking Fails)

```
Naive approach: split file into 4s chunks, run NMF on each.

Problem:
  Chunk 1: NMF learns Stem 0 = Voice, Stem 1 = Drums
  Chunk 2: NMF learns Stem 0 = Drums, Stem 1 = Voice

Result: stitched audio is chaos.

Root cause: NMF (like PCA) requires Global Context
  — it must "see" the whole file to learn consistent W matrices.

The W matrix (Spectral Templates) must be FIXED across all chunks.
Only H (Activations) can vary per chunk.
```

---

## 2. The Solution — Two-Pass Streaming Engine

```
Target memory footprint: ~5MB constant (regardless of file length)
Current footprint: ~20GB for 2h file

Two passes solve both constraints:
  Pass 1: Global Context (learns W) — low-res proxy
  Pass 2: Chunked Render (applies W) — constant RAM
```

### Pass 1 — The Scout (Global Analysis)

```
Input:  Full audio file (streamed, never fully loaded)
Output: W matrices, PCA ratios, energy profile

Steps:
  1. WavChunkReader streams file
  2. Downsample on-the-fly: stereo 48kHz → mono 11kHz
     (4x channel reduction × 4.4x sample reduction = ~17.6x smaller)
  3. Run NMF fit() on low-res proxy
     → W matrices (Spectral Templates) — LOCKED after Pass 1
  4. Run PCA Spatial analysis on low-res proxy
     → correlation, pc1_ratio, ms_angle_rad
  5. Compute Level 1 energy ratios
     → per-stem RMS targets

Memory cost: ~5MB
Time cost:   ~1s (downsampled = tiny NMF)
```

### Pass 2 — The Render (Chunked Processing)

```
Input:  Full audio file (via WavChunkReader, 65536 samples/chunk)
Output: Mastered WAV (via WavChunkWriter, written chunk-by-chunk)

Chunk size: 65536 samples = ~1.37s at 48kHz
  Large enough: NMF context from Pass 1 is stable
  Small enough: ~2MB per chunk in RAM

Per chunk:
  1. Read chunk via WavChunkReader
  2. STFT forward (with overlap-add ring buffer for boundaries)
  3. Project onto locked W from Pass 1 → chunk H (activations)
  4. Apply NMF masks + Mask Refinement (spectral gate + FIR)
  5. POX Voice chain (if broadcast flavour)
  6. Spatial Engine (PCA ratios from Pass 1)
  7. Level 1 energy compensation
  8. DspAdapter mastering (EQ/Comp/Limiter)
  9. iSTFT
  10. WavChunkWriter.write_chunk() → disk
  11. Drop chunk from RAM

Memory cost: ~2MB constant
```

---

## 3. The Overlap-Add Problem (Chunk Boundaries)

```
Critical: iSTFT requires overlap between adjacent chunks.
Without this: audible "clicks" every 1.37s.

Solution: Ring Buffer for overlap-add

struct OlaRingBuffer {
    overlap: Vec<f32>,  // FFT_SIZE samples
}

impl OlaRingBuffer {
    fn add_chunk(&mut self, chunk: &[f32]) -> Vec<f32> {
        // Mix overlap from previous chunk with start of new chunk
        // Return clean output, save tail as new overlap
    }
}

Overlap size: FFT_SIZE / 2 = 1024 samples = 21ms
Acceptable: inaudible seam
```

---

## 4. Integration Points

```
Files to modify (v3.0):
  lineos/m0/m0-daemon/src/handlers/master.rs
    → Replace run_dsp_internal() collect() with TwoPassEngine

  lineos/m1/sp314-dsp/src/stft/mod.rs
    → Add streaming forward/inverse with OLA ring buffer

  lineos/m1/sp314-dsp/src/stft/nmf.rs
    → Separate fit() from transform()
      fit() = Pass 1 (learns W)
      transform_chunk() = Pass 2 (applies W to chunk)

Files already done (foundation):
  lineos/m1/sp314-dsp/src/io/stream.rs ✅
    WavChunkReader + WavChunkWriter
    3/3 tests passing
    Roundtrip MSE < 1e-10
```

---

## 5. NMF API Change Required

```
Current API:
  fit_transform(frames) → (W, H)
  Both pass in one call — cannot separate

Required API:
  fit(low_res_frames) → W           ← Pass 1
  transform_chunk(W, chunk_frames) → H_chunk  ← Pass 2 (per chunk)

Why: W must be learned once and reused for every chunk.
     Current fit_transform() couples them — must decouple.

NmfEngine already has:
  pub w: Vec<f32>  ✅ (W matrix stored on struct)
  pub h: Vec<f32>  ✅ (H matrix stored on struct)

So fit() = run fit_transform() on low-res, keep self.w
   transform_chunk() = run transform() using self.w on each chunk
```

---

## 6. Memory Model

```
Phase          | RAM usage | Duration
─────────────────────────────────────
Pass 1 Scout   | ~5MB      | ~1s
Pass 2 per chunk| ~2MB     | ~0.3s/chunk
Total (2h file)| ~7MB peak | ~10s total

vs current:
Full load 2h   | ~20GB peak| ~10s
```

---

## 7. Implementation Phases (v3.0)

| Phase | Task | Gate |
|-------|------|------|
| ST-P1 | Decouple NMF fit() from transform() | existing tests pass |
| ST-P2 | STFT streaming forward/inverse + OLA ring buffer | roundtrip MSE < 1e-5 |
| ST-P3 | TwoPassEngine struct | unit test with 5min audio |
| ST-P4 | Wire TwoPassEngine into master.rs | EBU certified |
| ST-P5 | Memory profiling (valgrind/heaptrack) | peak < 50MB for 2h file |

**Start with ST-P1** — NMF fit/transform decoupling is the
lowest risk and enables all other phases.

---

## 8. What's Already Done (Foundation)

```
✅ WavChunkReader  — streams WAV in chunks, never full load
✅ WavChunkWriter  — writes mastered audio chunk-by-chunk
✅ Roundtrip test  — MSE < 1e-10 (bit-perfect)
✅ Duration test   — correct for any file length
✅ EOF test        — None at end of file

These are the "rails" — Two-Pass wiring plugs into them.
```

---

## 9. Invariants

| ID | Invariant |
|----|-----------|
| INV-ST-1 | W matrix is computed ONCE per mastering run (Pass 1) |
| INV-ST-2 | W matrix is READ-ONLY during Pass 2 |
| INV-ST-3 | Peak RAM ≤ 50MB for any file length |
| INV-ST-4 | OLA ring buffer size = FFT_SIZE / 2 = 1024 samples |
| INV-ST-5 | Chunk size = 65536 samples (fixed, constitutional) |
| INV-ST-6 | EBU R128 output identical to full-load processing |
| INV-AB-1 | Deterministic — same input → same output |

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-04 | Initial spec — context saved before UI chapter |

---

**Lead Architect:** Anestis
**Strategist:** Claude
**System:** Creator OS — Podcast Engine foundation
**Status:** 📝 DRAFT — implement after UI chapter

---

*The W matrix is the memory of the machine.*
*Learned once. Applied forever.*
*This is how a 2-hour podcast becomes 7MB of RAM.*
