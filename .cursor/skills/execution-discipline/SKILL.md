---
name: execution-discipline
description: >-
  Hard rules for the execution agent's conduct inside a single task:
  prompt boundaries, forbidden commands, how edits are made and
  shown, and how numbers are reported. Use this at the START of
  every task, before touching anything. Distilled from real
  incidents on 2026-07-16/17 (each rule below exists because its
  violation caused rework that session).
---

# Execution Discipline

## 1. One prompt = one action = full stop
Execute ONLY the task in the current prompt. Do not carry out steps
remembered from earlier prompts, do not anticipate the next step
even when it is obvious. Mid-prompt "STOP" markers are not
reliable — the task simply ends where the prompt ends.
(Incident: "part 2" was executed unprompted from stale context,
overwriting an approved correction in the process.)

## 2. NEVER git push — and confirmation-seeking for destructive git
git push is orchestrator-only, manually, every time — even when a
push seems obviously next. git checkout/reset/clean on tracked
files only when the prompt explicitly instructs it.
(Incident: two unprompted pushes in one session; harmless only by
luck.)

## 3. Direct edits and direct commands only — no intermediary scripts
No ad-hoc Python/sed/awk file surgery, no wrapper shell scripts for
edits or verification sequences. Use the editor's native edit
operations and run commands one at a time.
(Incident: a brace-counting Python deletion script left orphaned
attribute lines above a neighboring struct — caught in review, not
by the script.)

## 4. Show, don't summarize
Every edit task ends with the raw `git diff` pasted verbatim — full
hunks, every file. A described diff is not a reviewable diff. Long
output is not a reason to summarize; truncation is declared, never
silent.
(Incident: a five-file change was reported as a prose summary; the
diff, once shown, revealed a trait method had been deleted.)

## 5. Adjacent-line discipline
Deletions remove exactly the specified lines. Before showing the
diff, re-view the lines ABOVE and BELOW every deletion/insertion
point: doc comments, attributes (#[derive], #[serde], #[cfg]),
and opening braces belong to their item and die or survive with it.
(Incidents: a deleted let-binding took a still-used neighbor with
it; a deleted struct left its doc comment and #[derive] orphaned
over the next item; an inserted attribute landed outside its impl
block's indentation.)

## 6. Numbers name their source
Every count reported (tests, warnings, lines) states exactly where
it came from: which test binary, fresh-vs-cached build, which log
file and which grep. An unexpected number is reported as a
deviation — never smoothed over with a plausible-sounding
explanation, and never labeled "as expected" when it isn't.
(Incidents: warning counts from a cached clippy log; a neighboring
crate's test count reported as the target suite's; "matches
expectations" written over an off-by-one.)

## 7. Deviations from spec are declared, never silent
If the applied edit differs from the prompt's exact text — even as
an improvement, even when both halves of a deviation cancel out —
say so explicitly in the report. The reviewer decides whether the
deviation stands; silence converts a judgment call into a hidden
change.
(Incident: an output-path field assignment deviated from spec in
two mutually-canceling ways; it happened to be better, but was
discovered in review rather than declared.)

## 8. When blocked: stop and report, with options
A gate failure, a non-matching "replace" text, a moved value, a
truncated log — all mean STOP and report verbatim, optionally with
2-3 candidate paths forward, and wait. Improvising past a blocker
is how single incidents become compound ones.
(Positive example: the by-value decoder move was reported instead
of improvised around — the fix chosen in review, a blanket &T impl,
was better than either improvisation candidate.)

---
