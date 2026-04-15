# Golden Blob — Specification

**Document:** `creator-os/specs/golden-blob-spec.md`
**Version:** 1.0
**Status:** 🔒 LOCKED
**Date:** 2026-04-03
**Authority:** Creator OS Constitution v2.4
**Schema:** `creator-os/contracts/golden-blob.schema.json`
**Audience:** Compile Agent, Engineers

---

## Purpose

The Golden Blob is the canonical output unit of Creator OS.

It is the single artifact that encodes everything that happened during a processing session — the mastered content, the measurements, the parameters, the provenance. If you have the Golden Blob, you have everything. If you don't have the Golden Blob, you have nothing authoritative.

Every deterministic pipeline in Creator OS produces exactly one Golden Blob.  
Every report, every export, every compliance check derives from the Golden Blob.  
Nothing re-measures. Nothing re-processes. The Blob is the source of truth.

---

## Types

The Golden Blob has two types. The type is declared at creation and never changes.

| Type | Producer | Content |
|------|---------|---------|
| `"audio"` | `lineos/m1/sp314-dsp` (E11) | Mastered audio + loudness metrics |
| `"av"` | `lineos/m1/av-core` | Mastered AV container + audio + video metrics + timeline |

---

## Structure

### Top-level fields

```
GoldenBlob {
    id:               UUID           -- globally unique, generated at creation
    version:          String         -- schema version, e.g. "1.0"
    type:             "audio" | "av" -- immutable after creation
    created_at:       ISO 8601 UTC
    input_hash:       SHA-256        -- hash of the original input file(s)
    seed:             u64            -- determinism seed, derived from input_hash
    pipeline_version: String         -- version of the engine that produced this blob
    content:          AudioContent | AvContent
    loudness:         LoudnessMetrics
    quality:          QualityMetrics
    provenance:       Provenance
}
```

---

### `AudioContent`

Produced by `sp314-dsp` (E11). Contains the mastered audio and its encoding metadata.

```
AudioContent {
    flac_bytes:   Vec<u8>    -- mastered audio, FLAC encoded, lossless
    sample_rate:  u32        -- samples per second (e.g. 44100, 48000, 96000)
    bit_depth:    u8         -- bit depth of the encoded audio (16, 24, 32)
    channels:     u8         -- channel count (1 = mono, 2 = stereo)
    duration_ms:  u64        -- duration in milliseconds
    codec:        "flac"     -- always FLAC for audio Golden Blobs
}
```

---

### `AvContent`

Produced by `av-core`. Contains the mastered AV container and its metadata.

```
AvContent {
    container_bytes: Vec<u8>    -- AV container (MP4/MKV), content-addressed
    video_codec:     String     -- e.g. "av1", "h265"
    audio_codec:     String     -- e.g. "flac", "aac"
    frame_rate:      f32        -- frames per second (e.g. 24.0, 30.0, 60.0)
    resolution:      Resolution { width: u32, height: u32 }
    duration_ms:     u64
    color_space:     String     -- e.g. "bt709", "bt2020"
    hdr:             bool
    audio_track:     AudioContent  -- embedded audio, same structure as audio blob
}
```

---

### `LoudnessMetrics`

The canonical loudness representation. BS.1770-4 values are always present.  
Platform-derived values (EBU R128, ATSC A/85, etc.) are computed from BS.1770-4 — never measured independently.

```
LoudnessMetrics {
    -- BS.1770-4 canonical values (always present, always authoritative)
    integrated_lufs:    f32    -- integrated loudness (gated), LKFS
    short_term_lufs:    f32    -- short-term loudness (3s window)
    momentary_lufs:     f32    -- momentary loudness (400ms window)
    true_peak_dbtp:     f32    -- inter-sample true peak, dBTP
    lra:                f32    -- loudness range, LU

    -- EBU R128 derived (computed from BS.1770-4, not re-measured)
    ebu_r128: EbuR128Metrics {
        target_lufs:      f32    -- e.g. -23.0 for broadcast
        target_lra_max:   f32    -- e.g. 20.0 LU
        target_tp_max:    f32    -- e.g. -1.0 dBTP
        compliant:        bool   -- true if all values meet EBU R128 targets
    }

    -- Platform targets (all derived from BS.1770-4)
    platform_targets: {
        spotify:        PlatformTarget { target: -14.0, compliant: bool }
        youtube:        PlatformTarget { target: -14.0, compliant: bool }
        apple_podcasts: PlatformTarget { target: -16.0, compliant: bool }
        apple_music:    PlatformTarget { target: -16.0, compliant: bool }
        broadcast:      PlatformTarget { target: -23.0, compliant: bool }
        tidal:          PlatformTarget { target: -14.0, compliant: bool }
    }

    -- K-weighting filter (applied as per BS.1770-4)
    k_weighted: bool   -- always true for valid blobs
}
```

**Key rule:** `integrated_lufs`, `true_peak_dbtp`, and `lra` are the only values that are measured. All other loudness values are derived computations. Nothing in metadata or reports re-measures the audio.

---

### `QualityMetrics`

Objective quality measurements derived from the processed content.

```
QualityMetrics {
    -- Stereo / phase
    stereo_correlation: f32    -- -1.0 (out of phase) to +1.0 (mono)
    phase_coherence:    f32    -- 0.0 to 1.0
    stereo_width:       f32    -- 0.0 (mono) to 1.0 (wide)

    -- Dynamic range
    dynamic_range_db:   f32    -- crest factor approximation
    rms_db:             f32    -- RMS level

    -- Spectral
    spectral_centroid:  f32    -- Hz — perceptual brightness
    spectral_flatness:  f32    -- 0.0 (tonal) to 1.0 (noise-like)

    -- Clipping
    clips_detected:     u32    -- count of detected clip events
    clip_free:          bool   -- true if clips_detected == 0
}
```

For AV blobs, `QualityMetrics` additionally contains:

```
    -- Video quality (AV type only)
    video_bitrate_kbps: u32
    video_psnr:         Option<f32>    -- peak signal-to-noise ratio
    video_ssim:         Option<f32>    -- structural similarity index
    av_sync_offset_ms:  f32            -- audio/video sync offset
    av_sync_compliant:  bool           -- true if |av_sync_offset_ms| < 40ms
```

---

### `Provenance`

The complete audit trail. Every Golden Blob knows exactly how it was produced.

```
Provenance {
    engine_id:        String    -- e.g. "E11", "E12", "E13"
    engine_version:   String    -- semver of the producing engine
    pipeline_params:  JSON      -- all parameters that affected the output
    seed:             u64       -- same as top-level seed (redundant for safety)
    input_hash:       SHA-256   -- same as top-level input_hash
    processing_time_ms: u64     -- wall clock time of the pipeline run
    host_os:          String    -- e.g. "linux-x86_64"
    created_by:       String    -- session identifier
    aether_enriched:  bool      -- true if Aether devices contributed to this session
    aether_devices:   Vec<String> -- list of Aether device IDs used (empty if none)
}
```

**Key rule:** `aether_enriched: false` means LineOS processed the audio with no ML involvement. This flag supports auditing and reproducibility verification.

---

## Lifecycle

### Production

```
1. User initiates session → Cockpit
2. Input file → M0 IPC → sp314-dsp (audio) or av-core (AV)
3. Processing completes → Golden Blob created in memory
4. Golden Blob written to local storage (content-addressed by input_hash + seed)
5. M0 writes audit log entry: blob_id, input_hash, engine_id, timestamp
6. Golden Blob → downstream consumers (telemetry, metadata, insights)
7. All downstream work derives from the Blob — nothing re-processes the input
```

### Consumption

The Golden Blob is **read-only after creation**. These are the only permitted consumers:

| Consumer | What it reads | What it produces |
|---------|--------------|-----------------|
| `m1/telemetry` | `loudness.integrated_lufs`, `quality.*` | EBU R128 report |
| `m1/metadata` | All fields | BMR-128 report, project manifest |
| `m1/insights` | `loudness.platform_targets`, `quality.*` | Compliance evaluation |
| `m1/rule-engine` | `loudness.*`, `quality.*` | Pass/fail + recommendations |
| `aether/feature-adapter` | `loudness.*`, `quality.*`, `provenance.*` | Feature vectors for Aether |
| `apps/stillair` | All fields | UI display, export |

**Rule:** No consumer may modify the Golden Blob. It is append-only at the session level — you create a new blob, you never edit an existing one.

---

## Determinism Contract

The Golden Blob is the determinism guarantee.

```
same input_file + same seed + same engine_version → identical Golden Blob
```

This means:
- `flac_bytes` or `container_bytes` are bit-identical
- All `LoudnessMetrics` values are identical
- All `QualityMetrics` values are identical
- `Provenance` is identical except `processing_time_ms` and `created_by`

**Forbidden in the pipeline that produces Golden Blobs:**
- `rand::thread_rng()` — use `seed` instead
- `std::f32::tanh()` — use `libm::tanhf()`
- `Date::now()` — never influence processing
- Any network call — processing is fully offline

---

## Versioning

Golden Blobs are versioned by schema version.

| Schema | Type support | Notes |
|--------|-------------|-------|
| v1.0 | `"audio"` | Tokyo — audio mastering only |
| v1.1 | `"audio"`, `"av"` | Osaka — AV support added |
| v2.0 | `"audio"`, `"av"`, `"multimodal"` | Fukuoka — future |

Breaking changes require a new schema version: `golden-blob-v2.schema.json`.  
Old schema versions remain valid and loadable. No silent migration.

---

## Size Considerations

### Audio blobs
Typical size: 10–150 MB (depends on duration and bit depth).  
FLAC compression ratio: ~50% of uncompressed WAV.

### AV blobs
Typical size: 100 MB – 10 GB (depends on resolution, duration, codec).  
For large AV blobs, the implementation may use **content-addressed chunking** — the `container_bytes` field becomes a manifest of chunk hashes rather than the raw bytes. This does not change the schema contract; it is an implementation detail of the storage layer.

**Rule:** Chunking is a storage optimization. The logical Golden Blob is still a single unit. The schema does not change. Chunking details live in `gallery/creator-pool/`.

---

## Golden Blob vs Reports

This is a common source of confusion:

| Artifact | What it is | Where it lives |
|---------|-----------|---------------|
| **Golden Blob** | The complete, authoritative output | Local storage, content-addressed |
| **BMR-128 Report** | Human-readable compliance summary | Derived from Blob, exportable |
| **EBU R128 Report** | Loudness compliance report | Derived from Blob, exportable |
| **Project Manifest** | Session metadata | Derived from Blob, exportable |

Reports are derived. The Golden Blob is primary. If the Blob and a report disagree, the Blob wins.

---

## Where to Go Next

| I want to... | Go to... |
|-------------|---------|
| See the JSON Schema | `creator-os/contracts/golden-blob.schema.json` |
| Understand the DSP pipeline that produces audio blobs | `lineos/m1/sp314-dsp/src/pipeline.rs` |
| Understand the AV pipeline | `lineos/m1/av-core/src/export.rs` |
| Understand how loudness is measured | `lineos/m1/telemetry/ebu-r128.rs` |
| Understand what the rule-engine does with the blob | `lineos/m1/rule-engine/` |
| Understand how Aether uses the blob | `aether/adapters/feature-adapter/` |

---

*The Golden Blob is the source of truth.*  
*Everything derives from it. Nothing overrides it.*  
*If it is not in the Blob, it was not produced by the pipeline.*
