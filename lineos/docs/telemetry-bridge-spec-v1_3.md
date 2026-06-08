# Telemetry Bridge — Spec v1.3
# lineos/docs/telemetry-bridge-spec.md

**Version:** 1.3  **Date:** 2026-06-08  **Status:** DRAFT Phase 9
**Authority:** Creator OS Constitution v2.5
**Owner:** Lead Architect (Anestis) / Strategist: Claude

---

## Critical Architecture Note

m0-daemon and src-tauri are SEPARATE PROCESSES.
Tauri command cannot access m0-daemon AppState directly.
Solution: UDP localhost 127.0.0.1:9000 (Dante/game audio model).

---

## Three Systems (No Overlap)

System A: GET /playback/telemetry → LUFS from Golden Blob ✅ DONE
System B: UDP Bridge → spectrum + goniometer from xaak 📝 Phase 9
System C: GET /progress/:job_id/stream → SSE mastering progress ✅ DONE

---

## Architecture

m0-daemon xaak audio callback:
  frame = compute_realtime_frame(left, right)
  bytes = bincode::encode(frame)  // 520 bytes, zero alloc
  udp_socket.send_to(bytes, "127.0.0.1:9000")  // fire and forget

UDP 127.0.0.1:9000 (~0.01ms, kernel memory copy on localhost)

src-tauri UDP listener (background thread):
  recv() → decode → Arc<Mutex<Option<RealtimeFrame>>>
  always overwrites latest frame

Tauri command get_live_telemetry_realtime():
  reads Mutex → Option<RealtimeFrame> → Dioxus

Dioxus requestAnimationFrame:
  invoke() → render spectrum + Lissajous goniometer

---

## RealtimeFrame (520 bytes, Copy, no heap)

pub struct RealtimeFrame {
    pub spectrum:    [f32; 64],          // 64-band log-spaced dBFS
    pub gonio_path:  [(f32, f32); 32],   // 32 pairs — smooth Lissajous
    pub position_ms: u64,                // sync with xaak position
}

---

## Implementation Phases

TB-P1: RealtimeFrame type in lineos-types        ✅ DONE
TB-P2: RealtimeBridge (ring buffer, for testing) ✅ DONE
TB-P3: UDP sender in xaak (m0-daemon)            📝 next
TB-P4: UDP listener in src-tauri + LatestFrame   📝 next
TB-P5: Tauri command registration                📝 next
TB-P6: 64-band spectrum computation              📝 future
TB-P7: Goniometer decimation (32 pairs)          📝 future
TB-P8: Dioxus consumer (requestAnimationFrame)   📝 cockpit scope

---

## Invariants

INV-TB-1: UDP fire-and-forget — sender never blocks audio thread
INV-TB-2: bincode — zero heap allocation per frame
INV-TB-3: src-tauri listener always overwrites latest frame
INV-TB-4: Audio thread never waits for UI thread
INV-TB-5: RealtimeFrame is Copy — 520 bytes, no heap
INV-TB-6: Tauri command reads Mutex — zero network hop to UI
INV-TB-7: Goniometer 32 pairs — smooth Lissajous, not a dot
INV-TB-8: Spectrum 64 log-spaced bands 20Hz–20kHz

---

## Why UDP

Zero backpressure — sender never blocks
bincode: 520 bytes fixed, zero heap
localhost UDP: kernel memory copy ~0.01ms
Industry standard: Dante audio protocol, game state sync

RealtimeBridge (ring buffer in AppState): retained for testing.
Will deprecate after UDP proven in production.

---

## Changelog

v1.0 2026-06-08: Initial spec
v1.1 2026-06-08: RealtimeFrame, xaak source, three-system arch
v1.2 2026-06-08: AtomicU64 bypass, gonio[32], Tauri IPC
v1.3 2026-06-08: UDP architecture (separate processes discovered)
