# PROJECT MEMORY MAP

*How to use this doc: Before starting work on a "new" feature (especially data modeling, algorithms, or infrastructure), check this map first. This document catalogs things that ALREADY exist in the codebase but are easily forgotten because they aren't fully wired into the main production path yet, or are duplicated for migration reasons. Stop reinventing the wheel.*

---

## 1. Live vs Deprecated/Legacy Engines

The codebase is in the middle of a major architectural migration (Batch to Streaming).

*   **DEPRECATED (Legacy Batch Path):**
    *   `Sp314MasteringEngine` (in `lineos/m1/sp314-dsp/src/pipeline/engine.rs`). This is the old monolithic processor. It requires loading the entire audio file into memory.
    *   `DspAdapter::master()` (in `lineos/m0/m0-daemon/src/dsp/maestro.rs` or similar). This wraps the old engine for the daemon.
    *   *Usage:* These are still in the codebase, but they are considered **legacy**. They are primarily kept alive in older CLI tools (`test_engine`, `sp314_master`) and legacy tests.

*   **LIVE (Target Production Path):**
    *   `DspGraph` & `StreamingWavWriter` (in `lineos/m1/sp314-nodes/src/graph.rs` and `lineos/m1/sp314-dsp/src/io/stream.rs`).
    *   `run_streaming_pipeline_with_scout()` (in `lineos/m0/m0-daemon/src/dsp/streaming_pipeline.rs`).
    *   *Usage:* This is the true, memory-efficient, chunk-based streaming architecture (Phase 2). This is what the HTTP/Tauri endpoints in `m0-daemon` should be calling moving forward.

---

## 2. Duplicated DSP Primitives Across Crates

You will find multiple versions of the exact same DSP math (e.g., Biquads, Compressors) in different crates. **This is intentional.**

*   **Examples:**
    *   `Biquad` exists in `sp314-dsp/src/masking_eq/biquad.rs` AND `sp314-nodes/src/nodes/biquad.rs`.
    *   `Compressor` exists in `sp314-dsp/src/compressor/core.rs` AND `sp314-nodes/src/nodes/compressor.rs`.

*   **Why is it duplicated?**
    `sp314-dsp` is the legacy monolith. `sp314-nodes` is the new modular graph-based architecture. To avoid breaking the legacy tests and CLI tools while we build the new streaming graph, the math was isolated and re-implemented natively for the `DspNode` trait in `sp314-nodes`.
*   **Source of Truth:** If you are building a new feature or fixing a bug in the production mastering pipeline, the source of truth is **`sp314-nodes`**.

---

## 3. Data Models/Schema (Unwired but Ready)

The SurrealDB schema (`lineos/m0/m0-daemon/src/db/schema.rs`) is significantly ahead of the actual API endpoints. Several advanced concepts are fully modeled in the database but have **no production consumers** yet.

*   **Active (Wired):** `Project`, `Track`, `Session`, `Blob`.
*   **Inactive (Waiting to be used):**
    *   **The Git-like Versioning Model:** `MixCommit` and `Branch`. This is the exact schema needed for A/B/C/D mastering comparisons and version histories. It already has `parent_hash`, `branch_name`, and `head_hash`. Do not invent a new flat-table versioning system; use this.
    *   **Graph Relations:** `user_finding_feedback` and `user_flavour_preference`. These are defined as SurrealDB `RELATION` edges (Graph links) between Users and Commits/Projects. The schema supports graph traversals, even though core entities currently use flat foreign-key IDs.

---

## 4. CI/Test Visibility Gaps (Feature-Gated Blind Spots)

We have a recurring pattern where files hidden behind `#[cfg(feature = "...")]` do not compile or run during a standard `cargo check` or `cargo test`, leading to silently broken code.

*   **Known Blind Spots:**
    *   `lineos/m1/xaak/src/telemetry_worker.rs`
    *   `lineos/m1/sp314-dsp/tests/realtime_contract.rs`
    *   `lineos/m1/sp314-dsp/tests/io_contract.rs`
*   **The Problem:** These require the `cli` or `realtime` features to be activated. If a core API changes, these files will quietly break, and CI won't catch it unless the specific feature flag happens to be triggered.
*   **Actionable Advice:** If you change a core struct (like `hound::WavReader` usage or `Sp314MasteringEngine` signatures), explicitly run `cargo check --all-features` to ensure you didn't break these hidden files.

---

## 5. The 5-Layer Architecture Vision vs Reality

The `ARCHITECTURE.md` describes a grand 5-layer vision: Creator OS → LineOS → Aether → Apps → Marketplace, with the assumption of multiple independent apps running on the OS.

**The Reality Today:** We only have ONE application product built on this stack right now: **Stillair** (Tauri/Dioxus). Furthermore, Stillair is currently heavily coupled with the `m0-daemon` backend. 

*This is completely normal and expected at this stage of the startup.* We are building the engine while flying the plane. Do not over-engineer generic "App Store" plugin loaders right now when we only need to ship Stillair. The architecture provides the *seams* for future apps, but we do not need to flesh out the empty rooms yet.
