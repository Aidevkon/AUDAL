# Creator OS — Amendment A-003
## Audio Spine Separation

**Document:** `creator-os/constitution/amendments/A-003-audio-spine.md`
**Version:** 1.0
**Date:** 2026-04-16
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 §13
**Inherits:** `creator-os-invariants.md` v1.1
**Related:** Amendment A-002 (UI Disposable)

---

## Preamble

This amendment defines the architecture of the audio data path
in Creator OS. It separates PCM ownership from processing,
establishes the internal audio kernel (xaak.rs), and defines
the activation sequence for each layer.

If this amendment conflicts with the Creator OS Constitution v2.6,
the Constitution wins. If it conflicts with `creator-os-invariants.md`,
the invariants win.

---

## §1 — Dual-Layer Audio Spine

Creator OS adopts two distinct audio spine layers:

### 1.1 — Internal Audio Kernel (`xaak.rs`)

- Belongs to LineOS/M1
- Is the sole owner of PCM after mastering completes
- Activates in Phase 12 only
- Provides a unified, lock-free ring buffer for PCM
- Consumers: DSP → Telemetry → Export → Playback → Coach

### 1.2 — External Audio Spine (`Xaak-rs`)

- MIT-licensed external Rust crate
- Activates in Phase 14 only
- Provides multi-room, cloud rendering, sendspin integration
- Does not replace xaak.rs — extends it for distributed pipelines
- Sits above xaak.rs — never below it

```
sp314-dsp (M1) + fundsp graph layer
        │
        │ PCM (Vec<f32>) — Phase 9–11 only
        ▼
xaak.rs (internal PCM kernel) — Phase 12+
        │
        ├── Telemetry
        ├── Export Layer (WAV / FLAC / OPUS / MP3)
        ├── Playback Engine (via cpal)
        └── Coach Engine
        │
        ▼
cpal I/O (Phase 12+)
        │
        ▼
Xaak-rs + sendspin (Phase 14+)
cloud render · multi-room · telemetry fan-out
```

**Rule:** No subsystem may conflate the two spine layers.
xaak.rs is internal. Xaak-rs is external. They are not interchangeable.

---

## §2 — PCM Ownership

PCM has one and only one owner:

> **xaak.rs** (internal kernel, Phase 12+)

Before Phase 12, PCM travels as `Vec<f32>` inside M0 handlers.
This is a temporary implementation detail, not a contract.

**Forbidden in all phases:**

```
❌ PCM in SessionStateJson
❌ PCM in the Cockpit (Dioxus or Leptos)
❌ PCM in Dioxus/Leptos components
❌ PCM in Coach structs
❌ PCM in Telemetry structs
❌ PCM in Export request/response structs
❌ PCM crossing Tauri IPC boundary
❌ PCM in GoldenBlobJson (IPC contract)
```

All subsystems access PCM only through xaak.rs after Phase 12.
Before Phase 12: M0 handlers hold PCM internally, never serialize it.

**Audit trail rule:** xaak.rs lifecycle events must be written
to the M0 audit log:

```
m0d.xaak_buffer_allocated { size_bytes, sample_rate, channels, blob_id }
m0d.xaak_buffer_released  { blob_id, duration_ms }
```

---

## §3 — Phase Activation Rules

Phase bleed is a constitutional violation.
Each layer activates at its designated phase and not before.

### Phase 9–11 (current)

```
✅ PCM as Vec<f32> in M0 handlers (internal only)
✅ Cockpit is static (no live audio)
✅ Export is synchronous (read audio_bytes from StoredBlob)
❌ xaak.rs NOT active
❌ Xaak-rs NOT active
❌ cpal NOT active
```

### Phase 12

```
✅ xaak.rs activated
✅ PCM transferred to unified kernel ring buffer
✅ Playback becomes real-time (via cpal)
✅ Telemetry / Export / Coach read from xaak.rs
✅ cpal activated as I/O backend under xaak.rs
❌ Xaak-rs NOT active
❌ sendspin NOT active
```

### Phase 14

```
✅ Xaak-rs activated as external spine above xaak.rs
✅ Cloud rendering permitted
✅ Multi-room audio permitted
✅ Distributed DSP permitted
✅ sendspin integration permitted
✅ mmap for OS-level shared memory (cloud paths)
```

---

## §4 — Ring Buffer Architecture (Phase 12)

xaak.rs uses a **lock-free ring buffer** for PCM.

**Rationale — why ring buffer, not alternatives:**

| Option | Zero-copy | Lock-free | RT-safe | Verdict |
|--------|-----------|-----------|---------|---------|
| `Arc<RwLock<Vec<f32>>>` | ❌ | ❌ | ❌ | Rejected — read locks cause latency spikes |
| **Lock-free ring buffer** | ✅ | ✅ | ✅ | **Selected — Phase 12** |
| mmap (shared memory) | ✅ | ✅ | ✅ | Deferred — Phase 14 (cloud only) |

**Ring buffer contract:**

```rust
// Phase 12 — xaak.rs interface (not implemented until Phase 12)
pub struct XaakKernel {
    buffer: ringbuf::HeapRb<f32>,  // lock-free, single-producer multi-consumer
    sample_rate: u32,
    channels:    u16,
    blob_id:     uuid::Uuid,
}

impl XaakKernel {
    // DSP writes PCM here after mastering
    pub fn write(&mut self, samples: &[f32]);

    // All consumers read from here — zero-copy
    pub fn consumer(&self) -> ringbuf::HeapConsumer<f32>;
}
```

**Crate:** `ringbuf` (MIT license, pure Rust, lock-free SPMC)

**Rule:** xaak.rs must never be implemented before Phase 12.
Any PR adding xaak.rs before Phase 12 is a constitutional violation.

---

## §5 — UI Isolation (extends A-002)

The Cockpit (Dioxus or Leptos) has zero access to PCM.
This extends Amendment A-002 §4 (UI is Disposable).

The Cockpit sees only:

```
SessionStateJson    ← metrics, findings, narrative
Telemetry values    ← LUFS, LRA, True Peak (numbers only)
Coach findings      ← structured data only
Export triggers     ← commands only
Playback commands   ← play/pause/seek/stop
```

**Rule:** A Cockpit component that imports xaak.rs, cpal, or
any audio buffer type is a constitutional violation.
Enforced by `ui-isolation-check.sh` (Amendment A-002 §8).

Update `ui-isolation-check.sh` to also check for:
```bash
UI_CRATES+=("xaak" "cpal" "ringbuf" "audiopus" "cpal")
```

---

## §6 — DSP Independence (fundsp)

sp314-dsp remains:
- Independent
- Deterministic
- Pure Rust
- Unaware of xaak.rs or Xaak-rs

fundsp is a **graph layer inside sp314-dsp**, not a separate engine.

**fundsp rules:**
- Used for: inline DSP graphs, modular chains, 8-stage pipelines
- Does not expose its own PCM lifecycle
- Does not bypass xaak.rs
- Does not change the export layer
- Does not change SessionStateJson

```
sp314-dsp {
    fundsp graph layer  ← internal implementation detail
    8-stage pipeline    ← uses fundsp internally
}
→ outputs PCM → xaak.rs takes ownership
```

**Constitutional rule:**
> "fundsp is an internal graph layer of sp314-dsp.
> It must not expose a PCM lifecycle or bypass xaak.rs."

---

## §7 — Export Routing Stability

The export layer remains stable across all phases.
Phase 12 changes only the PCM source — not the encoding logic.

```
Phase 9–11:  encoder.encode(&stored_blob.audio_bytes)
Phase 12+:   encoder.encode(&xaak.read())
```

Export providers:

| Format | Provider | Phase |
|--------|----------|-------|
| WAV | `WavExportProvider` (hound) | Phase 10+ |
| FLAC | `FlacExportProvider` (direct bytes) | Phase 10+ |
| OPUS | `OpusExportProvider` (audiopus) | Phase 11+ |
| MP3 | `Mp3ExportProvider` | Phase 13+ |

**Rule:** Export providers never own PCM.
They receive a slice reference and encode it. That is all.

---

## §8 — cpal as PCM I/O Backend

cpal is the PCM I/O driver. It sits below xaak.rs.

```
xaak.rs (PCM kernel) → cpal (I/O to device)
```

**cpal rules:**
- Activates Phase 12 only
- Does not own PCM
- Does not appear in SessionState
- Does not appear in Cockpit
- Does not appear in Coach
- Is not visible above xaak.rs

**Constitutional rule:**
> "cpal operates as PCM I/O backend under xaak.rs.
> It must not own or manage PCM outside the kernel lifecycle."

---

## §9 — External Spine (Xaak-rs + sendspin)

Xaak-rs and sendspin are Phase 14 only.

```
xaak.rs (internal PCM) → Xaak-rs + sendspin → cloud / other devices
```

**Rules:**
- May not activate before Phase 14
- May not own the internal PCM kernel
- May not modify SessionStateJson
- May not modify the Cockpit contract
- May not bypass xaak.rs to access sp314-dsp directly

**Constitutional rule:**
> "sendspin and Xaak-rs operate only as external spine above xaak.rs,
> without acquiring ownership of PCM or modifying the internal audio kernel."

---

## §10 — CI Enforcement

Update `infra/ci/checks/ui-isolation-check.sh`:

```bash
# Add audio kernel crates to UI_CRATES list
# These must never appear in Cockpit or UI components
UI_CRATES+=(
    "xaak"       # internal PCM kernel — core only
    "cpal"       # I/O backend — core only
    "ringbuf"    # lock-free buffer — core only
)
```

Add Phase 12 gate: before xaak.rs is activated, a CI check
must verify it does not exist in any crate before Phase 12:

```bash
# phase-guard-check.sh (Phase 11 and earlier)
if grep -r "xaak" lineos/m1/ apps/ aether/ 2>/dev/null | grep -v "amendments"; then
    echo "❌ xaak.rs referenced before Phase 12 — constitutional violation"
    exit 1
fi
```

---

## §11 — Forbidden (All Phases)

```
❌ xaak.rs before Phase 12
❌ Xaak-rs before Phase 14
❌ cpal before Phase 12
❌ PCM in SessionStateJson (any phase)
❌ PCM in Cockpit (any phase)
❌ PCM crossing Tauri IPC (any phase)
❌ fundsp exposing its own PCM lifecycle
❌ Arc<RwLock<Vec<f32>>> as PCM ownership pattern
❌ mmap before Phase 14
❌ Two subsystems sharing PCM ownership simultaneously
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-16 | Initial amendment — dual-layer audio spine, ring buffer architecture, phase activation rules, fundsp/cpal/sendspin boundaries |

---

**Lead Architect:** Anestis
**System:** Creator OS
**Document:** `creator-os/constitution/amendments/A-003-audio-spine.md`
**Version:** 1.0
**Date:** 2026-04-16
**Status:** 🔒 LOCKED

---

*PCM έχει έναν ιδιοκτήτη.*
*Ο kernel έχει ένα layer.*
*Το UI δεν αγγίζει audio.*
*Phase bleed είναι constitutional violation.*
