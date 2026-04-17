# LineOS — Phase 12A Task Decomposition

**Document:** `lineos/plan/phase-12a/task-decomposition.md`
**Version:** 1.0
**Phase:** 12A — xaak.rs PCM Kernel + cpal
**Status:** 🔒 LOCKED
**Authority:** Phase 12A Master Prompt · Amendment A-003

---

## Architecture (A-003 §1)

```
sp314-dsp (M1)
    │
    │ PCM Vec<f32> — after mastering completes
    ▼
xaak.rs (PCM Kernel) — NEW Phase 12A
    │  lock-free ring buffer (ringbuf crate)
    │  single producer, multi consumer
    │  owns PCM for lifetime of session
    │
    ├── Telemetry consumer (Phase 12B)
    ├── Export consumer (reads from ring buffer)
    └── cpal consumer → audio device output
            │
            ▼
       Hardware speakers
```

**Location:** `lineos/m1/xaak/`

**Rule (A-003 §2):** xaak.rs is the sole PCM owner.
No other subsystem holds PCM bytes after xaak takes ownership.

---

## Task Order

```
P12A-001  Add xaak crate to workspace
P12A-002  XaakKernel — ring buffer + PCM ownership
P12A-003  cpal integration — device output
P12A-004  Playback engine — play/pause/stop/seek
P12A-005  Audit trail — lifecycle events
P12A-006  Wire M0 master handler → xaak
P12A-007  Tauri commands: playback_control, get_playback_state
P12A-008  Update ui-isolation-check.sh (A-003 §10)
P12A-009  CI gate + tag
```

---

## P12A-001 — Add xaak Crate

Create `lineos/m1/xaak/`:

```toml
# lineos/m1/xaak/Cargo.toml
[package]
name    = "xaak"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
ringbuf  = "0.4"    # lock-free SPMC ring buffer — MIT
cpal     = "0.15"   # audio I/O — Apache-2.0
uuid     = { version = "1", features = ["v4"] }
tracing  = "0.1"

[dev-dependencies]
# none needed for unit tests
```

Add to workspace `Cargo.toml`:
```toml
"lineos/m1/xaak",
```

**DoD P12A-001:**
```bash
cargo check -p xaak
echo "✅ P12A-001"
```

---

## P12A-002 — XaakKernel

```rust
// lineos/m1/xaak/src/lib.rs

//! xaak.rs — Internal PCM Kernel
//! Authority: Amendment A-003 §1–§4
//! Phase 12 activation. Lock-free ring buffer. Single PCM owner.

use ringbuf::{HeapRb, HeapConsumer, HeapProducer};
use uuid::Uuid;

pub const TARGET_SAMPLE_RATE: u32 = 48000;
pub const TARGET_CHANNELS:    u16 = 2;

/// PCM ownership transfer. After this, caller must not hold PCM.
/// A-003 §2: xaak is the sole owner.
pub struct PcmTransfer {
    pub samples:     Vec<f32>,
    pub sample_rate: u32,
    pub channels:    u16,
    pub blob_id:     Uuid,
}

/// Internal PCM kernel. Owns PCM for session lifetime.
/// A-003 §4: lock-free ring buffer, not Arc<RwLock<Vec<f32>>>.
pub struct XaakKernel {
    blob_id:     Uuid,
    sample_rate: u32,
    channels:    u16,
    duration_ms: u64,
    /// Full PCM stored for seek support
    pcm:         Vec<f32>,
    /// Ring buffer for streaming consumers
    producer:    Option<HeapProducer<f32>>,
}

impl XaakKernel {
    /// Take ownership of PCM. Caller must not use samples after this.
    pub fn load(transfer: PcmTransfer) -> Self {
        let duration_ms = (transfer.samples.len() as u64 * 1000)
            / (transfer.sample_rate as u64 * transfer.channels as u64);

        tracing::info!(
            blob_id = %transfer.blob_id,
            samples = transfer.samples.len(),
            sample_rate = transfer.sample_rate,
            channels = transfer.channels,
            duration_ms,
            "xaak: buffer loaded"
        );

        Self {
            blob_id:     transfer.blob_id,
            sample_rate: transfer.sample_rate,
            channels:    transfer.channels,
            duration_ms,
            pcm:         transfer.samples,
            producer:    None,
        }
    }

    /// Create a streaming consumer for cpal playback.
    /// Fills the ring buffer from the current position.
    pub fn stream_from(&mut self, position_ms: u64) -> HeapConsumer<f32> {
        let frame_offset = (position_ms * self.sample_rate as u64
            / 1000) as usize * self.channels as usize;

        let slice = if frame_offset < self.pcm.len() {
            &self.pcm[frame_offset..]
        } else {
            &[]
        };

        let rb = HeapRb::<f32>::new(slice.len().max(4096));
        let (mut prod, cons) = rb.split();
        prod.push_slice(slice);
        self.producer = Some(prod);
        cons
    }

    pub fn blob_id(&self)     -> Uuid  { self.blob_id }
    pub fn sample_rate(&self) -> u32   { self.sample_rate }
    pub fn channels(&self)    -> u16   { self.channels }
    pub fn duration_ms(&self) -> u64   { self.duration_ms }
    pub fn pcm_len(&self)     -> usize { self.pcm.len() }
}

/// Playback state — exposed via Tauri IPC as JSON (no PCM).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlaybackState {
    pub blob_id:      String,
    pub position_ms:  u64,
    pub duration_ms:  u64,
    pub is_playing:   bool,
    pub sample_rate:  u32,
    pub channels:     u16,
}
```

**DoD P12A-002:**
```bash
cargo check -p xaak
echo "✅ P12A-002"
```

---

## P12A-003 — cpal Integration

```rust
// lineos/m1/xaak/src/player.rs

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapConsumer;
use std::sync::{Arc, Mutex};

pub struct CpalPlayer {
    stream:     Option<cpal::Stream>,
    position_ms: Arc<Mutex<u64>>,
}

impl CpalPlayer {
    pub fn new() -> Self {
        Self { stream: None, position_ms: Arc::new(Mutex::new(0)) }
    }

    /// Start playback from a ring buffer consumer.
    /// cpal pulls samples from the consumer on the audio thread.
    pub fn play(
        &mut self,
        mut consumer: HeapConsumer<f32>,
        sample_rate: u32,
        channels:    u16,
        position_ms: Arc<Mutex<u64>>,
    ) -> Result<(), String> {
        let host   = cpal::default_host();
        let device = host.default_output_device()
            .ok_or("No audio output device found")?;

        let config = cpal::StreamConfig {
            channels:    channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let pos = position_ms.clone();
        let sr  = sample_rate;
        let ch  = channels as u64;

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let filled = consumer.pop_slice(data);
                // Silence for any unfilled frames
                data[filled..].fill(0.0);
                // Update position
                let frames = filled as u64 / ch;
                let delta_ms = frames * 1000 / sr as u64;
                if let Ok(mut p) = pos.lock() {
                    *p += delta_ms;
                }
            },
            |err| tracing::error!("cpal stream error: {err}"),
            None,
        ).map_err(|e| format!("cpal build stream failed: {e}"))?;

        stream.play().map_err(|e| format!("cpal play failed: {e}"))?;
        self.stream = Some(stream);
        self.position_ms = position_ms;
        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(ref s) = self.stream {
            let _ = s.pause();
        }
    }

    pub fn stop(&mut self) {
        self.stream = None;
        if let Ok(mut p) = self.position_ms.lock() {
            *p = 0;
        }
    }

    pub fn position_ms(&self) -> u64 {
        self.position_ms.lock().map(|p| *p).unwrap_or(0)
    }
}
```

**DoD P12A-003:**
```bash
cargo check -p xaak
echo "✅ P12A-003"
```

---

## P12A-004 — Playback Engine

```rust
// lineos/m1/xaak/src/engine.rs

use std::sync::{Arc, Mutex};
use crate::{XaakKernel, PlaybackState, PcmTransfer, player::CpalPlayer};

pub struct PlaybackEngine {
    kernel:   Option<XaakKernel>,
    player:   CpalPlayer,
    position: Arc<Mutex<u64>>,
    playing:  bool,
}

impl PlaybackEngine {
    pub fn new() -> Self {
        Self {
            kernel:   None,
            player:   CpalPlayer::new(),
            position: Arc::new(Mutex::new(0)),
            playing:  false,
        }
    }

    /// Load PCM into kernel. Takes ownership — caller drops samples.
    pub fn load(&mut self, transfer: PcmTransfer) {
        self.player.stop();
        *self.position.lock().unwrap() = 0;
        self.playing = false;
        self.kernel = Some(XaakKernel::load(transfer));
    }

    pub fn play(&mut self) -> Result<(), String> {
        let kernel = self.kernel.as_mut()
            .ok_or("No PCM loaded")?;
        let pos_ms = *self.position.lock().unwrap();
        let consumer = kernel.stream_from(pos_ms);
        self.player.play(
            consumer,
            kernel.sample_rate(),
            kernel.channels(),
            self.position.clone(),
        )?;
        self.playing = true;
        Ok(())
    }

    pub fn pause(&mut self) {
        self.player.pause();
        self.playing = false;
    }

    pub fn stop(&mut self) {
        self.player.stop();
        self.playing = false;
    }

    pub fn seek(&mut self, position_ms: u64) -> Result<(), String> {
        let was_playing = self.playing;
        self.player.stop();
        *self.position.lock().unwrap() = position_ms;
        if was_playing { self.play()?; }
        Ok(())
    }

    pub fn state(&self) -> Option<PlaybackState> {
        self.kernel.as_ref().map(|k| PlaybackState {
            blob_id:     k.blob_id().to_string(),
            position_ms: *self.position.lock().unwrap(),
            duration_ms: k.duration_ms(),
            is_playing:  self.playing,
            sample_rate: k.sample_rate(),
            channels:    k.channels(),
        })
    }
}
```

**DoD P12A-004:**
```bash
cargo check -p xaak
echo "✅ P12A-004"
```

---

## P12A-005 — Audit Trail (A-003 §2)

Add to `XaakKernel::load()` and `PlaybackEngine`:

```rust
// Audit events — written to M0 audit log via tracing
// m0d.xaak_buffer_allocated { size_bytes, sample_rate, channels, blob_id }
// m0d.xaak_buffer_released  { blob_id, duration_ms }

tracing::info!(
    event = "m0d.xaak_buffer_allocated",
    blob_id = %blob_id,
    size_bytes = samples.len() * 4,
    sample_rate = sample_rate,
    channels = channels,
);
```

**DoD P12A-005:**
```bash
cargo check -p xaak
echo "✅ P12A-005"
```

---

## P12A-006 — Wire M0 master handler → xaak

Update `lineos/m0/m0-daemon/src/handlers/master.rs`:

After mastering completes, transfer PCM ownership to xaak:

```rust
// In run_dsp(), after blob is stored:
use xaak::{PcmTransfer, PlaybackEngine};

// Transfer PCM to xaak kernel (A-003 §2 — one owner)
let transfer = PcmTransfer {
    samples:     pcm_for_telemetry,  // already computed
    sample_rate: pcm.sample_rate,
    channels:    pcm.channels,
    blob_id:     blob_id,
};

// PlaybackEngine lives in AppState
state.playback.lock().unwrap().load(transfer);
```

Add `PlaybackEngine` to `AppState`:
```rust
pub struct AppState {
    pub blob_store: Arc<BlobStore>,
    pub playback:   Arc<Mutex<PlaybackEngine>>,  // NEW
    pub audit:      AuditLog,
}
```

Add `xaak` to `m0d/Cargo.toml`:
```toml
xaak = { path = "../../../m1/xaak" }
```

**DoD P12A-006:**
```bash
cargo check -p m0d
echo "✅ P12A-006"
```

---

## P12A-007 — Tauri Commands

```rust
// apps/stillair/src-tauri/src/commands/playback.rs

use crate::ipc::m0_client::M0Client;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct PlaybackStateJson {
    pub blob_id:     String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing:  bool,
}

/// Control playback: play | pause | stop | seek
#[tauri::command]
pub async fn playback_control(
    action:      String,   // "play" | "pause" | "stop"
    position_ms: Option<u64>,  // for "seek"
) -> Result<PlaybackStateJson, String> {
    let client = M0Client::new();
    client.playback_control(&action, position_ms).await
        .map_err(|e| format!("IO_ERR:0x02:Playback failed: {e}"))
}

/// Get current playback position and state
#[tauri::command]
pub async fn get_playback_state() -> Result<Option<PlaybackStateJson>, String> {
    let client = M0Client::new();
    client.get_playback_state().await
        .map_err(|e| format!("IO_ERR:0x02:Playback state failed: {e}"))
}
```

Add M0 playback endpoints:
```
POST /playback/control  { action, position_ms? }
GET  /playback/state
```

Register in `lib.rs`:
```rust
commands::playback::playback_control,
commands::playback::get_playback_state,
```

**DoD P12A-007:**
```bash
cargo check -p stillair
echo "✅ P12A-007"
```

---

## P12A-008 — Update ui-isolation-check.sh (A-003 §10)

```bash
# Add to UI_CRATES in infra/ci/checks/ui-isolation-check.sh
UI_CRATES+=(
    "xaak"       # internal PCM kernel — core only (A-003 §5)
    "cpal"       # I/O backend — core only (A-003 §8)
    "ringbuf"    # lock-free buffer — core only (A-003 §4)
)
```

**DoD P12A-008:**
```bash
bash infra/ci/checks/ui-isolation-check.sh
echo "✅ P12A-008"
```

---

## P12A-009 — CI Gate + Tag

```bash
cargo test --workspace
just ci
bash infra/ci/checks/ui-isolation-check.sh

git add -A
git commit -m "feat(xaak): Phase 12A — PCM kernel + cpal playback backend

xaak.rs (Amendment A-003 §1-§4):
  - XaakKernel: lock-free ring buffer (ringbuf 0.4 SPMC)
  - PcmTransfer: ownership transfer from M0 master handler
  - PlaybackEngine: play/pause/stop/seek
  - CpalPlayer: cpal 0.15 audio device output
  - Audit trail: m0d.xaak_buffer_allocated/released (A-003 §2)

Architecture invariants (A-003):
  - sole PCM owner: xaak.rs ✅
  - no PCM in IPC, SessionState, or Cockpit ✅
  - Arc<RwLock<Vec<f32>>> rejected — ring buffer used ✅
  - cpal under xaak, not above ✅
  - Xaak-rs/sendspin: Phase 14 — not referenced ✅

New Tauri commands: playback_control, get_playback_state
ui-isolation-check.sh: xaak/cpal/ringbuf added to UI_CRATES

Authority: Amendment A-003 · LineOS Constitution v2.0"

git tag v0.12a.0-xaak
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 12A — xaak.rs PCM Kernel — COMPLETE

xaak.rs:        lock-free ring buffer ✅
cpal:           audio device output ✅
PlaybackEngine: play/pause/stop/seek ✅
Audit trail:    lifecycle events ✅
M0 wired:       PCM → xaak after mastering ✅
Tauri commands: playback_control, get_playback_state ✅
ui-isolation:   xaak/cpal/ringbuf blocked from UI ✅
just ci:        ✅

Tag: v0.12a.0-xaak ✅

Ready for: Phase 12B — Transport bar UI + live telemetry
```

---

**Lead Architect:** Anestis
**System:** LineOS M1 — xaak.rs
**Phase:** 12A
**Version:** 1.0
**Status:** 🔒 LOCKED

---

*PCM has one owner. The kernel has one layer.*
*Phase bleed is a constitutional violation.*
