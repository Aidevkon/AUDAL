# Amendment A-004 — Enforceability

**Document:** `creator-os/constitution/amendments/A-004-enforceability.md`
**Amends:** Creator OS Constitution v2.6 → v2.7
**Date:** 2026-09-06
**Status:** 🔒 LOCKED
**Approved by:** Anestis (Lead Architect), 2026-09-06
**Author:** measured during the session of 2026-09-06

---

## Preamble

The constitution is correct in doctrine and wrong in mechanism.

Every rule in this amendment was **measured**, not inferred. Each one
names a rule that was written before the system existed, and states
what was found when the system was finally measured against it.

Per §14, this amendment is **additive**. Nothing in v2.6 is deleted.
Sections are clarified, and the enforcement mechanism is replaced
where it was found to be impossible.

---

## A-004.1 — A rule whose enforcement mechanism cannot work is not a rule

**Amends:** §12 (CI Requirements), row "No silent failures"

### What v2.6 says

> | No silent failures | inline grep | `let _ =` → build failure |

And §13 lists: `❌ let _ = anywhere in production code — silent failure`

### What was measured (2026-09-06)

Three separate inline greps were run against the same tree:

```
grep -v test                          →  142
cut at first #[cfg(test)]             →  181
same, excluding tests/ and benches/   →  119
```

Three mechanisms, three numbers, no agreement.

The reason is structural: **in Rust, unit tests live inside the
production file** under `#[cfg(test)]`. A line-based grep cannot
distinguish production code from test code. It never could.

Verified by sampling: the four occurrences of `let _ = engine…`
that looked most alarming — a `Result` from audio processing being
discarded — are all inside `#[cfg(test)]` blocks in
`two_pass.rs`, with `assert_eq!` on the next line.

### What the classification actually shows

Of the occurrences found, the shape is:

| category | approx. count | judgement |
|---|---|---|
| UI / Tauri (`listen_fn`, `invoke`, `app.emit`) | ~20 | outside the audio path |
| channel sends (`response.send`, `tx.send`) | ~43 | receiver may legitimately be gone |
| filesystem cleanup (`remove_file`, `create_dir_all`) | ~17 | idempotent by nature |
| **everything else** | **~15** | **this is what the rule was written for** |

A rule that fires a hundred and nineteen times is not enforced.
It is bypassed. This is the same failure as a black-list of values,
which proved to be a trap three times in the same session, and the
same failure as fourteen CI gates of which four exist.

### The amendment

The prohibition on discarding a `Result` **stands unchanged in
intent**. Its mechanism does not.

1. `inline grep` is **removed** as the specified mechanism. It is
   incapable of the distinction the rule requires.
2. The rule is narrowed to what it was written to protect:
   **discarding a Result whose failure would leave data or audio in
   an unknown state.** Channel sends to a receiver that may have
   disconnected, and idempotent filesystem cleanup, are permitted
   **with a written reason on the line**.
3. Enforcement is by a guard with a **frozen count** that warns
   rather than blocks, consistent with A-004.2. A clippy lint
   (`let_underscore_must_use` or equivalent) may replace it once
   its false-positive rate is measured — not before.
4. Until such a guard exists, this row of §12 is marked
   **NOT ENFORCED** rather than left claiming to be a hard failure.

### Declared limit of the measurement

The figure of one hundred and nineteen cuts at the **first**
`#[cfg(test)]` in each file. A file with a second test block lower
down is under-counted. The number is the most careful of three, not
a certainty, and no decision in this amendment rests on its exact
value.

---

## A-004.2 — A gate that blocks is a gate that gets bypassed

**Amends:** §12, opening sentence

### What v2.6 says

> All CI gates are hard failures. There are no warnings.

Fourteen gates are then named.

### What was measured (2026-09-06)

Four workflow files exist. The fourteen named scripts in
`infra/ci/checks/` were not found. `layer-isolation-check.sh`,
named in §02.2 as enforcing dependency direction on every commit,
does not exist — and five upward dependency edges were measured in
the workspace graph.

Meanwhile, six guards written during the session of 2026-09-06 —
covering unsourced thresholds, uncalled gates, silent tests, empty
passes, broken references and stale line pointers — all **warn and
freeze a count** rather than blocking. All six are run, and all six
have caught real defects, including three cases where they caught
their own author.

The reason is recorded in the project's own design notes from
2026-08-12: *a commit that should not be blocked is exactly what
teaches people to add `--no-verify`.*

### The amendment

§12 is amended to recognise **two classes of gate**:

- **BLOCKING** — correctness of the delivered artifact. Determinism,
  signature validity, schema validity. A failure here means the
  output is wrong.
- **FROZEN COUNT** — accumulated debt. The count may not rise. The
  existing count is cleaned over time. A rise is a hard failure;
  the standing count is not.

A gate must declare which class it belongs to. A gate that names no
class defaults to FROZEN COUNT.

Gates named in §12 that do not exist are marked **NOT BUILT** in the
table rather than listed as if they run. A gate that is claimed and
absent is worse than one that is absent and admitted, because it
consumes the trust that a real gate would have earned.

---

## A-004.3 — `crates/` is defined and was never created

**Amends:** §10 (Repository Structure)

### What v2.6 says

> **Rule:** This structure is immutable.

And §06.2:

> Shared Rust crates with at least two distinct consumers live in
> `crates/`. A crate with only one consumer stays inside that
> consumer's directory.

### What was measured (2026-09-06)

Four of the eleven directories in §10 do not exist: `backend/`,
`crates/`, `engines/`, `gallery/`.

Three crates in the tree satisfy the §06.2 test for `crates/`
exactly — two or more distinct consumers, no layer identity of
their own — and are therefore homeless:

| crate | current location | consumers | internal dependencies |
|---|---|---|---|
| `proof` | `proof/` | two | none |
| `integration` | `integration/` | three | none |
| `aether-bridge` | `shared/aether-bridge/` | one, plus the layer it bridges | three |

A dependency-graph scan reported five upward edges from these
placements. Two of them dissolve on inspection: a crate with no
internal dependencies cannot point upward at anything.

**These are not violations. They are correct crates in the wrong
place, because the right place was specified and never built.**

### The amendment

1. `crates/` is created, and the three crates move into it. Each
   move is a separate step with its own verification, per the
   project's one-change-at-a-time discipline.
2. §10 is amended: directories that exist are listed as such;
   directories that were specified and never built are listed as
   **NOT BUILT**, with the date they were specified.
3. "This structure is immutable" is replaced by: **the structure of
   what exists is immutable; what has not been built is a plan, and
   a plan is amendable without ceremony.** A structure that
   guarantees permanent violation is not a constraint — it is a
   standing untruth.

---

## A-004.4 — External tools are forbidden in the product, required in verification

**Amends:** §07.3 (Forbidden Technologies)

### What v2.6 says

> **Forbidden runtimes and processes:**
> - `ffmpeg` — external process, non-deterministic across versions

Unqualified.

### What was measured (2026-09-06)

`ffmpeg` and `ffprobe` were used dozens of times during a single
session as **independent jurors**, and each time they settled a
question our own code could not:

- the sample-rate ratio of exactly 0.500 that exposed a spectral
  measurement running at half scale for months
- agreement to within eight thousandths of a decibel on the
  encoder's output
- confirmation that the delivered file is forty-four one, mono, one
  hundred and ninety-two kilobits
- the loudness reading that verified our own meter against an
  implementation nobody here wrote

The project's own doctrine states the principle plainly: *prefer
measurement of the final file with an external tool; the tool that
opened the container proved more than twenty unit tests.*

The distinction between product and verification exists in the
project's working philosophy. It does not exist in the constitution,
which therefore reads as forbidding the best verification work done
to date.

### The amendment

§07.3 is amended to distinguish:

- **Production path** — anything whose output reaches the user, or
  whose behaviour affects the delivered artifact. External processes
  remain **forbidden** here, for the reason given: non-determinism
  across versions.
- **Verification path** — anything whose output is read by a human
  or by a test, and which cannot alter the artifact. External tools
  are **permitted and encouraged** here. An independent
  implementation is worth more than a second opinion from the same
  codebase.

A verification tool must be named, and its version recorded, in any
document that cites its output.

---

## A-004.5 — A dependency rule must be technically correct

**Amends:** §07.3, forbidden dependency patterns

### What v2.6 says

> - `hound`, `lewton`, `claxon` — redundant; `symphonia` covers all
>   formats

### What was measured (2026-09-06)

`hound` appears in four production manifests. One carries its own
justification in a comment: *WAV write — pure Rust, MIT.*

`symphonia` is a **decoding** library. It reads containers. It does
not write WAV files. The rule assumes a capability the named
replacement does not have.

### The amendment

The prohibition on `lewton` and `claxon` stands — both are decoders
and both are genuinely redundant.

`hound` is **permitted for writing** and remains forbidden for
reading, where `symphonia` is the single decoder. Any use of
`hound` in a production manifest must carry a comment stating that
it is used for writing.

This is not a relaxation. It is the correction of a rule that was
factually wrong about what the approved library does.

---

## A-004.6 — Section numbering

**Amends:** §03 through §08

Amendment A-001 states that it *renumbered all subsequent sections*.
It did not finish. As of v2.6:

- `§03.1` appears **twice** — once as Pipeline Definition Rules,
  once as Engine & Device Registry
- `§05.1` and `§05.2` appear inside `§06`
- `§06.1` and `§06.2` appear inside `§07`
- `§07.1` through `§07.3` appear inside `§08`

Sub-section numbers are corrected to match their parents. **No text
changes.** The correction is recorded here rather than made
silently, because a document that renumbers itself without saying so
breaks every reference pointing into it — which is the same failure
this project measured sixteen times in its own documentation on the
same day.

---

## A-004.7 — The dependency audit enforces a different rule than the one written

**Amends:** §08 (Technology Stack), dependency audit row

### What v2.6 says

> | Dependency audit | `cargo deny` | Zero violations required |

And §12 lists: `| License audit | cargo deny check licenses | MIT/Apache2 only |`

### The error in this sub-article, recorded rather than corrected silently

This sub-article was drafted claiming the configuration did not
exist. **It does**, committed in May, 479 bytes. The claim came from
a shell chain that broke before the check ran, and the `||` fallback
caught the whole command rather than the missing file — the sixth
instrument in this session to report a number without measuring one,
and the only one whose fault was a shell operator.

The error is recorded here rather than corrected silently: an
amendment about mechanisms that do not work must not itself rest on
one. Per §14 an amendment is additive and sections may be
**clarified**; this correction was made before the document locked,
which makes it clarification and not revision.

### What was measured (2026-09-06, re-measured)

`deny.toml` exists and is committed. Its allow-list holds **nine**
licences: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception,
BSD-2-Clause, BSD-3-Clause, Unicode-3.0, MPL-2.0, Zlib, ISC.

§12 says `MIT/Apache2 only`.

**The mechanism exists and enforces a different rule than the one
written.** That is worse than absence, and by a wide margin: an
absent gate is visibly absent, while this one has been passing since
May against a policy nobody approved. MPL-2.0 in particular is
weak-copyleft — a class §07.3 does not contemplate — and it is on
the list.

The rest of the pattern still holds. Of the fourteen gates §12
names, four exist. The inline grep does not. The difference here is
that the configuration was built and then diverged, rather than
never built at all.

The licence question that prompted the search turned out to be
already solved, and solved well. The MP3 encoder binding links
**dynamically only**, its build script refuses static linking in
three separate comments, the crate itself is MIT, no encoder source
is vendored, and a locked notice from April documents the compliance
requirement and names the constitution as its authority.
**Nothing was wrong. Nothing was checked either.**

One gap is real: the third-party register scopes itself to assets
embedded via `include_bytes!` and explicitly excludes crates, on the
grounds that crate licences travel in registry metadata. That is
correct for permissive licences. A weak-copyleft dependency needs
visibility **to the user of the product**, not only in the
repository.

### The amendment

1. `deny.toml` exists. Both rows are marked **NOT ENFORCED** until
   the allow-list and §12 say the same thing — a gate that passes
   against an unapproved policy is not enforcement, it is noise with
   an exit code of zero.
2. The audit runs as a **FROZEN COUNT** gate per A-004.2, so that an
   existing advisory does not block a commit while a new one cannot
   arrive unnoticed.
3. The divergence is resolved in one direction, explicitly: either
   §12 is widened to the nine licences actually in use, each with its
   compliance reasoning, or the allow-list is narrowed and the
   offending dependencies are named and removed. **The direction is a
   decision, not a cleanup**, because MPL-2.0 carries obligations the
   permissive two do not.
4. A user-facing licence notice is required before commercial
   release, covering any dependency whose licence imposes obligations
   on distribution. The existing notice is the content; what is
   missing is its path to the user.
5. The duplicate copy of that notice under `lineos/plan/` is removed.
   Two identical legal documents in two places is one document and
   one future contradiction. The build script that cites it by bare
   filename gains the full path.

---

## A-004.8 — The amendment process has never been followed to completion

**Amends:** §14 (Amendment Process) — adds a record, changes no step

### What v2.6 says

> 1. Draft the amendment as a separate document
> 2. Increment the version (`2.5 → 2.6`)
> 3. Document the reason and impact in the Changelog
> 4. Update all affected layer constitutions to reference the new version
> 5. Update CI to enforce any new rules

### What was measured (2026-09-06)

**A-001 is the only previous amendment.** It locked on 2026-04-09 and
produced v2.6. Of the five steps:

| step | A-001 | evidence |
|---|---|---|
| 1 — separate document | **done** | `A-001-execution-model.md`, LOCKED |
| 2 — increment version | **done** | header reads 2.6 |
| 3 — changelog entry | **done** | the 2.6 row names the amendment |
| 4 — update layer constitutions | **NOT DONE** | all seven still read v2.5 |
| 5 — update CI to enforce | **NOT DONE** | the two gates its own changelog names do not exist |

Step 4, measured across every layer constitution in the tree:

```
aether/constitution/aether-constitution.md              v2.5
aether/constitution/pre-analysis-constitution.md        v2.5
apps/stillair/constitution/hud-constitution.md          v2.5
lineos/constitution/lineos-constitution.md              v2.5
lineos/m0/constitution/m0-constitution.md               v2.5
marketplace/constitution/marketplace-constitution.md    v2.5
pipelines/constitution/pipelines-constitution.md        v2.5 · Amendment A-001
```

Seven documents, five months, zero updated. The last of them cites
A-001 by name while still declaring authority from the version A-001
superseded.

Step 5 fails on its own evidence: the 2.6 changelog row states *«CI
gates: pipeline-logic-check.sh + layer-orchestration-check.sh»*.
Neither file exists anywhere in the tree.

### The amendment

1. **The seven layer constitutions are NOT bumped by this amendment.**
   Jumping them from v2.5 to v2.7 would erase the evidence that step
   4 was skipped, and would assert a compatibility with v2.6 that
   nobody measured. Their alignment is **named work**, not a side
   effect of this document.
2. Step 4 gains a completion criterion: a layer constitution is
   updated when its content has been **read against** the new version,
   not when its header string changes. A version number is a claim
   about compatibility, and an unmeasured claim is the failure mode
   this entire amendment exists to name.
3. This sub-article is itself the record required by step 3 — the
   process documents its own non-compliance, because the alternative
   is a fourth source of truth.

### Why this is the heaviest finding in the amendment

Every other sub-article names a rule whose mechanism does not work.
This one names **the mechanism by which rules are supposed to
become real**, and finds that it has run once and completed three
fifths of itself.

A-004 is the second. If it holds all five steps it is the first to
do so — and step 5 is the one it cannot hold, for the reason stated
under §14 below.

---

## Changelog entry (to be appended to the constitution)

| Version | Date | Changes |
|---------|------|---------|
| 2.7 | 2026-09-06 | Amendment A-004: Enforceability. §12 recognises BLOCKING and FROZEN COUNT gate classes; `inline grep` removed as an impossible mechanism for the silent-failure rule; gates that do not exist marked NOT BUILT. §10 distinguishes what exists from what was specified and never built; `crates/` created and three homeless crates relocated. §07.3 distinguishes production path from verification path for external tools, and corrects the `hound` rule, which assumed a write capability `symphonia` does not have. §08 dependency audit marked NOT ENFORCED because `deny.toml` exists and allows nine licences where §12 says two; user-facing licence notice required before commercial release; duplicate encoder licence notice removed. §03–§08 sub-section numbering identified as incomplete after A-001 and recorded as pending — not executed, 378 section references across 72 files. §14 records that the amendment process has completed three of its five steps once, in A-001: the seven layer constitutions still cite v2.5 and two CI gates named in the 2.6 changelog were never built. |

---

## What this amendment does not do

It does not weaken a single doctrinal rule. The ML Origin Rule, the
Adapter Boundary, determinism, contract-only communication, no
training on user data, and the prohibition on discarding results
that matter all stand exactly as written.

It changes only mechanisms that were measured and found incapable,
and structures that were declared immutable without ever having been
built.

---

*If it is not in a constitution, it does not exist.*
*If its enforcement cannot work, it is not enforced — and saying so
is the first step to enforcing it.*
