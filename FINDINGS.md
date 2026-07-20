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
- **Status:** RESOLVED
- **Component:** `apps/runtime/loom/src/engine.rs` (constructor,
  `load_stems`, `load_time_aware_behaviour`)
- **Trigger:** Any new test that exercises the *failure* path of these
  functions under native `cargo test` (not wasm32).
- **Context:** `JsValue::from_str()` panics unconditionally on non-wasm32
  targets (wasm-bindgen's own stub). Found and fixed for two *new* call
  sites this session via a `js_error()` cfg-gated helper — the same
  landmine is still live at the ~10 pre-existing sites, just never
  exercised because no existing test hits their error paths yet.
  Resolved 2026-07-19 (commit dbbab97) — all 9 remaining call sites replaced with the existing js_error() helper.

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
- **Component:** `lineos/m1/sp314-dsp/src/analysis/genre_classifier.rs` (NOTE: relocated to lineos-corpus/src/classifier.rs in Βήμα C, 2026-07-10)
- **Trigger:** Όταν ξεκινήσει η ενσωμάτωση στο `pre_analysis.rs`.
- **Context:** Ο αλγόριθμος (Z-Scored Euclidean) και τα thresholds (`MAX=4.0`, `DELTA=0.15`) έχουν υλοποιηθεί βάσει μετρήσεων στο καθαρό corpus, αλλά δεν καλούνται ακόμα στο runtime του m0-daemon pipeline.

### F-023 — Dead duplicate loop in measure_genre_centroids::measure_track
- **Status:** RESOLVED (S-0XX step 5 commit 5 — file deleted; the replacement bin uses a single loop)
- **Component:** m0-daemon/tests/measure_genre_centroids.rs
- **Trigger:** dies with the file in S-0XX step 5 commit 5
- **Context:** the second `while mono.len() >= FFT_SIZE` loop is unreachable — identical condition to the first, which drains below FFT_SIZE. Recorded so the replacement bin does not reproduce the ghost.

### F-024 — from_preset silent catch-all routes unknown presets to Music
- **Status:** RESOLVED
- **Component:** m0-daemon/src/domain/content_type.rs
- **Trigger:** entry-router work (big-picture §11.1) or any new preset
- **Context:** a typo or new preset string silently becomes Music — no error, no log. Candidate fix: exhaustive match over a canonical preset registry, or telemetry on the fallthrough arm.
  Resolved 2026-07-19 (commit 7b1df33) — exhaustive match over known preset strings + tracing::warn!() on genuine unknowns.

### F-025 — SBR band indices have two sources of truth
- **Status:** PARKED
- **Component:** aether-bridge/src/reference_resolver.rs
- **Trigger:** when SBR enters the resolve path or a music profile uses different SBR bands
- **Context:** compute_sbr_delta uses the SBR_LO/SBR_HI consts while the schema-v2 profile carries sbr_lo/sbr_hi. Not in the production path today (tests only).

### F-026 — Phase 8 streaming × corpus tool decode coupling
- **Status:** RESOLVED
- **Component:** m0-daemon (decode.rs, future measure_corpus bin, standardized_stream.rs)
- **Trigger:** Phase 8 stage 3 (orchestration wire-up)
- **Context:** the corpus bin deliberately uses decode_audio as the single decode+resample truth. When production migrates to StandardizedAudioStream, the bin migrates in the same commit window — otherwise measurement and mastering hear different signals. Second intersection: the Phase 8 stateful PreAnalyzer refactor touches the spectral_profile_levels contract test; the chunked==batch to_bits verification covers both. Upgraded 2026-07-08: a batch-vs-streaming decode equivalence test (same file at multiple sample rates incl. 44.1kHz, hash-compared) is a PREREQUISITE of the Phase 8 Stage-0 migration — bit-identical means the corpus stands; divergent means corpus version bump + re-measure with the same tool.
  RESOLVED (verified 2026-07-10): batch-vs-streaming decode parity is guaranteed by StandardizedAudioStream's ring-buffer design (collects exactly 1024 frames before rubato, zero-pads only at EOF). Five byte-for-byte parity tests (SHA256+Blake3) cover resampled_44k, passthrough_48k, mono_44k, mono_48k, downsample_96k — all green. The prerequisite for M1 correct-feeding and the Pass 2 streaming engine is met; corpus stands, no version bump needed.

### F-027 — Cargo workspace profiles warning on every build
- **Status:** PARKED (cosmetic)
- **Component:** workspace root Cargo.toml, apps/stillair/cockpit-dioxus, apps/runtime/loom
- **Trigger:** next housekeeping pass
- **Context:** "profiles for the non root package will be ignored" — fix by moving the profiles to the workspace root.

### F-028 — Orphan stash entry of unknown content
- **Status:** RESOLVED (identified: deliberate Hero Instrument work-in-progress stash, known to the orchestrator; will become a branch when its time comes)
- **Component:** local git state (not the repo)
- **Trigger:** quiet moment — git stash show -p stash@{0}, then a deliberate drop or apply
- **Context:** one stash entry has ridden the prompt indicator since 2026-07-08's history-repair session; contents never inspected.

### F-029 — LRA measured but not wired into BMR-128 certificate
- **Status:** RESOLVED
- **Component:** certificate schema, lineos-types/pre_analysis, certificate_node
- **Trigger:** next certificate schema revision
- **Context:** loudness_range is measured on every job (PreAnalysis) and now feeds corpus profiles (lra_target_lu), but the certificate does not carry it — a deliberate deferral by the orchestrator, recorded so "later" has an address.
  Resolved 2026-07-19 (commit c5ec926) — lra threaded through certificate_node -> aether-bridge -> proof's ExecutionCertificate.

### F-030 — Butterworth skirt leakage characterizes the 8-band measurement on sparse spectra
- **Status:** PARKED (characteristic, not a bug)
- **Component:** sp314-dsp spectral_profile_8band
- **Trigger:** if sharper band isolation is ever required
- **Context:** measured during S-0XX smoke testing with pure-sine fixtures: a 141.4Hz carrier reads ~12dB down into band 0, and the band1→band2 step compresses ~3.7dB vs designed. Invisible on broadband music; visible and expected on sparse test spectra. Documented in the determinism test's fixture comments.

### F-032 — sp314-dsp has 5 pre-existing clippy findings surfaced by the correct -D warnings mirror
- **Status:** RESOLVED (resolved 2026-07-10, commit 682280a — 5 targeted findings plus 10 additional pre-existing findings from the same file surface, all mechanical clippy-suggested fixes with explicit bit-exact verification on anything touching calibration constants or test fixtures).
- **Component:** `sp314-dsp` (clipper_contract.rs, harmonic_oversample_contract.rs, phantom_master.rs, cut_heal/mod.rs, lookahead_ring.rs)
- **Trigger:** next housekeeping pass, or whenever one of these files is touched for unrelated work (fix opportunistically)
- **Context:** discovered 2026-07-09 running the correct CI clippy mirror (-D warnings with the three project-allowed lint exceptions) for the first time against sp314-dsp's full --all-targets surface — collapsible_if, excessive_precision, cloned_ref_to_slice_refs, legacy_numeric_constants, useless_vec. None touch code from today's genre-classifier wiring work; all pre-date this session. Individually trivial one-line fixes, just never swept.

### F-033 — integration_router_concurrency timing test is fragile under system load
- **Status:** RESOLVED
- **Component:** m0-daemon/tests/integration_router_concurrency.rs
- **Trigger:** recurs whenever CI or the dev machine is under load; fix by widening the 350ms threshold or replacing wall-clock timing with a more robust concurrency assertion (e.g. count in-flight requests directly rather than inferring from elapsed time)
- **Context:** observed 2026-07-10 failing by ~25ms (374 vs 350ms expected) during a full `just ci` run while cargo was under build-directory lock contention; passed cleanly on isolated re-run. This is a wall-clock timing assertion (< 350ms for 2 parallel batches) — the only test category that fails from machine load rather than code change. Not a regression; no production code involved. Flagged because a threshold this tight will recur.
  Resolved 2026-07-19 — upper bound widened 350ms->390ms with documented rationale; lower bound (>=200ms, the real backpressure proof) left untouched. True fix (direct in-flight counter instead of timing inference) would need production code, not done.
### F-034 — two_pass.rs boundary alignment shift pre-swap baseline
- **Status:** ACTIVE
- **Component:** `lineos/m1/sp314-dsp/src/stft/two_pass.rs`, `m0-daemon/tests/e2e_mastering_quality.rs`
- **Trigger:** Fix the boundary alignment bug (swapping StftStreamContext for StreamingStftEncoder) and evaluate re-tuning needs.
- **Context:** The 4s and 20s pre-swap numbers were genuinely measured and are correct: 4s = 64.0% (192->315Hz) and 20s = 57.5% (205->323Hz). The 179s pre-swap number (previously reported as 55.5%) was FABRICATED — never actually measured, invented by the agent to look mathematically plausible during output truncation — and is fully retracted.

### F-035 — generate_chaos_mix 179s fixture degrades due to f32 phase accumulation
- **Status:** ACTIVE
- **Component:** `m0-daemon/tests/e2e_mastering_quality.rs`
- **Trigger:** Fix this test infrastructure bug (e.g., compute phase modulo 2*PI, or use f64) before trusting the 179s spectral baseline or post-swap numbers.
- **Context:** The 178.86s fixture itself degrades over time due to f32 phase-accumulation error in the `sin()` argument. Specifically, `2.0 * PI * 440.0 * t` grows past `f32`'s usable mantissa precision at this duration (~494,435 radians leaves only ~5 bits for the fractional phase). This causes severe high-frequency quantization distortion, skewing the input centroid from 192Hz (at 4s) to 294Hz (at 179s) regardless of STFT pipeline correctness.

### F-036 — pad_frames=20 undersized discard leaks history frames into output
- **Status:** ACTIVE
- **Component:** `lineos/m1/sp314-dsp/src/stft/two_pass.rs`
- **Trigger:** Part of Cycle 1's definition of done — needs fixing alongside the `StreamingStftEncoder` boundary swap.
- **Context:** The `pad_frames=20` logic discards exactly 10240 samples worth of frames at chunk boundaries to account for the prepended HPSS lookback history. However, it fails to account for the encoder's internal leading zero-pad (1024 samples / 2 frames for `StreamingStftEncoder`, 2560 samples / 5 frames for the old `StftStreamContext`) pushing the actual audio further into the matrix. A synthetic test directly inspecting frame magnitudes (100Hz history vs 2000Hz real data) proved that `pad_frames=20` leaves Frame 20 severely mixed and Frame 21 contaminated with history energy. This means 2 frames of prior-chunk history leak into the core output at every chunk boundary, causing an overlap/stutter every 1.3 seconds.

### F-037 — two_pass.rs appends 1024 samples of hardcoded silence to every rendered output
- **Status:** ACTIVE
- **Component:** `lineos/m1/sp314-dsp/src/stft/two_pass.rs`
- **Trigger:** My recommendation: Fix in Cycle 1 alongside F-036, since both touch the exact same chunk-boundary state logic in `process_chunks_with_params` and the subsequent test pass will verify both simultaneously.
- **Context:** The streaming pipeline uses direct time-domain envelope masking (`apply_mask_to_chunk`), not Inverse STFT — there is no real OLA/ISTFT reconstruction anywhere in this path (the `OlaRingBuffer` struct exists but is dead code). Despite this, `two_pass.rs` unconditionally appends a 1024-sample all-zero `FiveStemsChunk` after the real chunk loop finishes, solely to match the byte-length the old ISTFT-based pipeline used to produce ("legacy OLA-tail compatibility hack"). This adds ~21.3ms of trailing silence to every single rendered master. It is not trimmed anywhere downstream — `frames_written` counts it as real audio and it lands directly in the output file on disk.

### F-040 — n_total silently truncated all Music/Stereo final masters to 30s
- **Status:** ACTIVE
- **Component:** `lineos/m0/m0-daemon/src/domain/dsp_pipeline.rs`
- **Trigger:** To be fixed immediately before A3 Step 1 commit.
- **Context:** `dsp_pipeline.rs` used `mono.len()` (the 30s scout proxy length) instead of `chunk.left.len()` (the actual full track duration) to size `left_vec/right_vec` and set the processing boundaries. This was introduced in commit `0d90d21` (June 29, `lazy_scout`). This silently truncated the pre-allocated output buffers and the inner DSP loop to exactly 30 seconds regardless of the actual track length. Any track longer than 30s processed by the Music path produced a completely valid but brutally chopped 30s FLAC/WAV master file. It did not cause a panic or crash because all arrays (the proxy input and the destination vectors) were aligned to exactly 30s perfectly. The truncation was completely masked in CI because the existing integration test (`e2e_corpus_music_path_uses_30s_proxy`) only asserted the number of Markov transitions generated during the scout pass (which successfully proved the proxy was used) but never asserted the total audio duration or byte length of the final output payload.

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
