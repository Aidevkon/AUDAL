# sp314-dsp — Development Philosophy

**Document:** `lineos/m1/sp314-dsp/PHILOSOPHY.md`
**Version:** 1.0
**Date:** 2026-05-21
**Status:** 🔒 LOCKED
**Owner:** Lead Architect (Anestis)

---

## The Core Principle

> **Dev time: full armored. Run time: constitutional.**

These are two completely separate rule sets.
Confusing them is the source of most over-engineering.

---

## Dev Time — Full Armored

During development, testing, and verification, use every available tool.
The goal is to arrive at a correct, verified result. How you get there
does not matter.

**Permitted at dev time:**

| Tool | Purpose |
|------|---------|
| Python3 + NumPy/SciPy | Math verification, reference output generation, algorithm prototyping |
| FFmpeg | Test fixture generation (reference audio, codec ground truth) |
| `std::f32` trig/exp/pow | Test code, build scripts, fixture generators |
| External HTTP/web search | Library evaluation, ISO standard lookup, algorithm research |
| Any Rust crate (MIT/Apache2) | Prototyping, benchmarking, test harness |
| Sub-agents | Math derivation, test writing, library evaluation |
| Parallel / async agents | Implementation, compile-fix loops, benchmark tuning |
| Python scripts in `tools/` | Any dev-time automation that helps |

**The only dev-time constraint:** MIT or Apache2 licenses only.
No GPL anywhere in the toolchain.

---

## Run Time — Constitutional

The shipping binary has strict rules.
These are not preferences — they are enforced by CI gates that fail the build.

**Run time rules (non-negotiable):**

| Rule | Why |
|------|-----|
| Pure Rust only in `src/` | No C FFI, no subprocess, no Python runtime |
| `libm` for all trig/exp/pow/sqrt | Platform-independent bit-exact results |
| No `std::f32` math methods in DSP path | `x.sqrt()`, `x.powf()`, `x.sin()` etc. are platform-dependent |
| No ML weights in LineOS M1 | ML origin rule — belongs to Aether |
| No FFmpeg subprocess | External process = non-deterministic, LGPL violation risk |
| No network calls | All core functionality runs offline |
| No `let _ =` | Silent failures forbidden |
| No `rand::thread_rng()` in pipeline | Non-deterministic |
| No hardware FTZ/DAZ flags | Platform-dependent — breaks cross-platform determinism |
| MIT/Apache2 licenses only | `cargo deny check licenses` must pass |
| Same input → bit-identical output | On x86_64, aarch64, and macOS |

---

## How This Applies to Agents

When an agent receives a task:

**Writing `src/` code:**
Follow run-time rules strictly. The constitutional CI gates will reject
violations automatically. No exceptions.

**Writing `tests/` code:**
Dev-time rules apply. `std::f32`, Python fixtures, FFmpeg-generated audio,
SciPy reference values — all permitted. The tests must be deterministic
(same test run = same pass/fail) but the test code itself is not subject
to the libm requirement.

**Writing `tools/` or `build.rs`:**
Dev-time rules apply. These never ship in the binary.

**Writing `benches/`:**
Dev-time rules apply. Benchmark code does not ship.

---

## The Practical Consequence

A sub-agent that writes a test using `std::f32::powf` to generate a sine
sweep: **correct and expected**.

A sub-agent that writes `src/` DSP code using `std::f32::powf` instead of
`libm::powf`: **constitutional violation, build fails, rejected**.

The CI gate in `§6` of every contract enforces this boundary automatically.
The agent does not need to remember — the build tells it immediately.

---

## Why This Philosophy Exists

**Dev time freedom** exists because:
- The goal is correctness, not purity
- Math verification tools (SciPy, NumPy) are more reliable than hand-derived
  Rust formulas for algorithm validation
- Restricting dev tools slows convergence without improving the product

**Run time strictness** exists because:
- `std::f32::sin(x)` on ARM and x86 can return different bit patterns
  for identical inputs — this breaks the determinism contract
- FFmpeg subprocesses introduce version-dependent behavior
- ML weights in the DSP layer violate the ML origin rule
- These are not theoretical concerns — they have caused real bugs in this
  codebase (see Architecture Decisions v1.3, M1 finding: Hermite tangent
  bug that passed 3 audit cycles)

---

## FFmpeg — Dev Time Power Tool

FFmpeg is one of the most battle-tested audio/video tools in existence —
20+ years, thousands of contributors, tested against every edge case
imaginable. We do not use it at runtime. At dev time it is invaluable.

### What FFmpeg Does For Us

**Test Fixture Generation**

Instead of writing Rust code to generate test audio (which can have bugs),
the agent runs FFmpeg:

```bash
# Pink noise — 10 seconds, 48kHz
ffmpeg -f lavfi -i anoisesrc=c=pink -ar 48000 -t 10 \
  tests/fixtures/pink_noise.wav

# Sine sweep 20Hz → 20kHz — exponential, 2 seconds
ffmpeg -f lavfi \
  -i "aevalsrc=0.5*sin(2*PI*(20*pow(1000\,t/2))*t):s=48000" \
  -t 2 tests/fixtures/sine_sweep.wav

# Impulse train — one impulse every 512 samples
ffmpeg -f lavfi \
  -i "aevalsrc=if(eq(mod(n\,512)\,0)\,1\,0):s=48000" \
  -t 5 tests/fixtures/impulse_train.wav

# Silence → loud transient stress test
ffmpeg -f lavfi -i "anullsrc=r=48000" -t 0.5 \
       -f lavfi -i "sine=f=1000:s=48000" -t 1.5 \
       -filter_complex "[0][1]concat=n=2:v=0:a=1" \
       tests/fixtures/transient_stress.wav
```

FFmpeg is verified. Our Rust sine generator might not be yet.
Use the verified tool for fixtures, save the Rust effort for the DSP.

---

**Independent Loudness Verification**

FFmpeg has a built-in EBU R128 meter — the same standard we implement:

```bash
ffmpeg -i our_output.wav \
  -af loudnorm=print_format=json -f null - 2>&1
# Returns: input_i (LUFS), input_tp (true peak), input_lra (LRA)
```

If sp314-dsp reports `-14.0 LUFS` and FFmpeg reports `-14.1 LUFS`:
within tolerance — pass.
If FFmpeg reports `-11.0 LUFS`: something is wrong — investigate.

This is **independent verification** — not self-validation.
The v2.9 Hermite bug survived 3 audits because the code validated itself.
FFmpeg cannot share our implementation bugs.

---

**Codec Reference for Codec Preview Validation**

Stage 8.5 implements deterministic codec artifact simulation.
FFmpeg produces real codec output for comparison:

```bash
# Real MP3 128k reference
ffmpeg -i input.wav -codec:a libmp3lame -b:a 128k \
  tests/fixtures/ref_mp3_128k.wav

# Real AAC 128k reference
ffmpeg -i input.wav -codec:a aac -b:a 128k \
  tests/fixtures/ref_aac_128k.wav
```

Our simulation does not need to be identical to FFmpeg output.
But spectral characteristics must agree: if FFmpeg shows HF loss at 16kHz,
our simulation must also show HF loss at 16kHz. Direction and magnitude
must match within defined tolerance.

---

**Phase and Frequency Response Verification**

```bash
# Independent frequency response measurement
ffmpeg -i our_eq_output.wav \
  -af "showfreqs=s=1024x512:mode=line" -frames:v 1 freq_response.png

# Phase comparison between dry and wet paths
ffmpeg -i dry.wav -i wet.wav \
  -filter_complex "[0][1]join,aphasemeter" phase_check.wav
```

For the comb filtering integration test: FFmpeg produces the reference
frequency response. Our test compares pipeline output against it.
No custom FFT analysis code needed in the test harness.

---

**Golden Blob Independent Validation**

After the pipeline produces its final output, FFmpeg runs a sanity check:

```bash
# True peak check
ffmpeg -i mastered.wav -af astats -f null - 2>&1 | grep "Peak level"

# LUFS target validation
ffmpeg -i mastered.wav \
  -af "loudnorm=I=-14:TP=-1:LRA=11:print_format=json" -f null -
```

sp314-dsp and FFmpeg must agree on LUFS within ±0.2 LU.
If they disagree beyond that — it is a bug, not a rounding difference.

---

### The Rule

```
FFmpeg role              │ Where              │ Status
─────────────────────────┼────────────────────┼──────────────
Generate test fixtures   │ tests/fixtures/    │ ✅ Encouraged
Measure reference LUFS   │ CI verification    │ ✅ Encouraged
Real codec encoding      │ tests/fixtures/    │ ✅ Encouraged
Phase response check     │ test harness       │ ✅ Encouraged
Any call in src/         │ shipping binary    │ ❌ Forbidden
Any subprocess at runtime│ shipping binary    │ ❌ Forbidden
```

---

### Agent Instructions

When writing tests or generating fixtures:

> FFmpeg MAY be used freely. Every fixture generated by FFmpeg MUST be
> committed to `tests/fixtures/` with a `README.md` documenting the exact
> command used, the FFmpeg version, and the purpose of the fixture.
> This ensures reproducibility — another agent or developer can regenerate
> the fixture deterministically.
>
> FFmpeg MUST NOT appear in `src/`, `Cargo.toml` dependencies, or any
> code path that executes at runtime. The CI constitutional gate checks
> for `Command::new` in `src/` and will reject any violation.

---

```
tests/          → dev-time rules  → use anything that works
tools/          → dev-time rules  → use anything that works
build.rs        → dev-time rules  → use anything that works
benches/        → dev-time rules  → use anything that works

src/            → run-time rules  → libm, pure Rust, no exceptions
```

The boundary is `src/`. Everything outside `src/` is dev-time.
Everything inside `src/` is constitutional.

---

*Dev time: full armored.*
*Run time: constitutional.*
*The CI gate is the enforcer — not the agent's memory.*
