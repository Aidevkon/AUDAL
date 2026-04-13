# Creator OS — Core Invariants

**Version:** 1.1  
**Date:** 2026-03-15  
**Status:** 🔒 LOCKED  
**Location:** `creator-os/invariants/creator-os-invariants.md`  
**Authority:** OS-level — supersedes all module-level definitions

---

## Purpose

This document defines the non-negotiable invariants of the Creator OS.  
Every module — current and future — inherits and enforces these rules.  
If a module constitution conflicts with this document, this document wins.

To inherit: add `"inherits_from": ["creator-os-invariants"]` to the module definition or constitution.

---

## §01 — Processing Invariants

### 01.1 Deterministic Output
- Same input + same seed → **identical binary output**, always.
- No randomness in production pipelines unless explicitly seeded and documented.
- Determinism must be verified by automated tests in every module.
- Seeds must be **derived deterministically** from input data or provided explicitly by the user. Seeds derived from system entropy (e.g., `rand::thread_rng()`, `Date.now()`) are forbidden in production pipelines.

### 01.2 Local-First Processing
- All core functionality must run fully offline.
- No module may require a network connection for its primary use case.
- Cloud features are **additive and optional** — never a dependency for core behavior.

### 01.3 No Hidden State
- No hidden state may affect output without being documented in the module contract.
- All parameters that influence output must be explicit in the input contract.

---

## §02 — Privacy Invariants

### 02.1 Privacy by Architecture
- User data (audio, voice, scripts, assets) never leaves the device without **explicit user action**.
- Cloud sync is opt-in, never opt-out.
- No telemetry or analytics on user content.

### 02.2 No Raw Data Retention
- Temporary files generated during processing are deleted automatically when the session ends.
- **Session ends** is defined as: browser tab close, application quit, or explicit session reset by the user. Modules must handle all three cases.
- No module may persist raw user content beyond the active session without user consent.

### 02.3 No Training on User Data
- User data may not be used for model training or fine-tuning without **explicit, informed, opt-in consent**.
- This applies to all modules, all tiers, and all infrastructure providers (including Hetzner GPU).

---

## §03 — Architecture Invariants

### 03.1 Hexagonal Layering
- Dependencies flow **inward only**: UI → API → Engine → Models.
- Engines never import UI.
- No cross-layer imports (e.g., Export calling TTS directly).
- No circular dependencies.

### 03.2 Contract-Only Communication
- Modules communicate exclusively via JSON contracts defined in `creator-os/contracts/`.
- No module may call another module's internal functions directly.
- All contracts are versioned. Breaking changes require a new contract version (e.g., `audio-v2.json`).

### 03.3 No Engine-to-Engine Dependencies
- Engines (Still Air, VoiceForge, MotionCraft) must not depend on each other.
- Cross-engine workflows are orchestrated via contracts and the API layer only.
- Shared cores (audio-core, ml-core, ui-core) are part of the Engine layer and may be used by engines. Shared cores themselves must not import any engine or module.

### 03.4 Separation of Concerns
- Engine, API, and UI are strictly separated in every module.
- Engine never calls the UI.
- UI communicates via event bus (preferred) or polling — never direct engine access. Event bus is preferred for responsiveness; polling is acceptable for simple status checks.
- `user_data/` is accessed exclusively through the API layer.

---

## §04 — Quality Invariants

### 04.1 No Silent Failures
- Every error must be caught at the boundary where it occurs.
- Every error must be logged with structured context.
- Every error must be surfaced to the user with actionable guidance.
- Silent `let _ = result` patterns are forbidden in all modules.

### 04.2 No Silent Fallbacks
- Fallback behavior that changes output must be visible to the user.
- No module may silently degrade without notifying the user.

### 04.3 No Lossy Intermediate Formats
- Internal processing pipelines must use lossless formats: WAV, FLAC, or AIFF.
- Lossy encoding (MP3, OGG, etc.) occurs only at **final export**, never as an intermediate step.
- Temporary lossless files created for performance reasons (e.g., within an engine) are permitted but must be deleted after use — they are subject to §02.2.

---

## §05 — License Invariants

### 05.1 License Purity
- All code shipped to users must be licensed under **MIT or Apache 2.0 only**.
- GPL-licensed crates are **forbidden** in any module that ships to users.
- All third-party dependencies must be audited for license compatibility before inclusion.
- Any crate with a non-MIT/Apache 2.0 license requires **OS-level approval** before it may be added. Approval must be documented in the module's constitution or amendment.

### 05.2 No Vendor Lock-In
- ML models must use open formats (ONNX or equivalent) and be replaceable.
- No proprietary SDK may be a hard dependency for core functionality.
- Third-party marketplace content must be signed and license-checked before installation.

### 05.3 No Code Execution from Packs
- Marketplace packs (voice packs, DSP extensions, etc.) contain only data and model files.
- No executable code may be distributed via the Marketplace.

---

## §06 — Compatibility Invariants

### 06.1 Backward Compatibility
- All schemas, contracts, and public APIs evolve **additively**.
- Fields may be added; existing fields may not be removed or renamed without a version bump.
- Modules must remain compatible with the previous MAJOR version of shared cores.

### 06.2 Modular Independence
- Every module must function as a **standalone product**.
- Creator Cloud is optional — all modules run fully offline.
- Creator Studio is optional — all modules expose their own UI.
- Marketplace is optional — no module depends on it for core functionality.

---

## §07 — Testing Invariants

### 07.1 Determinism Tests
- Every module must include automated tests that verify deterministic output.
- Same input + same seed must produce bit-identical output across runs and platforms.
- Determinism tests must be run on **at least two architectures** (e.g., x86_64 and ARM) to verify cross-platform determinism. Single-platform passing is insufficient.

### 07.2 Contract Compliance Tests
- Every module must include integration tests that verify contract compliance with at least one other module or a test harness simulating the counterpart.

### 07.3 CI Enforcement
- The Creator OS CI pipeline must run all invariant and contract compliance tests on every commit.
- A failing invariant test blocks merge — no exceptions.

---

## §08 — Forbidden Work (System-Wide)

The following are forbidden across all modules without exception:

- ❌ Module-to-module direct imports
- ❌ Circular dependencies
- ❌ Nondeterministic engines in production
- ❌ Server-side retention of user audio, voice, or script data
- ❌ Cloud dependency for core features
- ❌ GPL crates in shipping code
- ❌ Silent failures or silent fallbacks
- ❌ Lossy intermediate formats in processing pipelines
- ❌ Code execution from Marketplace packs
- ❌ Training on user data without explicit consent
- ❌ Hidden state that affects determinism
- ❌ Breaking changes to contracts without a new version
- ❌ Redesign of shared cores without OS-level approval

---

## §09 — How to Inherit These Invariants

### In a module JSON definition:
```json
{
  "dependencies": {
    "inherits_from": ["creator-os-invariants"]
  }
}
```

### In a module constitution (markdown):
```
This module inherits and enforces the Creator OS Core Invariants
defined in creator-os/invariants/creator-os-invariants.md.
If this constitution conflicts with the invariants, the invariants win.
```

### In CI:
- Reference `creator-os-invariants.md` as the authority document in the CI invariant test suite.
- Each module's CI step must include a lint/test pass that verifies invariant compliance.
- CI must include a step that verifies the module explicitly declares inheritance — either via the `"inherits_from"` JSON field or the inheritance statement in the module constitution.

### Inheritance declaration is mandatory:
- Every module must document its inheritance in its `README.md` or constitution.
- Modules that do not declare inheritance are considered non-compliant and will be blocked by CI.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.1 | 2026-03-15 | Seed determinism clarification (§01.1), session end definition (§02.2), shared cores layering note (§03.3), event bus preference (§03.4), lossless format clarification (§04.3), crate audit process (§05.1), cross-platform determinism tests (§07.1), inheritance declaration requirement (§09) |
| 1.0 | 2026-03-15 | Initial extraction from Still Air v6.4, VoiceForge v2.2, and Creator OS Architecture v1.1 |

---

**Lead Architect:** Anestis  
**System:** Creator OS  
**Document:** `creator-os-invariants.md`  
**Version:** 1.1  
**Date:** 2026-03-15  
**Status:** 🔒 LOCKED

---

*This document is the single source of truth for Creator OS invariants.*  
*Every module that ships under the Creator OS name enforces these rules.*  
*No exceptions. No workarounds. No silent deviations.*
