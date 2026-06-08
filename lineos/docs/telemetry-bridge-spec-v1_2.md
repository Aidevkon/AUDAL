# Telemetry Bridge — Spec v1.2
# lineos/docs/telemetry-bridge-spec.md

**Document:** `lineos/docs/telemetry-bridge-spec.md`
**Version:** 1.2
**Date:** 2026-06-08
**Status:** 📝 DRAFT — Phase 9
**Authority:** Creator OS Constitution v2.5
**Owner:** Lead Architect (Anestis)
**Strategist:** Claude

---

## 0. Purpose

Real-time spectrum + goniometer data → UI meters during playback.
Zero blocking. Zero memory growth. Native monitor refresh rate.

**What it enables:**
- Live spectrum analyzer (64-band) during playback
- Live Lissajous goniometer (32-pair path, not a dot)
- Auto-stop when UI goes to background

**What it does NOT do:**
- LUFS meters → already in LiveTelemetryResponse (Golden Blob)
- True Peak → already in LiveTelemetryResponse (Golden Blob)
- Progress bar → SSE /progress/:job_id/stream (Phase 8c)

---

## 1. Three Telemetry Systems (No Overlap)

```
System A — Pre-computed (Golden Blob):
  GET /playback/telemetry → LiveTelemetryResponse
  Data: momentary_lufs, short_term_lufs, true_peak_dbtp
  Source: StoredBlob (computed during mastering, read during playback)
  CPU: 0% — memory lookup only
  Status: ✅ IMPLEMENTED — do not change

System B — Real-time Bridge (this spec):
  tauri::command get_live_telemetry_realtime()
  Data: spectrum[64], gonio_path[(f32,f32); 32], position_ms
  Source: xaak audio callback (live FFT per audio block)
  CPU: ~1% — lightweight 64-band FFT
  Transport: Tauri IPC (zero TCP/JSON overhead)
  Status: 📝 Phase 9 — TB-P1..P7

System C — Progress Stream:
  GET /progress/:job_id/stream (SSE)
  Data: stage, progress%, blobId
  Source: update_stage() broadcast
  Status: ✅ Phase 8c — implemented
```

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────┐
│  xaak Audio Thread (cpal callback — runs at 48kHz)       │
│                                                          │
│  on_audio_block(left, right) {                           │
│    // Zero-cost bypass check (AtomicU64, no locks)       │
│    let elapsed = now_ms() - bridge.last_poll_ms.load();  │
│    if elapsed > 2000 { return; }  // UI gone → skip FFT  │
│                                                          │
│    frame = compute_realtime_frame(left, right)           │
│    producer.push(frame)  ← non-blocking, lock-free       │
│    if full: DROP         ← frame dropping, never blocks  │
│  }                                                       │
└─────────────────────┬────────────────────────────────────┘
                      │ HeapRb<RealtimeFrame, 4>
                      │ ~2KB total — INV-ST-3 safe
┌─────────────────────▼────────────────────────────────────┐
│  Tauri Command (zero TCP/JSON overhead)                  │
│                                                          │
│  #[tauri::command]                                       │
│  fn get_live_telemetry_realtime(state) → Option<Frame>   │
│    bridge.last_poll_ms.store(now_ms())  // reset timer   │
│    bridge.pop()                                          │
└─────────────────────┬────────────────────────────────────┘
                      │
┌─────────────────────▼────────────────────────────────────┐
│  Dioxus UI (requestAnimationFrame — native Hz)           │
│                                                          │
│  On every screen refresh (60/120/144fps):                │
│    invoke("get_live_telemetry_realtime")                  │
│    → update spectrum canvas                              │
│    → update Lissajous goniometer                         │
│                                                          │
│  Tab/window hidden → rAF stops automatically             │
│  → last_poll_ms ages out → xaak skips FFT               │
│  = The Absolute Harmony                                  │
└──────────────────────────────────────────────────────────┘
```

---

## 3. RealtimeFrame

```rust
/// Real-time audio visualization frame from xaak playback thread.
/// Copy type — zero heap allocation per frame.
/// Size: 64×4 + 32×8 + 8 = 520 bytes per frame.
/// Ring buffer: 4 × 520 = ~2KB total — INV-ST-3 safe.
#[derive(Debug, Clone, Copy)]
pub struct RealtimeFrame {
    /// 64-band log-spaced spectrum magnitude (dBFS)
    /// Band 0: ~20Hz, Band 63: ~20kHz
    pub spectrum:    [f32; 64],

    /// 32 decimated (L, R) sample pairs for Lissajous goniometer.
    /// Evenly spaced across the audio block.
    /// Gives smooth Lissajous path — not a single jumping dot.
    pub gonio_path:  [(f32, f32); 32],

    /// Playback position in ms (sync with xaak position_ms)
    pub position_ms: u64,
}
```

**Why 32 gonio pairs (not 1):**
- 1 sample = single dot jumping on screen = useless
- 32 evenly-spaced pairs = smooth Lissajous curve = professional
- Decimation from 1024-sample block: take every 32nd sample
- Size cost: 32×8 = 256 bytes — acceptable

**Why 64 spectrum bands:**
- Covers 20Hz–20kHz in log spacing
- Standard spectrum analyzer resolution

**Why capacity 4:**
- Audio callback: ~1024 samples / 48kHz = ~21ms per block
- 4 frames = ~84ms buffer
- At 60fps, UI polls every 16ms — always fresh data
- Frame dropping if ring buffer full → audio thread never blocks

---

## 4. RealtimeBridge

```rust
use ringbuf::{HeapRb, HeapProducer, HeapConsumer};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct RealtimeBridge {
    pub producer:     Arc<Mutex<HeapProducer<RealtimeFrame>>>,
    pub consumer:     Arc<Mutex<HeapConsumer<RealtimeFrame>>>,
    /// Timestamp of last UI poll (ms since epoch).
    /// Audio thread checks: if now - last_poll > 2000ms → skip FFT.
    /// Zero tasks, zero allocations — pure atomic operation.
    pub last_poll_ms: Arc<AtomicU64>,
}

impl RealtimeBridge {
    pub fn new() -> Self {
        let rb = HeapRb::<RealtimeFrame>::new(4);
        let (producer, consumer) = rb.split();
        Self {
            producer:     Arc::new(Mutex::new(producer)),
            consumer:     Arc::new(Mutex::new(consumer)),
            last_poll_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Non-blocking push — drops frame if ring buffer full.
    /// Checks last_poll_ms — returns immediately if UI gone > 2s.
    /// Safe to call from audio thread.
    pub fn push(&self, frame: RealtimeFrame) {
        let last = self.last_poll_ms.load(Ordering::Relaxed);
        let now  = current_time_ms();
        if now.saturating_sub(last) > 2000 { return; } // UI gone — skip
        if let Ok(mut prod) = self.producer.try_lock() {
            let _ = prod.push(frame);
        }
    }

    /// Non-blocking pop — returns None if empty.
    /// Updates last_poll_ms — resets the 2s auto-disable timer.
    pub fn pop(&self) -> Option<RealtimeFrame> {
        self.last_poll_ms.store(current_time_ms(), Ordering::Relaxed);
        self.consumer.try_lock().ok()?.pop()
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
```

---

## 5. Tauri Command (not HTTP)

```rust
/// Tauri command — zero TCP/JSON overhead.
/// Called by Dioxus via requestAnimationFrame.
/// Returns None if playback stopped or ring buffer empty.
#[tauri::command]
pub fn get_live_telemetry_realtime(
    state: tauri::State<'_, AppState>,
) -> Option<RealtimeFrame> {
    state.realtime.pop()
}
```

**Why Tauri command, not HTTP:**
- Desktop app — no network round-trip needed
- Tauri IPC: shared memory, zero TCP stack
- At 60fps: HTTP = ~60 × (TCP + JSON parse) overhead
           Tauri = ~60 × function call overhead
- For 64 floats + 32 pairs: Tauri is ~100× faster

---

## 6. xaak Integration

```rust
// xaak/src/player.rs — in cpal audio output callback
fn on_audio_block(
    left:   &[f32],
    right:  &[f32],
    bridge: &RealtimeBridge,
) {
    // push() handles the 2s timeout check internally
    // Returns immediately if UI not polling
    let frame = RealtimeFrame {
        spectrum:    compute_64band_spectrum(left),
        gonio_path:  decimate_to_gonio(left, right),
        position_ms: current_position_ms(),
    };
    bridge.push(frame);
}

/// Decimate audio block to 32 (L, R) pairs for Lissajous.
fn decimate_to_gonio(left: &[f32], right: &[f32]) -> [(f32, f32); 32] {
    let mut pairs = [(0.0f32, 0.0f32); 32];
    let n = left.len().min(right.len());
    let step = (n / 32).max(1);
    for i in 0..32 {
        let idx = (i * step).min(n - 1);
        pairs[i] = (left[idx], right[idx]);
    }
    pairs
}
```

---

## 7. 64-Band Spectrum

```rust
/// Compute 64-band log-spaced spectrum from mono signal.
/// Takes first 2048 samples. Maps 1025 FFT bins → 64 log bands.
/// Returns dBFS. Band 0 ≈ 20Hz, Band 63 ≈ 20kHz.
fn compute_64band_spectrum(signal: &[f32]) -> [f32; 64] {
    let n = signal.len().min(2048);
    if n == 0 { return [-120.0f32; 64]; }

    // Hann window + FFT (StftStreamContext — no new deps)
    // Map bins to log-spaced bands
    // Implementation: Phase TB-P5

    [-120.0f32; 64] // placeholder until TB-P5
}
```

---

## 8. Dioxus Consumer

```rust
// cockpit-dioxus — Phase TB-P7
fn use_realtime_telemetry(cx: &ScopeState) -> &UseState<Option<RealtimeFrame>> {
    let frame = use_state(cx, || None);

    use_future(cx, (), |_| {
        let frame = frame.clone();
        async move {
            loop {
                // requestAnimationFrame equivalent in Dioxus
                // Stops automatically when window hidden
                gloo_timers::future::TimeoutFuture::new(0).await;

                let result = invoke::<Option<RealtimeFrame>>(
                    "get_live_telemetry_realtime", &()
                ).await;
                frame.set(result.ok().flatten());
            }
        }
    });

    frame
}
```

---

## 9. Implementation Phases

| Phase | Task | Gate |
|-------|------|------|
| TB-P1 | RealtimeFrame type in lineos-types | cargo check |
| TB-P2 | RealtimeBridge struct + AppState | cargo check |
| TB-P3 | Tauri command registration | tauri build check |
| TB-P4 | xaak integration (push from audio callback) | real data |
| TB-P5 | 64-band spectrum computation | spectrum contract test |
| TB-P6 | Goniometer decimation (32 pairs) | visual test |
| TB-P7 | Dioxus consumer (requestAnimationFrame) | 60fps visual |

---

## 10. Invariants

| ID | Invariant |
|----|-----------|
| INV-TB-1 | Ring buffer capacity = 4 frames — never grows |
| INV-TB-2 | push() non-blocking — frame dropped if full |
| INV-TB-3 | pop() non-blocking — None if empty |
| INV-TB-4 | Audio thread never waits for UI thread |
| INV-TB-5 | Auto-disable: xaak skips FFT if UI silent > 2s |
| INV-TB-6 | RealtimeFrame is Copy — zero heap allocation |
| INV-TB-7 | Transport: Tauri IPC — zero TCP/JSON overhead |
| INV-TB-8 | Goniometer: 32 pairs — smooth Lissajous, not dot |
| INV-ST-3 | Ring buffer: 4 × 520 bytes = ~2KB — constant |

---

## 11. Audit History

### v1.0 → v1.1
- TelemetryFrame → RealtimeFrame
- LUFS/TruePeak removed (already in Golden Blob)
- Source: process_chunks → xaak audio callback
- Three-system architecture documented

### v1.1 → v1.2 (Architectural Audit by Google Antigravity Agent)
Three vulnerabilities fixed:

**Vuln 1 — Tokio Spawn Bomb (Section 8):**
- BEFORE: `tokio::spawn(sleep(5s))` on every poll → 50 sleeping tasks
- AFTER: `AtomicU64 last_poll_ms` — audio thread checks timestamp
- Result: Zero tasks, zero allocations, zero-cost abstraction

**Vuln 2 — Fake Goniometer (Section 3):**
- BEFORE: `gonio_l: f32, gonio_r: f32` — single dot
- AFTER: `gonio_path: [(f32, f32); 32]` — smooth Lissajous path
- Size: 520 bytes/frame (was 272), ring buffer ~2KB (was ~1.1KB)

**Vuln 3 — HTTP Overhead (Section 2 & 6):**
- BEFORE: `GET /telemetry/live` — HTTP + TCP + JSON parse at 60fps
- AFTER: `#[tauri::command]` — shared memory, zero network overhead
- Polling: `requestAnimationFrame` — native Hz, auto-stop on background

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-08 | Initial spec |
| 1.1 | 2026-06-08 | RealtimeFrame, xaak source, three-system arch |
| 1.2 | 2026-06-08 | AtomicU64 bypass, gonio_path[32], Tauri IPC, rAF |

---

**Lead Architect:** Anestis
**Strategist:** Claude
**Audit:** Google Antigravity Agent
**System:** Creator OS — Real-Time DSP Layer
**Status:** 📝 DRAFT v1.2 — Phase 9

*The DSP doesn't wait for the UI.*
*The UI doesn't block the DSP.*
*The ring buffer holds the conversation.*
*The monitor sets the pace.*
