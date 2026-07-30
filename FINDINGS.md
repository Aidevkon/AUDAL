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

### F-044 — Every audiobook render sums the signal with itself
- **Status:** RESOLVED (fb53431)
- **Component:** `pipelines/pipelineforge/src/flavor.rs` (LufsNormalization), `sp314-nodes/src/graph.rs`
- **Trigger:** Revisit before adjusting levels or repairing clipping on the audiobook path.
- **Context:** The LufsNormalization topology splits after eq_mud into two branches — gain_makeup and ambience_reverb -> ambience_width — and both terminate at Output. DspGraph sums multiple inputs into an accumulator. ReverbNode with mix = 0.0 outputs dry * 1.0, and WidthNode with decorrelation and side_gain_db at 0.0 is likewise a passthrough. So Output receives two identical copies and produces 2 x dry, a gain of 6.02 dB with no comb filtering since neither branch adds latency. It is invisible because episode_render measures LUFS on the graph output and applies a static correction in Pass 3, so the level comes back. What does not come back is the headroom: material peaking near -1 dBFS reaches roughly +5 dBFS inside the graph before that correction, ahead of the limiter.

  RESOLVED. The mechanism was worse than described here. ReverbNode
  never received its declared mix of 0.0 because
  DspGraph::from_topology validated JSON parameters and then discarded
  them for every node type it did not construct explicitly, so the
  reverb sat at its constructor default of 0.5 and the second branch
  carried dry signal plus half a reverb tail rather than a clean
  duplicate. Peaks inside the graph reached 4.98 linear, about
  +14 dBFS, on professionally mastered material — not the 6.02 dB a
  plain doubling would give. Fixed by applying parameters to every
  node at construction and by serialising the chain, which is the
  correct topology for an insert effect that blends dry and wet
  internally. Measured on speech: dynamic range 22.50 -> 32.75 dB,
  the reverb tail no longer filling the pauses between words.

### F-045 — MaskingEQ runs a full analysis to apply 0.0 dB on the audiobook path
- **Status:** ACTIVE
- **Component:** `sp314-nodes/src/nodes/masking_eq.rs`, `sp314-dsp/src/masking_eq/mod.rs`
- **Trigger:** Revisit when wiring the LTASS correction or optimizing the audiobook path.
- **Context:** eq_mud is a MaskingEqNode. Its correction derives from stem_ratios, which do not exist when skip_stems is set, so every band resolves to 0.0 dB. The node nevertheless performs a 1024-point FFT and a psychoacoustic masking pass every 512 samples, then runs all eight biquads per sample. On an eight-hour audiobook that is millions of operations producing no change. It is also the node the LTASS chain should replace, since it occupies exactly the position the correction needs. Note: This remains ACTIVE and is one of the two remaining disconnections on the audiobook path.

### F-046 — The LTASS correction is computed and discarded
- **Status:** RESOLVED — 993f7cd
- **Component:** `aether-bridge/src/reference_resolver.rs`, `m0-daemon/src/dsp/mod.rs`
- **Trigger:** Revisit when wiring the EQ correction to the node graph.
- **Context:** aether-bridge resolves PodcastV1, measures the full file through eight 4th-order Butterworth bands in trunk_pass, mean-centres against the Byrne targets over the six measured bands, clamps to the profile's g_max_db, and produces eight ZoneAdjustment values. Measured on a real podcast clip: +2.04, -3.02, +1.66, +1.04, +0.38, -2.10, -6.00, -1.14 dB, with the 6 kHz band hitting the clamp. Those values reach DspConfig.eq.zone_bands and stop there. apply_topology_overrides carries Compressor and Ambience parameters to their nodes but the eq mapping is an open TODO, and MaskingEqNode has an empty PARAMS list with set_parameter always returning false, so it has no surface to receive them. Verified by rendering the same file with the gains forced to zero: correlation 1.00000, spectral difference 0.00 dB in every band. **Resolution:** LtassCorrection flavour added — eight peaking biquads at the LTASS centres, selected by the router when zone_bands is non-empty, placed before LufsNormalization. Q = 1.0 (chosen by measurement; minimises midpoint ripple). A_INV compensation applied before writing gains. Node id match fixed (Merger prefix). Verified by unit test (0.001 dB tolerance on compensated gains) and Hann DTFT integration test (max deviation 0.024 dB across all eight bands). Also fixes the Ambience overrides, which were silently doing nothing since the Merger prefix was introduced.

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

### F-041 — Micro-VAD Scout classifier non-functional & audited feature replacement
- **Status:** ACTIVE
- **Component:** `m0-daemon/src/dsp/sparse_scout.rs`, `sp314-dsp/src/analysis/`, `genre_centroids_generated.rs`
- **Trigger:** Before attempting to wire or re-enable the Micro-VAD Scout / Macro-Scout Router bypass.
- **Context:** A comprehensive empirical audit across 170 audio files (130 consistently-extracted tracks + 40 held-out tracks) revealed:

  1. THE SCOUT IS NON-FUNCTIONAL: All four legacy axes clamp to zero on real audio material because every threshold sits above the range the audio actually occupies (variance_a 500 vs measured 100-550; crest 14 vs 9.6-19.7; correlation 0.85 vs 0.43-0.96; mfcc_dist 3.2 vs 1.2-4.7). All four therefore "agree", and confidence — which measures agreement, not certainty — reads 0.99. Every file classifies as Music. Source of thresholds: "hand-tuned on 4 flight clips", never verified. Consequence: The Macro-Scout Router (4e3c0fd) requires Speech with confidence >= 0.4 and never receives it, so it never bypasses and always runs NMF — inert, not harmful.

  2. FOUR AXES MEASURED AND REJECTED:
     - variance_a: unbounded ms², one speech pause swallows the window; within-file range 311 to 164022 on a single file.
     - CV of inter-onset intervals at FLUX_THRESHOLD 0.20: measures timing regularity (programmed vs performed). 90% on the old corpus but 5 of 9 classical pieces land on the speech side. Note FLUX_THRESHOLD 0.01 fires ~8 onsets/sec on everything — it measures spectral flicker, not rhythm. 0.20 gives 4-5/sec.
     - envelope modulation 3-6 Hz: speech peaks at 0.88 Hz (0.59 after log) — the phrase rate, not the syllabic rate. Envelope spectra fall off 1/f and the syllabic bump is never the global maximum.
     - MFCC mean distance: c0 correlates with mean volume at r=0.9996. It was ranking by loudness. Removing c0 drops accuracy to 70%.

  3. WHAT WORKS: (CV at FLUX_THRESHOLD 0.20, frame-level cepstral flux over c1..c12 with 1024-sample frames and 512 hop).
     - Exact formulas: 
       • CV = std(IOI) / mean(IOI) for STFT spectral flux onsets > 0.20 in 5s windows.
       • Cepstral Flux = mean_n sqrt( sum_{k=1}^{12} (c_k[n] - c_k[n-1])^2 ) using 1024-sample STFT frames with 512 hop.
     - Locked Centroids & Pooled Stds (trained on 130 files):
       • CV: Music Mean = 0.4375, Speech Mean = 0.6855, Pooled Std = 0.1536
       • Flux: Music Mean = 1.3387, Speech Mean = 1.7242, Pooled Std = 0.1738
     - Measured Accuracy:
       • 86.11% window accuracy (85.39% Music, 86.83% Speech) on the 130-file training set.
       • 78.65% window accuracy (76.74% Music, 80.57% Speech) on the 40 old files (clean untouched held-out set).
     - Per-file agreement is U-shaped: 83.7% of training files and 67.5% of held-out files sit in the 70-100% agreement bins (71 Music & 16 Speech in 90-100% for Set A; 10 Music & 11 Speech in 90-100% for Set B). Errors are per-file (inherently ambiguous tracks), not per-window jitter, so short spans can be trusted.
     - The two axes fail in opposite directions: classical music has low flux and high CV; percussive music has high flux and low CV; speech is the only class that is high on both.

  4. MEASUREMENT ARTIFACTS FOUND:
     - MfccAnalyzer::compute keeps only the first FFT_SIZE (1024) samples. The Scout feeds it 240,000 (keeps 0.4%); the corpus builder feeds it 4,800 (keeps 21%). compute_windowed exists (f5ec4cc) but cannot be wired until the centroids in genre_centroids_generated.rs are regenerated with matching semantics — they were produced by measure_corpus using the truncated compute.
     - The 38-track corpus those centroids came from is gone; it lived outside the repo.
     - Any level or dynamics feature is invalid on /tmp/diverse_corpus: those 40 files were cut with -t 30 and NO -ss, so they are file openings — fade-ins and intros. One file measured 230 dB depth from a silent tail and inflated a pooled std to the point where a third axis contributed nothing.

  5. REUSABLE TOOLING:
     - scripts/gate_corpus.py: mechanical admission control (fake-stereo detection, run-length clipping, envelope-correlation duplicate detection).
     - Leave-One-Source-Out (LOSO) cross-validation: never classify a file using a centroid its own source (album/show) helped build.

  6. WHAT REMAINS:
     - The speech side has 26 sources against 104 music, and the four music failures are all vocal-dominant tracks (acapella, solo voice, whispered pop, rap) — the classifier says "voice" because there is voice. For the router this errs safely: a wrong "music" runs NMF as today, a wrong "speech" would bypass and be audible.

### F-024 — THE STEM NAMES DO NOT MATCH THEIR CONTENTS ON SPOKEN MATERIAL.
- **Status:** PARKED
- **Component:** sp314-dsp / orchestrator
- **Trigger:** Music bus / VAD
- **Context:** Measured at HEAD across five podcast files: drums 38-44%, voice 15-18%. Two pure music files as control: drums 45-48%. The split is structural, not content-driven — HPSS routes every consonant, plosive and breath into the percussive stem, leaving the voice stem holding only sustained vowels.

  Consequences for what is already shipped and what was planned:
   - the Vocal Bus gate (7164cbb) operates on roughly 16% of the speech energy. Consonants pass ungated through the drums stem.
   - the Music Bus as designed would attenuate drums, i.e. attenuate the speaker's consonants. Ducking would make speech mumble — the opposite of its purpose.
   - the Macro-Scout Router's bypass places the full mix in voice, which is arguably more correct than the normal path for speech, and that inconsistency is itself worth noting.

  On the arithmetic, since it misled us repeatedly: the NMF component masks sum to 1.0 in MAGNITUDE, and reconstruction happens in the complex domain, so stems recombine by amplitude with phase — they add coherently or cancel depending on it. The sum of individual stem energies is therefore not comparable to the input energy and never was. Any future measurement of reconstruction fidelity must compare the energy of the summed waveform against the input, not the sum of the stems' energies.

  Not resolved. Recorded so that nothing else is built on the assumption that voice means voice.

### F-042 — On the stereo path, separation is equivalent to a shelf EQ
- **Status:** PARKED
- **Component:** `sp314-dsp/src/spatial/stereo.rs`
- **Trigger:** Revisit when attempting to optimize or redesign the stereo downmix for music or speech.
- **Context:** Comparing a normal render against the same file forced through `skip_stems` yields a high correlation (0.86 for speech/classical, 0.65 for music). The spectral difference is a monotonic tilt (+3.66 dB at 63 Hz falling to -0.94 dB at 16 kHz). Applying a matching low-shelf to the unseparated render raises the correlation to >0.95. The entire audible contribution of NMF, HPSS, and the spatial stage on stereo output is reproducible with a two-pole filter, likely because the "bass" component (lfe_weight 0.8) gains low-frequency energy by construction. Separation is only genuinely required for the 5.1 render, MaskingEQ ratios, and corpus features, not for the stereo output.

### F-043 — The spatial stage produces no width, and its output is not consumed
- **Status:** PARKED (Not repaired. Recorded so the cost of the spatial subsystem is known)
- **Component:** `sp314-dsp/src/spatial/five_dot_one.rs`
- **Trigger:** Revisit before investing any further effort in 5.1 upmixing or spatial widening.
- **Context:** `FiveDotOneStage::render_chunk` and `StereoRenderer` assign identical values to front (`r[i] = l[i]`) and rear (`rs[i] = ls[i]`) pairs by construction. Measured on a music render: correlation L/R = 1.0000, Ls/Rs = 1.0000. Every stem lands perfectly centered regardless of its assignment. The `side/width` parameter is never referenced. Furthermore, the 5.1 channel balance is inverted for its stated purpose: center sits 14 dB below the front pair (-35.73 against -21.34), so dialogue does not land where 5.1 expects it. The six-channel dump is written, level-corrected, and registered in the blob store—but is never requested, not included in `DspOutput`, and not in the certificate. It consumes CPU on every music render and delivers nothing that reaches a listener. Placements react to the separation's mechanics rather than the music (e.g., `bass_lfe` acts as a fixed low shelf based on the lowest-centroid slice).

### F-047 — THE PODCAST REFERENCE TARGET DESCRIBES A DIFFERENT RECORDING CONDITION THAN THE MATERIAL IT CORRECTS.
- **Status:** ACTIVE
- **Component:** `aether-bridge/src/reference_resolver.rs`, `shared/schema/reference-profiles/podcast-v1.json`
- **Trigger:** Revisit before altering targets or addressing LTASS miscalibrations.
- **Context:** podcast-v1's spectral target comes from Byrne et al. 1994, which measured free-field speech. The material it corrects is close-mic podcast and audiobook recording. Measured on three known-good speech files, the deviations are systematic and directional:

    band 0  Sub      20-80 Hz    +3.48 dB   proximity effect
    band 2  LowMid   250-500     -2.99
    band 3  MidLow   500-1000    -3.61      free-field midrange
    band 6  Treble   4-8k        +3.98      synthetic extrapolation

  Bands 1, 4 and 5 pass. The failures are not a measurement artefact: a mean-centring test moved every band by an identical -1.386 dB, confirming the normalisation cannot selectively shift one band, and the biases point in opposite directions, which a global error cannot produce.

  Band 6 is separately explained. Byrne's data stops at 2520 Hz, so bands 6 and 7 are a -4 dB/oct extension flagged SUPPLEMENTARY_SOURCE_DERIVED in the JSON. It was chosen deliberately dark to avoid boosting hiss and overshot: measured on real speech, 51-66% of band 6 energy sits in the lower half, 4-6 kHz, which is presence and consonant definition rather than sibilance. The correction cuts 4-6 dB there on every speech file, dulling the voice by default. The three files' implied targets cluster at -5.51 dB against the profile's -9.49.

  Since 993f7cd wired the correction into the graph, all four of these are audible on every audiobook render.

  Not resolved. Three files is not a corpus and the numbers here are a direction, not a calibration. The options are a corrected target measured from close-mic speech, a per-band g_max that limits correction where the target is least certain, or excluding the derived bands. All three need more material than we have.

---
### F-048 — The brickwall limiter enforced sample peak, not true peak
- **Status:** RESOLVED (`9aa91dc`)
- **Component:** `sp314-dsp/src/limiter/core.rs`, `sp314-dsp/src/limiter/delay.rs`
- **Trigger:** Before any release that claims a true-peak ceiling, and before re-pinning `inv_qa_8_true_peak_ceiling` or `inv_mus_2_podcast_bit_exactness`.
- **Context:** `BrickwallLimiter` computes a correct true-peak estimate and then discards it. In `process()`:

      current_peak   = true_peak.process(l, r)        // correct, but this sample
                                                      // exits the delay line 240
                                                      // samples from now
      delayed_peak   = max(delay_l.max_abs(),         // raw magnitude of the
                           delay_r.max_abs())         // sample exiting NOW
      sidechain_peak = max(current_peak, delayed_peak)

  The estimate is made for the incoming sample but never travels with it through the delay line. When a sample is finally scaled, only its raw magnitude survives, so the limiter behaves as a sample-peak limiter despite `true_peak_enabled: true`.
  Measured in isolation, ceiling -1.0 dBTP, no pipeline:

      sine 997Hz 0dBFS    +0.0004 -> -0.9996   holds
      sine 19kHz phased   +0.0000 -> -1.0000   holds
      sine 12kHz phased   +0.1088 -> +0.1088   untouched
      sine 12kHz +6dB     +6.1294 -> +1.6907   over by 2.69
      impulses +3.5dB     +3.5218 -> -1.0000   holds

  The cases that hold are exactly those where sample peak equals true peak. 12 kHz at 48 k is four samples per cycle; a 45-degree phase puts every sample at 0.707 (-3.01 dB) while the crest between them reaches 0 dB. The sidechain sees -3.01, calls it safe, applies nothing. In the +6 dB case the reduction is -4.42 dB, matching the sample peak; true-peak-driven reduction would have been -7.13 dB.
  This is not a regression. `fb53431` (topology parameters were ignored) raised the signal 2.75 dB and made it visible; the defect predates it. Measured across four commits: `21d92b4` -3.04 PASS, `fb53431` -0.29 FAIL, `eabacbd` -0.17 FAIL. At `21d92b4` the test passed with 2 dB of headroom — it passed because the limiter never had to engage, not because it worked. Same shape as F-052.
  Consequence beyond the failing test: `certificate_node.rs` sets `clip_free: true_peak <= -1.0`, so every master with energy near Nyquist/4 is certified as clipped. No users today, so this is wrong data rather than a live problem.
  Guarded by two tests in `sp314-dsp/tests/`. `limiter_true_peak_oracle.rs` (`d5933e5`, un-ignored in `9aa91dc`) asserts the ceiling holds across signals whose sample and true peaks diverge; its companion, which pinned the defect and was written to fail once fixed, did exactly that and was removed. `limiter_headroom_calibration.rs` (`9aa91dc`) prints the underread survey and asserts the worst case stays below `TRUE_PEAK_HEADROOM_DB`, so changing the estimator forces re-deriving the constant rather than silently invalidating it.
  RESOLVED in `9aa91dc`, two parts. `PeakRing` (delay.rs) stores each sample's peak estimate alongside the audio delay line, so the sidechain reads the value belonging to the sample it is about to scale, and reads it across the whole lookahead window — which is why `current_peak` alone never rescued the case: the follower ramps over 240 samples and needs the high reading to persist. `TRUE_PEAK_HEADROOM_DB = 0.35` (core.rs) absorbs what remained: with the ring alone every signal landed uniformly 0.25-0.29 dB above the ceiling. Surveyed across seven signals, three release times and four ceilings, the excess was 0.2526-0.2877 dB and invariant to all three — a fixed underread by the 4x polyphase estimator, not a follower error. Confirmed independently: a static gain landing the measured true peak on -1.0 dBTP is exact to four decimals. 0.35 is the worst measured plus margin, inside BS.1770's 0.5-1 dB allowance for 4x measurement; higher oversampling is the real fix and would let it shrink. Applied only when `true_peak_enabled`. `inv_qa_8_true_peak_ceiling` passes; no pinned hash changed anywhere in the workspace.
  Separately, `inv_qa_8_true_peak_ceiling` contradicts itself — doc says -1.0 dBTP, print says -1.0, assert says -0.5, message says "-0.5 dBFS". Every schema profile specifies `true_peak_ceiling_dbtp: -1.0`. The test needs its own cleanup.

---

### F-049 — butter_hp2/butter_lp2 are resonant (Q=1.414), not the Butterworth their comments claim
- **Status:** ACTIVE — deliberately not fixed; pinned oracle guards against a drive-by "fix"
- **Component:** `sp314-dsp/src/analysis/pre_analysis.rs` (butter_hp2, butter_lp2); consumers `spectral_profile_8band`, `trunk_pass::BandpassFilter`; asymmetric consumer `aether-bridge` ReferenceResolver
- **Trigger:** Before reopening F-047, and before ANY change to butter_hp2/butter_lp2 or the 8-band spectral profile path. The pinned oracle `sp314-dsp/tests/recon_butter_q_response.rs` fails on any Q change and points here.
- **Context:** Both designers compute `alpha = sn / (2.0 * SQRT_2)`, which under RBJ semantics (alpha = sin(w0)/2Q) is Q = 1.414 — a resonant filter — while the adjacent comment says "Q = sqrt(2)/2" (0.707, Butterworth). SQRT_2 where FRAC_1_SQRT_2 was intended. Introduced in ecd7895 (2026-05-30, RFC-008 phases 1-5); code and comment arrived together, never different since. Isolated: restoration/biquad.rs and masking_eq/biquad.rs take Q explicitly and their callers pass 0.707 correctly.
  Measured (recon 2026-07-30, empirical sine sweep, HP@1kHz/48k): the current HP peaks +3.56 dB at 1.2x cutoff where corrected-Q is flat (<=0.1 dB passband ripple); at the corner the current filter reads +6.02 dB hotter than true Butterworth. The LP mirrors it (+3.56 dB at 0.9x cutoff). The 8-band bandpass cascades 2xHP + 2xLP, so band-edge inflation compounds to roughly +7 dB worst case near each band edge.
  Blast radius, measured not assumed: both profile paths (offline spectral_profile_8band, streaming trunk BandpassFilter) use the same wrong-Q filters, so profile-vs-profile comparisons self-cancel. Exactly one asymmetric consumer exists: ReferenceResolver compares filter-measured signal profiles against externally-sourced Byrne LTASS targets (podcast-v1.json) that never passed through these filters. What survives to the EQ gains is only the DIFFERENTIAL band-edge inflation left after the 6-band mean-centering in aether-bridge/src/lib.rs, and the final gains are clamped to +-g_max_db (6.0) with dead zones disabling bands 6-7 — bounded damage, but its per-band magnitude on real speech is UNMEASURED. Plausible contributor to the F-047 systematic deviations; NOT established. Establishing it means re-measuring the three F-047 files through corrected-Q filters and comparing the deviation tables — that experiment belongs to the F-047 reopening, with profile re-measurement as its own oracle set.
  `butter_hp2_q` (explicit Q, added in c5d801c for the ACX analyzer's 8th-order cascade) is the correct-semantics designer; new code should use it and state its Q.

---

### F-050 — 45 hardcoded /tmp/ paths in tests: cross-process collisions
- **Status:** ACTIVE — mechanical fix deferred to its own session
- **Component:** m0-daemon test code in 8 files: standardized_stream.rs, input_lufs.rs, six_channel_stream.rs, decode_node.rs, dsp_pipeline.rs, lazy_reader.rs, stream_core.rs, wav_to_raw.rs (tests within src/, plus tests/)
- **Trigger:** Before setting up parallel CI runners sharing /tmp, and whenever a test fails with file-not-found or corrupt-WAV symptoms on fixture paths while another cargo test process is running.
- **Context:** Agent recon 2026-07-30: 45 hardcoded /tmp/ fixture paths, zero use of the tempfile crate in tests even though tempfile already sits in the dependencies. Names are unique PER TEST, so a single cargo test run cannot collide with itself — the collision is BETWEEN two cargo test processes writing the same absolute paths (half-written WAV -> symphonia "No such file or directory"/corrupt header on the other side). This stopped being theoretical the day dev+agent parallel test runs became the routine workflow: it fired twice within one hour on 2026-07-30 (m0d WAV fixtures, initially misread as "a parallel race condition" in an agent report). Two files are only partially exposed: decode_node.rs interpolates blob ids and stream_core.rs uses a uuid for one path (safe) but keeps one fixed name (f047_dummy.wav). Fix is mechanical — tempfile::TempDir per test — and well-suited to an agent under a per-file write contract; 45 sites is a session of its own, not a drive-by. Distribution table in the 2026-07-30 transcript.

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
