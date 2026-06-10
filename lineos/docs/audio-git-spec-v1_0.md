# Audio State Version Control — Spec v1.2
# lineos/docs/audio-git-spec-v1_0.md

**Document:** `lineos/docs/audio-git-spec-v1_0.md`
**Version:** 1.2 (Gemini concurrency audit applied)
**Date:** 2026-06-10
**Status:** 🏗️ ACTIVE R&D — Ready for TDD

---

## 0. Σκοπός

Άπειρες εκδοχές μίξης (Ducking, Spatial, EQ) στη μνήμη.
A/B/C/N testing και Undo/Redo σε πραγματικό χρόνο.
Zero audio dropout. Zero PCM copying.

---

## 1. Core Data Structures

```rust
use arc_swap::ArcSwap;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

/// DSP pipeline state snapshot — ~24 bytes, Copy.
/// Stored in every MixCommit. Never contains audio data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspState {
    pub ducking_depth:  f32,   // 1.0=neutral, >1.0=aggressive, <1.0=subtle
    pub sidechain_hold: usize, // frames to hold ducking after transient
    pub ms_width:       f32,   // Mid/Side width multiplier
    pub lfe_gain:       f32,   // LFE gain in dB
}

impl Default for DspState {
    fn default() -> Self {
        Self {
            ducking_depth:  1.0,
            sidechain_hold: 3,
            ms_width:       1.0,
            lfe_gain:       0.0,
        }
    }
}

/// Equivalent to a Git commit.
#[derive(Debug, Clone)]
pub struct MixCommit {
    pub hash:        String,
    pub parent_hash: Option<String>,
    pub timestamp:   u64,
    pub message:     String,
    pub state:       DspState,
}

/// The repository — holds full history + hot pointer.
pub struct AudioRepo {
    // UI Thread Data — protected by RwLock in AppState
    // UI can afford to block briefly for writes
    pub commits:       HashMap<String, MixCommit>,
    pub branches:      HashMap<String, String>, // branch_name → commit_hash
    pub active_branch: String,

    // THE HOT POINTER — 100% lock-free pointer swap
    // Arc<ArcSwap<DspState>>: audio thread holds clone of Arc,
    // reads lock-free regardless of DspState size.
    // UI thread swaps pointer on checkout/commit — never blocks audio.
    pub head_state: Arc<ArcSwap<DspState>>,
}
```

---

## 2. Thread Safety (Corrected — Gemini Audit v1.2)

```
WHY NOT AtomicCell<DspState>:
  DspState = 3×f32 + usize = ~24 bytes
  Hardware atomics support max 8-16 bytes
  crossbeam AtomicCell falls back to hidden spinlock for >16 bytes
  → priority inversion on audio thread → dropout ❌

WHY ArcSwap<DspState>:
  ArcSwap swaps the POINTER (8 bytes) — always lock-free
  DspState size is irrelevant
  Audio thread: holds Arc<ArcSwap<DspState>>, calls .load() → lock-free read
  UI thread: calls .store(Arc::new(new_state)) → lock-free pointer swap
  ✅ True lock-free, no spinlock, no priority inversion

WHY RwLock for UI data (commits, branches):
  commits/branches only accessed by UI thread (HTTP handlers)
  UI thread can afford brief blocking (not real-time)
  In AppState: Arc<RwLock<AudioRepo>>
  Audio thread: NEVER touches commits/branches HashMap
```

---

## 3. Core API (Corrected Signatures)

```rust
impl AudioRepo {
    pub fn new(initial_state: DspState) -> Self

    /// Create new commit on active branch.
    /// INV-GIT-1: stores only DspState (~24 bytes), never PCM.
    pub fn commit(&mut self, new_state: DspState, message: &str) -> String

    /// Switch active branch.
    /// &mut self — modifies active_branch + swaps head_state pointer.
    /// INV-GIT-2: ArcSwap pointer swap is O(1), lock-free.
    pub fn checkout(&mut self, branch_name: &str) -> Result<(), String>

    /// Undo: move branch pointer to parent commit.
    /// INV-GIT-3: child commit preserved in HashMap.
    pub fn revert_head(&mut self) -> Result<(), String>

    /// Create new branch from current HEAD.
    pub fn create_branch(&mut self, name: &str) -> Result<(), String>

    /// Lock-free read — safe to call from audio thread.
    /// Returns Arc<DspState> — clone is cheap (pointer bump).
    pub fn head_state(&self) -> Arc<DspState> {
        self.head_state.load_full()
    }

    pub fn branches_list(&self) -> Vec<(String, String)>
}
```

---

## 4. AppState Integration (Corrected)

```rust
// In m0-daemon AppState:
pub struct AppState {
    // RwLock wraps entire repo — UI thread can write safely
    // Audio thread only touches head_state (Arc<ArcSwap>) directly
    pub audio_repo: Arc<RwLock<AudioRepo>>,

    // Audio thread holds this separately for lock-free reads
    // Cloned from repo.head_state at startup
    pub head_state_ptr: Arc<ArcSwap<DspState>>,
}

// In HTTP handler (checkout):
async fn checkout_branch(
    State(app): State<AppState>,
    Json(req): Json<CheckoutRequest>,
) -> Json<CheckoutResponse> {
    // RwLock write — UI thread, can block briefly
    app.audio_repo.write().unwrap().checkout(&req.branch)?;
    // head_state_ptr automatically updated (same Arc)
}

// In render_node (audio path):
// Lock-free — audio thread never touches RwLock
let repo_state = app.head_state_ptr.load_full();
let stems = render_node::run(audio, &scout, &repo_state)?;
```

---

## 5. Integration with Render Node

```rust
pub fn run(
    audio:      DecodedAudio,
    scout:      &ScoutResult,
    repo_state: &DspState,
) -> RenderedStems {
    // AI base from Maestro (computed once in scout)
    let ai_base_gain = scout.suggested_ducking_gain;

    // User modifier from active branch
    // 1.0=neutral, 1.5=aggressive (Club Mix), 0.8=subtle (Radio Edit)
    let final_ducking_gain = (ai_base_gain / repo_state.ducking_depth)
        .clamp(0.1, 1.0);

    process_chunks_with_params(
        &audio.mid_channel,
        final_ducking_gain,
        repo_state.sidechain_hold,
    )
}
```

---

## 6. Invariants

| ID | Invariant |
|----|-----------|
| INV-GIT-1 | Commits store only DspState (~24 bytes). Never PCM audio. |
| INV-GIT-2 | checkout() uses ArcSwap pointer swap — O(1), truly lock-free. |
| INV-GIT-3 | revert_head() preserves child commit — Redo always possible. |
| INV-GIT-4 | head_state uses ArcSwap (not AtomicCell) — no spinlock fallback. |
| INV-GIT-5 | sp314-dsp has zero knowledge of repo/branches/commits. |
| INV-GIT-6 | Audio thread never acquires RwLock — only reads ArcSwap. |
| INV-AB-1  | Same branch + same audio → same output. Always. |

---

## 7. TDD Plan

```
File: lineos/m1/xaak/src/repo.rs

test_repo_initialization
test_commit_chaining (3 commits, correct parent chain)
test_zero_latency_checkout (head_state changes on checkout)
test_revert_undo (state_b reverts to state_a, state_b preserved)
test_create_branch
test_modifier_math:
  ai=0.707, depth=1.5 → final=0.471
  ai=0.707, depth=0.8 → final=0.884
```

---

## 8. Dependencies

```toml
# Add to lineos/m1/xaak/Cargo.toml:
arc-swap = "1"
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-10 | Initial spec |
| 1.1 | 2026-06-10 | AtomicCell, modifier math, render_node |
| 1.2 | 2026-06-10 | ArcSwap (not AtomicCell), RwLock AppState, &mut checkout |

---

**Status:** 🏗️ Ready for TDD
