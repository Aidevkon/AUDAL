# Prune status — §6.4 categories, tracked here

**Source of names:** `docs/specs/PRD-v7.2-20260918.md` §6.4 ("The new
repository"), the sentence starting "Scope (Appendix B as guide): …".
That document is intent/spec, revised at least twice already this
session (Router/Flavor/Pipelineforge broke apart on reading — F-125;
cut-repair broke apart on reading — O-008, `docs/ORPHANS.md`), with a
proposed PRD modification toward a next version given in conversation
21/09, outside the tree, not applied — the live document is v7.2 and
stays that way; if that proposal is ever applied, its hooks get
verified against the live document first. **This file does not copy
the list — it
tracks status only, one line per category, and points back to the PRD
for the name.** If the PRD's wording of a category ever changes, this
file's line for it is read against the PRD text at hand, not against
what's written below.

Each line: the category name as §6.4 writes it · fate · evidence (commit
hash or F-/O-number). Nothing here is a judgment call beyond what's
already measured and committed elsewhere in the tree — `FINDINGS.md` and
`docs/ORPHANS.md` carry the actual measurements; this file only says
where each §6.4 name currently stands.

---

## The fourteen names, as §6.4 lists them

1. **cockpit** — ΕΦΥΓΕ. Deleted with Tauri, `83f908ea` (34,618 lines,
   both folders together — `apps/stillair/src-tauri`,
   `apps/stillair/cockpit-dioxus`).
2. **JiniPanel and Ollama** — split fate, one name, two subjects.
   `JiniPanel` (the UI component) ΕΦΥΓΕ — it lived inside
   `cockpit-dioxus/src/panels/jini_panel.rs` and left with it in
   `83f908ea`. The Ollama-calling backend logic is a separate thing and
   is still live in the server: `lineos/m0/m0-daemon/src/jini/mod.rs`
   (`ollama_call`, `ollama_name_call`) and the pre-warm call in
   `lib.rs:176` (`http://localhost:11434/api/generate`) — ΦΕΥΓΕΙ ΜΕ ΤΟΝ
   ΔΙΑΚΟΜΙΣΤΗ, not yet.
3. **Router/Flavor/Pipelineforge** — ΔΕΝ ΚΛΑΔΕΥΕΤΑΙ ΩΣ ΕΧΕΙ, F-125.
   Measured live: three of the nine `Flavor::` stages are real delivery
   stages that travel forward — `DcRemoval` (Stage 0, R5b),
   `HumRemoval` (Stage 2, R5b), and `POXVoice` (source of Stages 1, 2,
   3, 6 — O-007, ΤΑΞΙΔΕΥΕΙ). The category doesn't prune as a unit.
4. **dead DSP nodes and their tests** — partial. Three of four ΕΦΥΓΑΝ,
   `de8a6c90` (multiband, harmonic, lookahead_telemetry — 400 lines).
   The fourth, `nodes/limiter.rs`, is not deleted and is not scheduled
   here: §6.0 has a "LIMITER (spec ceiling)" box in the delivery chain,
   the function is a requirement, and no delivery topology has been
   completed yet. Judged when the first one runs.
5. **spatial path** — ΜΕΝΕΙ ΠΙΣΩ ΜΕ ΑΠΟΦΑΣΗ ΙΔΙΟΚΤΗΤΗ, 19/09 (width).
   Has a live caller in the core: `sp314-dsp/src/stft/two_pass.rs:2143`
   (`FiveDotOneStage::render`, inside `compute_firewall_scales`, called
   from `:1527`). Judged, not disconnected.
6. **third upmix** — ΜΕΝΕΙ ΠΙΣΩ ΜΕ ΑΠΟΦΑΣΗ ΙΔΙΟΚΤΗΤΗ, 19/09 (width).
7. **cut-repair** — ΔΕΝ ΚΛΑΔΕΥΕΤΑΙ ΩΣ ΕΧΕΙ, O-008 (`docs/ORPHANS.md`,
   corrected 19/09). The one candidate that looked like a match
   (`cut_heal/` folder) was a name-match, not a behavior-match — the
   literal string "cut-repair" appears nowhere in the code. What
   `cut_heal` actually does (detect-only cuts + fixed-length crossfade
   smoothing) is already named elsewhere as the de-click stage
   (R5b Stage 4), not as "cut-repair". Current status: ΑΚΡΙΤΟ — no §6.4
   category currently names this behavior.
8. **EarFatigue model** (PRD's own parenthetical: "keep the set
   plumbing R4 needs") — ΦΕΥΓΕΙ ΜΕ ΤΟΝ ΔΙΑΚΟΜΙΣΤΗ.
9. **fuzzer** — ΔΕΝ ΥΠΗΡΞΕ ΠΟΤΕ. Zero folder, zero `fuzz_target!`, zero
   dependency, zero history.
10. **Tauri** — ΕΦΥΓΕ, `83f908ea` (with cockpit, same commit, same
    34,618-line deletion).
11. **SurrealDB and the six unused tables** — ΦΕΥΓΕΙ ΜΕ ΤΟΝ ΔΙΑΚΟΜΙΣΤΗ.
12. **Caddy** — ΕΦΥΓΕ, `b2f5c5a9` (267 lines — the field, its readers,
    `caddy.json`, and the Justfile recipes that validated it).
13. **axum** — ΦΕΥΓΕΙ ΜΕ ΤΟΝ ΔΙΑΚΟΜΙΣΤΗ, and whatever else lives only
    in `m0-daemon`.
14. **the segmenter with its classifier** (`SegmentScout`,
    `smooth_and_segment`, escalation flagging, the onset detector's
    Otsu threshold) — ΚΛΕΙΔΩΣΕ ΩΣ ΜΗ ΚΛΑΔΕΥΣΙΜΗ, F-135. Runs on both
    live button paths (`execute_streaming_plan` via
    `agents/executor.rs:310,316`, `run_dsp_internal` via
    `domain/dsp_pipeline.rs:1036`), both calling into
    `trunk_pass.rs:890` for every window.

⚠ **The commit hashes in the original dictation of this DOCUMENT task
(`28d1d09`, `d0d9c70`, `3d71793`) do not exist in this repository** —
checked (`git cat-file -t`, all three: not a valid object name). The
three real prune commits, verified against `git log`, are `de8a6c90`
(dead DSP nodes), `83f908ea` (interface/Tauri/cockpit), `b2f5c5a9`
(Caddy) — used above instead, per the repo-over-description rule.

**`m0-daemon` itself does not leave** until `execute_streaming_plan` and
`run_deliver_core` relocate out of it (PRD:465, F-133) — everything
tagged ΦΕΥΓΕΙ ΜΕ ΤΟΝ ΔΙΑΚΟΜΙΣΤΗ above waits on that, not on a decision
of its own.

**Cost of the three completed deletions, to fix once at the end of the
prune, not now:** `line-ref-lint` rose from 2 to 14 broken references,
`reference-lint` from 39 to 43 (F-132 and the guard-cost notes in the
Prune 2/3 commit messages).
