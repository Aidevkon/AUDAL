# Draft Amendment — Tech Stack Reality Check & Open Questions
**Status:** 🟡 DRAFT — pending Lead Architect approval
**Date:** 2026-06-26
**Against:** Creator OS Constitution v2.6

## Confirmed gaps between §08 (Technology Stack) and actual implementation

Our systematic codebase audit confirmed the following divergences between the constitution and the live repository:

1. **Frontend Framework:** 
   * **Reality:** The UI is built entirely with `Dioxus` (confirmed via `apps/stillair/cockpit-dioxus`).
   * **Search Result:** `Leptos` is completely absent from all `Cargo.toml` files in the workspace (0 results).
   * **Classification:** **Conscious Deviation.** Dioxus was explicitly chosen and confirmed tonight as the intended framework for the Cockpit UI.

2. **Database / Persistence:**
   * **Reality:** `SurrealDB` is the sole database engine used for state persistence (embedded KV/Mem).
   * **Search Result:** `postgres` and `sqlx` are completely absent from all workspace `Cargo.toml` files (0 results).
   * **Classification:** **Conscious Deviation.** SurrealDB was explicitly chosen over Postgres to serve the local-first architecture.

3. **Audio File IO:**
   * **Reality:** The `hound` crate is actively used for audio IO (e.g., `StreamingWavWriter` integrated and benchmarked tonight).
   * **Classification:** **Conscious Deviation.** It is actively powering the new zero-copy streaming pipeline effectively.

4. **Cloud / Backend Tier (Elixir/Phoenix):**
   * **Reality:** Zero `.ex` or `.exs` files exist in the repository. The `/backend` directory does not exist.
   * **Classification:** **Not-yet-built, scoped future need.** This is not a mistake or deprecated code. The Elixir backend is entirely unbuilt because it is intentionally deferred.

## Προτεινόμενη διατύπωση για το §07.1/§07.2 (Elixir/Phoenix)

Elixir/Phoenix is reserved exclusively for a future, distinct cloud-hosted tier — specifically API gateway + WebRTC signaling for paying users who run cloud-hosted sessions (BEAM's concurrency model is well-suited to coordinating many lightweight, low-latency peer connections). It is NOT required for, and must never become a dependency of, the core local-first product. No backend/ code exists yet — this is intentional, deferred until real cloud-tier demand exists, not a gap to fill prematurely.

## Open Questions for Next Cycle

1. **IPC Bottlenecks** — RESEARCH QUESTION, recon-first sequence (measure actual Tauri IPC mechanism + RTT before any solution design).
2. **SurrealDB graph expansion** — ποιο από τα 8 unwired schema structs (βλ. PROJECT_MEMORY_MAP.md §3) εκτός από το MixCommit αξίζει να γίνει wired επόμενο, και αν τα core entities (Project/Track) αξίζει να μετατραπούν σε πραγματικά graph relations. (Το MixCommit αποφασίστηκε παρακάτω).

## Decisions Made Tonight

1. **Dioxus Motion** — DECIDED, no library needed now. Progressive enhancement:
   * **Layer 1** (CSS transitions, GPU-accelerated, for buttons/hovers/fades).
   * **Layer 2** (Canvas API/requestAnimationFrame, already exists in `neon_canvas.rs`, for audio meters/FFT/waveforms — no UI library is fast enough for this anyway).
   * **Layer 3** (lightweight spring-physics crate, ONLY if genuinely needed for complex chained animations, evaluated later against real pain, not now).
2. **MixCommit/Branch wiring** — DECIDED, next concrete implementation step (separate from this doc — see streaming/5.1 Epic threads).
