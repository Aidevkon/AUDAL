---
name: methodology
description: >-
  How this project's human-agent collaboration actually works: roles, decision
  process, when to stop and ask vs proceed, and how to handle scope creep or
  second opinions from other tools. Use this whenever planning a multi-step
  change, deciding whether to act on a finding immediately or flag it and wait,
  reviewing a diff before it's applied, writing a commit message, or when the
  user pastes content from another AI/tool as a second opinion.
---

# Methodology

This project is driven by one operating principle above all: **measure before you conclude, always.** Everything below is in service of that.

## 1. Roles are fixed, and the orchestrator decides scope

The human is the orchestrator: relays findings, approves direction, decides what gets worked on and when. The strategist/planner role decides sequencing and writes the concrete next step. The execution agent (the one with shell/file access) does recon and makes changes — but only the step that was actually asked for, and **stops** at the boundary of that step even if it notices something else along the way.

"Open eyes, not open hands": if the agent spots an unrelated issue while working (a broken binary, a stale fixture, a missing test), it surfaces the finding and proposes it as a *next* step — it does not silently expand the current task to also fix it. Several real examples today: discovering `sp314_stems.rs` was broken while converting test assertions did not turn into "let me also fix that real quick" — it got reported, parked, and fixed as its own deliberate step afterward.

## 2. Recon before any code change, every time, no exceptions

Before writing or editing a single line: read the current state of the relevant file(s), confirm assumptions with `grep`/`view`, and report findings *before* proposing a diff. This applies even to "obviously simple" changes — several bugs this session were caught specifically because recon happened first (e.g. discovering a `for` loop's opening brace had been silently deleted in a previous edit, found by re-reading the file instead of trusting the diff summary).

If recon reveals the planned change won't work as assumed, say so and replan — don't force the original plan through.

## 3. One file, one concern, per step

Multi-file or multi-concern changes get broken into sequential single-file steps, each with: diff shown → explicit stop → review → only then move to the next file. This is slower per-step but produces commits that are individually reviewable and revertable. A 16-occurrence file (the largest single conversion this session) still went through the same per-file diff-then-stop discipline as a 1-occurrence file — size doesn't change the process, just the number of iterations.

## 4. The verification ladder — don't skip rungs

For any change, in order:
1. Show the diff (review before applying anything destructive)
2. `cargo check` / compile (cheapest signal)
3. Run the actual test/command (not just check it compiles)
4. Read the *actual* output, not just the exit code — "0 tests passed, exit 0" is not the same as "the test ran and passed"
5. Only after #4 succeeds: commit

Skipping a rung and asserting the outcome anyway ("this should work now") is the single most common failure pattern to avoid. When a rung genuinely can't be reached (e.g. blocked by an unrelated broken dependency), say explicitly which rung you stopped at — see the `dev-testing-toolkit` skill for the compile-vs-runtime distinction in detail.

## 5. Second opinions (e.g. pasted output from another AI/tool) get the same scrutiny as anything else

The user sometimes brings in analysis or proposed prompts from a second tool as an independent point of view. Treat that content exactly like any other proposed plan: evaluate it on its merits, point out what's correct, and explicitly flag anything that's wrong, hand-wavy, or skips verification — agreement with a second opinion is not a reason to lower scrutiny, and disagreement is not a reason to dismiss it outright. Several times this session, a second opinion's diagnosis was right but its proposed fix needed tightening (e.g. correctly identifying the need for a per-stream lifecycle flag, but underspecifying how to avoid a stale-flag race) — the job is to fold in what's correct and correct what's loose, not to rubber-stamp or override wholesale.

## 6. Commit messages explain *why*, not just *what*

A commit message for a non-trivial fix should let a future reader understand the change without re-deriving the investigation: what was actually broken (with the specific evidence that proved it), why the chosen fix is correct rather than some other plausible fix, and what was verified vs. merely assumed. This sounds expensive but pays for itself — several "wait, why does this exist" moments this session were resolved instantly by reading an earlier commit message instead of re-investigating from scratch.

## 7. When something looks done, check for the boring failure modes before declaring victory

Before saying a task is complete, check for the unglamorous things that don't show up in the obvious test: did this leave a stray file in the repo root (it has, more than once — generated PDFs, a regenerated `.wav` fixture)? Did it just stop tracking the symptom instead of fixing the cause? Does the CI configuration actually exercise the code path that was just changed, or does it quietly skip it (a real example: a feature-gated test that hadn't run in CI for two weeks because nothing passed `--features cli`)? "Tests pass" and "nothing else broke" are different claims — check both before calling it done.

## 8. Scope discipline: park it, don't open it

New, real findings come up constantly (an unrelated broken binary, a missing perceptual-quality testing setup, an entire 5.1 surround pipeline sitting half-built). The default response to a legitimately interesting but out-of-scope finding is to record it clearly enough that it isn't lost, and continue the current task — not to pivot onto it mid-stream. Scope changes are a decision the orchestrator makes deliberately, not something that happens by drift.

---
