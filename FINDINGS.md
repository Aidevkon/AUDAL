# FINDINGS.md — Creator OS Technical Findings Registry

Durable record of technical findings discovered during development —
architectural gaps, deferred decisions, and known-but-not-urgent issues.
Lives in the repo, not in chat memory, so it survives across sessions,
compaction, and tool switches.

**How to use this file:**
- Before starting new work in an area, grep this file for the component name.
- When you close a finding, move it to the RESOLVED section with the commit hash.
- When you discover something new mid-task and decide *not* to fix it now,
  add it here immediately — don't let it live only in a chat transcript.
- `Trigger` = the condition that should make you revisit this, not "someday."

Format per entry: ID, Status, Component, Trigger, one-paragraph context.

---

## ACTIVE / PARKED

### F-001 — Aether Black pipeline lacks unified documentation
- **Status:** PARKED (large, needs own session)
- **Component:** `aether/markov/*`, `aether/chaos/*`, `spec/locked/S-009*`
- **Trigger:** Before onboarding anyone else to this codebase, or before
  modifying any Markov/Chaos/Firewall code.
- **Context:** Confirmed real, complete, active pipeline (Markov classifiers
  → PredictiveController → ChaosLayer → IntegrationFirewall →
  build_dsp_config), officially Stage 4 in `track_lifecycle.md`, referenced
  in old commit messages (M1→M8 COMPLETE) but never tied to that name in
  any doc a new reader would find. S-009's drift banner (added this
  session) flags the connection but doesn't document the system itself.

### F-002 — Biquad design duplicated across 5 independent implementations
- **Status:** PARKED (needs its own recon before touching)
- **Component:** `nodes/biquad.rs`, `masking_eq/biquad.rs`,
  `restoration/biquad.rs`, `compressor/crossover.rs` (inline Linkwitz-Riley),
  `analysis/pre_analysis.rs` (inline Butterworth)
- **Trigger:** If a real multi-band EQ or new filter-design feature gets
  built (not the 64-band FFT spectrum analyzer — that's unrelated, already
  confirmed via doc grep). Only then is it worth confirming which of the
  5 are true duplicates vs. legitimately different designs for different
  jobs (crossover filters guarantee phase-coherent L+R summing; a generic
  EQ bell doesn't need that property and shouldn't share math with it).
- **Context:** Do not collapse into one shared type without first
  confirming per-implementation which properties each depends on.

### F-003 — rms.rs reimplements EnvelopeFollower math by hand
- **Status:** PARKED (needs verified-equivalence pass, not a find-replace)
- **Component:** `nodes/rms.rs`
- **Trigger:** Next time rms.rs's attack/release behavior needs to change,
  or if a bug is suspected in its envelope tracking.
- **Context:** `sp314_dsp::compressor::envelope::EnvelopeFollower` already
  exists and is shared (CompressorNode uses it). rms.rs independently
  reimplements the same attack/release coefficient math instead of
  adopting it. Swapping requires confirming bit/behavior equivalence first
  — this is live, tested DSP code, not dead code.

### F-004 — DspGraph::clone() still fully rebuilds every node + re-runs topo sort
- **Status:** PARKED (real fix needs DspNode-level clone support)
- **Component:** `sp314-nodes/src/graph.rs`, `sp314-nodes/src/node.rs`
- **Trigger:** If profiling ever shows DspGraph::clone() as a hot spot
  beyond the crossfade path already fixed this session.
- **Context:** Arc<DspTopology> (shipped this session) made the topology
  *data* cheap to share, but Clone still reconstructs all ~15 nodes and
  re-runs Kahn's algorithm every time. A genuinely cheap clone needs each
  DspNode to support its own clone — not attempted, since nodes carry live
  per-instance mutable state (filter coefficients, glider position) that
  can't be Arc-shared between two simultaneously active graph copies.

### F-005 — No mechanism to forward live UI parameter changes during an active crossfade
- **Status:** PARKED (real feature, not a bug)
- **Component:** `apps/runtime/loom/src/engine.rs`
- **Trigger:** If users report "my knob change didn't apply" during
  playback near a section boundary.
- **Context:** Discovered while fixing the crossfade dummy-clone bug.
  Parameter changes sent during an active crossfade are silently no-op'd
  (matches pre-existing behavior — previously applied to a soon-discarded
  dummy clone and lost; now explicit and documented instead of accidental).
  Real fix would need the crossfader to accept param updates on both of
  its two active graphs — new API surface, not attempted.

### F-006 — ~10 pre-existing JsValue::from_str() calls in engine.rs are unguarded against native-test panics
- **Status:** PARKED (known landmine, not yet triggered)
- **Component:** `apps/runtime/loom/src/engine.rs` (constructor,
  `load_stems`, `load_time_aware_behaviour`)
- **Trigger:** Any new test that exercises the *failure* path of these
  functions under native `cargo test` (not wasm32).
- **Context:** `JsValue::from_str()` panics unconditionally on non-wasm32
  targets (wasm-bindgen's own stub). Found and fixed for two *new* call
  sites this session via a `js_error()` cfg-gated helper — the same
  landmine is still live at the ~10 pre-existing sites, just never
  exercised because no existing test hits their error paths yet.

### F-007 — API response format is inconsistent across handlers
- **Status:** PARKED (needs explicit architecture decision, not silent fix)
- **Component:** `m0-daemon/src/handlers/*`
- **Trigger:** Before adding new handlers, or before any frontend work that
  needs to handle errors consistently.
- **Context:** Most handlers (scout, export, master) always return HTTP 200
  + `{status:"error"}` JSON. `preview.rs`, `pdf_gen.rs`, `blob.rs` return
  real 404/400 status codes. Neither is wrong in isolation, but the
  inconsistency is real. Standardizing changes an existing, presumably
  already-consumed API surface — needs an explicit decision, not an
  automatic sweep.

### F-008 — 28 RwLock/Mutex .unwrap() calls on lock guards (poison-only risk)
- **Status:** NOT A BUG — documented deliberately, no action planned
- **Component:** `m0-daemon/src/handlers/mix.rs`, `tinder.rs`, `preview.rs`,
  `dev_snapshot.rs`
- **Trigger:** None expected. Only revisit if a deliberate different
  poison-recovery policy is wanted project-wide.
- **Context:** These only panic if another thread already panicked holding
  the same lock (poisoned state) — existing, correct Rust practice
  (fail loud rather than silently continue on possibly-corrupted shared
  state). Recorded here so it's not mistaken for an open risk later.

### F-009 — sparse_scout: mono-channel handling reviewed but untested; ITU-R BS.1770-4 non-compliance in 5.1 documented but not fixed
- **Status:** PARKED
- **Component:** `m0-daemon/src/dsp/sparse_scout.rs`
- **Trigger:** If mono input handling or 5.1 loudness gating produces a
  user-visible bug report.
- **Context:** Doc-comment added (§11.6) noting the spec gap; no test
  coverage added for the mono path.

### F-010 — STFT/OlaBuffer unification opportunity
- **Status:** PARKED
- **Component:** `sp314-dsp/src/stft/two_pass.rs` (tight-coupled OLA logic)
  vs. the standalone `sp314-dsp/src/ola_buffer.rs` built this session
- **Trigger:** Next time two_pass.rs's OLA logic needs modification.
- **Context:** Not urgent; two_pass.rs's OLA predates the shared
  OlaBuffer primitive and works correctly as-is.

### F-011 — Go Distributor (vision doc) vs. Rust Conductor/Executor (already built) overlap undefined
- **Status:** PARKED — deliberate position: stay pure Rust
- **Component:** vision/roadmap docs vs. `m0-daemon/src/agents/{conductor,executor,operator}.rs`
- **Trigger:** Only if a real, measured scaling problem appears that the
  existing Rust Conductor/Executor genuinely can't handle (e.g. hundreds
  of parallel mastering jobs across multiple machines).
- **Context:** The vision doc's "Go Distributor" predates knowledge that
  Conductor/Executor already exist and work. No Go code exists in the
  repo today. Adding a second language/runtime for a problem that hasn't
  materialized is the single highest-cost mistake available to a solo
  developer — costs double context-switching forever, for a benefit that
  is currently zero.

### F-012 — sp314-dsp API surface not narrowed to facades
- **Status:** PARKED — deliberate non-action, not an oversight
- **Component:** `sp314-dsp/src/lib.rs` (~20 public modules)
- **Trigger:** Only if sp314-dsp is ever published as an independent crate
  consumed by unknown third parties.
- **Context:** Only 2 internal crates (m0-daemon, sp314-nodes) consume this
  API today, both maintained by the same person in the same repo. The
  "protects consumers from breaking changes" argument only has force
  against consumers you don't control — there are none here. Would cost
  ~31 file changes for zero functional benefit today.

### F-013 — LookaheadRing has no type-level Observer/Consumer distinction
- **Status:** PARKED — deliberate non-action, premature abstraction
- **Component:** `sp314-dsp/src/lookahead_ring.rs`
- **Trigger:** When a second, real Consumer node (e.g. an actual lookahead
  delay/limiter node) is built.
- **Context:** Zero production callers of `consume_into()` exist today —
  the only current user (LookaheadTelemetryNode) is observer-only and
  already documented as such. Designing a typestate split before a real
  Consumer exists risks guessing its API wrong (e.g. if it needs dynamic
  Observer→Consumer transitions, a static typestate pattern would need
  rework anyway).

### F-014 — e2e_tier1_abort.rs's real-MP3 test has no committed MP3 fixture
- **Status:** PARKED
- **Component:** `m0-daemon/tests/e2e_tier1_abort.rs`
  (test_run_dsp_passes_real_mp3_podcast)
- **Trigger:** If symphonia's MP3 decode path needs verified test
  coverage beyond what synthetic WAV fixtures can prove.
- **Context:** Test already has a correct `exists()` guard and skips
  safely everywhere except the original author's machine — not
  broken, just never actually exercised in CI. A synthetic MP3 could
  be generated via the already-present lame-sys/lame encoder in this
  workspace, but that's new fixture-generation work, not a CI fix.

### F-015 — BPM is hardcoded to 0.0 throughout the pipeline, silently falls back to 120 BPM in two separate places
- **Status:** PARKED (real gap, not urgent — cosmetic UI effect today,
  becomes a blocker for genre classification / BPM-aware ducking)
- **Component:** `sp314-dsp/src/analysis/pre_analysis.rs` (bpm: 0.0 hardcoded),
  `apps/stillair/cockpit-dioxus/src/components/neon_canvas.rs`
  (FALLBACK_PULSE_MS=500ms silently produces 120 BPM math), 
  `apps/stillair/src-tauri/src/commands/session.rs` (hardcoded "120"
  string in Jini prompt template)
- **Trigger:** When genre classification or BPM-aware DSP features
  (maestro ducking already reads bpm as an input per `e2e_maestro_proof.rs` tests, but always via mocked test values, never real
  detection) need an actual measured value instead of a placeholder.
- **Context:** No autocorrelation/onset-detection algorithm exists
  anywhere in the codebase. The Hero Instrument UI's floor pulse
  has always animated at exactly 120 BPM regardless of the actual
  track, because 0.0 (never-computed) falls through a fallback that
  happens to equal 120 BPM by coincidence of the chosen constant
  (60000ms / 500ms = 120). Found while investigating whether BPM
  detection was ready enough to include in a first genre classifier
  pass — it is not; real BPM detection is its own separate task.

### F-016 — TrackFeatures struct is MFCC-only by design, not yet extended for future features (spectral centroid, onset rate, BPM)
- **Status:** PARKED — deliberate, avoid schema guessing
- **Trigger:** When onset detection or BPM detection is actually built
  (separate task each), extend the struct then, with the real
  shape those algorithms produce — not before.
- **Context:** Considered adding placeholder 0.0 fields now "to save
  future refactoring", rejected — same premature-abstraction pattern
  already avoided today for API facades, typestate pattern, and
  biquad unification. Sentinel 0.0 values for "not yet measured"
  also violate the project's own §8 rule (`Option<T>`/explicit `Err`,
  never numeric sentinels in forensic/DSP context).

### F-017 — Genre corpus versioning/reproducibility design (pre-decided, not yet built)
- **Status:** PARKED (design decided, no code yet — genre classifier
  itself doesn't exist yet either, this is the plan for when it does)
- **Component:** μελλοντικό `genre_classifier.rs` + certificate schema
- **Trigger:** Όταν χτιστεί ο πρώτος πραγματικός genre classifier
  (μετά τη συλλογή reference tracks + πρώτο measure_genre_centroids run)
- **Context:** Ο χρήστης ρώτησε ρητά "πώς κάνει κάποιος remaster με
  reproducibility αν εμείς έχουμε αλλάξει version corpus;". Λύση,
  ίδιο μοτίβο με Cargo.lock: το certificate κλειδώνει ρητά ποιο
  corpus version (π.χ. "genre-corpus-v3", με hash/tag) χρησιμοποιήθηκε
  στο πρώτο mastering. Remaster δίνει ρητή επιλογή στον χρήστη:
  "identical" (ίδιο locked version, true reproducibility) ή "latest"
  (νέο corpus version, ρητά σημειωμένο ως αλλαγή). Καμία σιωπηλή
  version drift ποτέ.

### F-018 — measure_genre_centroids.rs has no sanity check for track duration
- **Status:** PARKED (low risk, manual curation should catch this)
- **Component:** `m0-daemon/tests/measure_genre_centroids.rs`
- **Trigger:** Αν κατά λάθος μπει πολύ μεγάλο αρχείο (π.χ. ολόκληρο CD
  rip αντί για ένα track) στο `genre_references/` folder.
- **Context:** Δεν υπάρχει έλεγχος duration/file-size πριν το processing.
  Discovered while automating reference-track download attempts —
  some archive.org "tracks" were actually full album rips. Manual
  curation (choosing individual, correctly-labeled tracks) should
  avoid this in practice, but the tool itself doesn't defend against it.

### F-022 — GenreClassifier implemented, wiring pending
- **Status:** ACTIVE
- **Component:** `lineos/m1/sp314-dsp/src/analysis/genre_classifier.rs`
- **Trigger:** Όταν ξεκινήσει η ενσωμάτωση στο `pre_analysis.rs`.
- **Context:** Ο αλγόριθμος (Z-Scored Euclidean) και τα thresholds (`MAX=4.0`, `DELTA=0.15`) έχουν υλοποιηθεί βάσει μετρήσεων στο καθαρό corpus, αλλά δεν καλούνται ακόμα στο runtime του m0-daemon pipeline.

---

## RESOLVED THIS SESSION (for traceability — see git log for full detail)

| ID | One-line summary | Commit |
|----|----|----|
| R-001 | Ghost parameter `phase_variance` sent to Width node that never had it (pipelineforge) | `0c33bd8` |
| R-002 | Certificate LUFS was pre-gain, not actual rendered value (false attestation) | `7d66d86` |
| R-003 | NaN-unsafe `partial_cmp` in tinder.rs A/B/C/D matcher | `05ad8b3` |
| R-004 | Hot-path dummy clone in crossfade + cheap Arc<DspTopology> sharing | `7142596` |
| R-005 | Config scattered across 6+ files, inconsistent M0_/CREATOR_OS_ env prefixes | `c70eff2` |
| R-006 | ParameterGlider lived in wrong crate (sp314-nodes instead of sp314-dsp) | `2d6748e` |
| R-007 | GitHub CI never actually verified — 3 missing Linux/macOS system deps, clippy flags drifted from Justfile, Gate 5 OOM on shared runners | `775ccd4`, `cdd7a2f`, + 2 more |
| R-008 | Item 10 (cross-platform float determinism) — verified non-issue via real ARM64 CI run, not theory | (same CI commits) |
| R-009 | Two e2e tests (e2e_agent_pipeline.rs, e2e_album_sse.rs) used hardcoded personal-machine absolute paths with no portability guard — one masked by a race condition that made it appear to pass in earlier local runs | `1009def` |
| F-023 | S-002 stem-count drift (4→5 stems) fixed to match real FiveStems code | `pending commit` |

---

## Adding a new entry

```
### F-0XX — [short title]
- **Status:** PARKED / ACTIVE
- **Component:** [file/crate paths]
- **Trigger:** [specific condition, not "someday"]
- **Context:** [what you found, why it's not urgent, what would make it urgent]
```
