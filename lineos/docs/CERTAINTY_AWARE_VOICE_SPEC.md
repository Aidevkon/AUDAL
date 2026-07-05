# Certainty-Aware Voice Processing — Specification

**Version:** 0.1 (Draft / Exploratory)
**Status:** Design principle + subsystem sketch — NOT scheduled
**Subsystem candidate ID:** (to be assigned when folder created)
**Authority:** Creator OS design philosophy — "trust the math"
**Context:** Emerged from Phase 8 podcast/Episode streaming work.
This is forward-looking R&D, not current-wave implementation.

---

## 0. One-Sentence Summary

A deterministic voice-processing layer that predicts what kind of
speech sound is coming next, acts before it lands, and — critically —
knows how certain it is, propagating that certainty through the DSP
intensity, the JINI narration, and the certificate.

---

## 1. The Core Principle: Certainty Propagation

This is not a feature. It is a horizontal design principle that
crosses three layers.

Today the system throws away uncertainty at the enum boundary:

```
analysis computes something continuous
    ↓
collapses to enum: SpectralBehaviour::Muddy
    ↓  ← uncertainty discarded HERE
JINI sees only the enum → speaks with 100% confidence
```

The problem: a 51%-vs-49% "Muddy" call is narrated with the same
absolute certainty as a 99% call. The system is (in the agent's
words) "confidently blind."

This creates a small crack in the trust MOAT: the certificate layer
is honest and deterministic, but the narration layer overstates
confidence. A product that sells "don't trust us, trust the math"
should not have a narrator that bluffs.

**The fix is not to add a model. It is to stop discarding
information the system already has.**

```
Instead of:
  classify() -> SpectralBehaviour::Muddy

Carry the certainty:
  classify() -> (SpectralBehaviour::Muddy, certainty: 0.51)
```

Then certainty flows through three layers:

```
DSP INTENSITY:
  certainty 0.95 → full correction
  certainty 0.51 → gentle, conservative correction

JINI NARRATION:
  high  → "Your track was muddy. Smoothed it into warmth."
  low   → "There might be a touch of mud — smoothed it gently,
           just in case."

CERTIFICATE:
  "94% recognized as clean speech (high confidence).
   6% flagged uncertain (music/noise/overlap) and processed
   conservatively."
```

One concept, three layers. Honest DSP, honest narration, honest
certificate.

---

## 2. Why Deterministic (Not Neural)

The entire value depends on being deterministic. This is not a
"poor man's neural" — it is the ONLY approach compatible with
certifiable audio.

| Property | Neural | Deterministic (HMM/Viterbi/FSM) |
|----------|--------|--------------------------------|
| Reproducible | No (same input → slightly different) | Yes (same input → identical) |
| Certifiable | No (can't sign non-reproducible output) | Yes (identical → identical hash) |
| Explainable | Black box | See the states, the probabilities |
| "Knows when it doesn't know" | Confidence is itself a guess | Log-likelihood below threshold = measurable "I don't recognize this" |
| Cost | GPU, heavy | CPU, streaming, bounded RAM |

The deterministic approach gives a property neural cannot: it can
say "I don't know" WITH PRECISION. If an observation doesn't match
any modeled state above a likelihood threshold, that is explicit and
measurable — not a hallucinated confidence number.

That explainable uncertainty is itself a feature of the trust model.

---

## 3. The Toolbox (Deterministic Sequence Processing)

These are not alternatives to each other — they are LAYERS of one
pipeline:

```
MFCC / spectral features        (observations — already exist)
    ↓
HMM / Viterbi                   (which sound-class, optimally,
                                 with likelihood)
    ↓
FSM                             (what phase is the voice in:
                                 silence → onset → voiced →
                                 sibilant → silence)
    ↓
Rule engine                     (what DSP to apply — with guardrails
                                 so it can't break the audio)
```

Supporting techniques from the same family: finite-state automata,
regex-over-feature-streams (a "grammar" for sounds, not text),
dynamic programming, symbolic sequence models, retrieval + rule-based
ranking.

Key point: the MFCC already exists in the system. The Markov gives
transitions. The HMM binds features to hidden sound-states. Viterbi
finds the best interpretation. The FSM executes state-dependent DSP.
Rules are the guardrails.

---

## 4. The Podcast Application: Predictive Plosive / Sibilant Lookahead

The concrete, near-term-plausible use case. This is where all the
threads come together.

### 4.1 Why it's tractable (not sci-fi)

Podcast processing does NOT need full speech recognition. It needs
far less:

```
Full speech recognition (hard):
  "which exact word/phoneme is this?"
  → needs a large model, language, context

Plosive / sibilant DETECTION (easy):
  "is the next chunk a TYPE of sound with an energy
   spike in a specific band?"
  → much simpler, nearly language-independent
```

Acoustic signatures, not linguistic knowledge:
- Plosive (p/b): sudden transient, low-frequency burst
- Plosive (t/k): sudden transient, broadband
- Sibilant (s/sh/f): energy concentration ~5–8 kHz, relatively steady

You don't need to know Greek or English to catch these. They are
acoustic patterns.

### 4.2 Why it fits the streaming architecture we already built

Predictive lookahead needs... lookahead. And the lookahead
infrastructure already exists:

- The BrickwallLimiter (Phase 8) already runs a ~5 ms lookahead
  buffer (LIMITER_FLUSH_FRAMES = 240 @ 48 kHz).
- The plosive predictor uses the SAME pattern: a small, bounded
  lookahead buffer, O(1) in RAM, streaming-safe.

So predictive de-plosive does NOT break the bounded-RAM streaming
render. It lives inside the same chunk-based, lookahead-buffer
model. It is a natural extension of episode_render, not a new
architecture.

### 4.3 The four-layer flow (for a podcast)

```
LEVEL 1 — PREDICTION (deterministic tools)
  HMM/FSM watches the voice stream.
  "Given current phoneme + transition probabilities, is a
   plosive/sibilant coming?" → with a certainty score.

LEVEL 2 — LOOKAHEAD ACTION (DSP)
  High certainty  → proactive HPF/gain before the burst lands
  Low certainty   → gentler, conservative intervention

LEVEL 3 — NARRATION (JINI)
  High → "Caught 12 plosives, tightened them clean."
  Low  → "A few spots might have been plosives — handled gently."

LEVEL 4 — CERTIFICATE (trust)
  "Plosive protection: 12 high-confidence, 3 borderline
   (conservative). Dialogue intelligibility preserved."
```

### 4.4 Anomaly detection as a free side effect

The same model that predicts phonemes also flags the impossible.
A transition with ~0 probability means:
- not voice (music, noise, beep), or
- an edit point (two takes spliced — unnatural transition), or
- a glitch/artifact

So the same model that does de-ess ALSO becomes a voice-vs-noise
detector and an edit-point detector — because all of these are the
same question: "how natural-human-speech is this transition?"

---

## 5. The JINI Connection (Today's State)

Recon of the current JINI (Phase 8):

- **Two implementations:** an LLM path (Gemma via Ollama, returns
  {narrative, action, confidence}) and a deterministic rule-based
  fallback (sp314-dsp) that runs on timeout so the user never sees
  an error.
- **Fallback confidence is hardcoded to 0.85.** It is not real.
- **jini_matrix.json** (510 lines, pre-computed strings) has three
  zones: zone_a (stage × persona), zone_b (finding × flavour ×
  persona), zone_c (platform × persona).
- **There is NO uncertainty axis.** Findings are categorical
  (Muddy / Harsh / Flat / Quiet / Perfect). Every string expresses
  100% certainty.
- JINI runs AFTER analysis and sees only the collapsed enum, never
  the underlying number.

**To make JINI certainty-aware requires:**
1. Analysis returns probabilities/certainty, not bare enums.
2. jini_matrix.json (and/or the LLM prompt) gains a certainty axis
   (e.g. Certainty::High vs Certainty::Low) with new strings.

This is a refactor of the analysis CONTRACT (many classifiers must
return (verdict, certainty) instead of verdict), not a single
touch-point. That is why this is R&D, not a current task.

---

## 6. Scope & Sequencing (Honest)

**This is NOT the current wave.** The current next step remains the
"second floor": streaming decode (decode_node still loads the whole
file — see the ignored test full_pipeline_heap_is_scale_invariant).

This spec describes where the product goes in 1–2 years, not what
gets built next.

Rough dependency order if/when pursued:
1. Certainty propagation in the analysis contract (verdict →
   (verdict, certainty)). Foundational — everything else needs it.
2. Certainty axis in JINI (matrix strings + rule engine).
3. Certificate certainty fields.
4. HMM/FSM acoustic-class predictor (plosive/sibilant).
5. Predictive lookahead DSP (de-plosive / de-ess), reusing the
   existing bounded-lookahead pattern.

Layers 1–3 are valuable ALONE (honest narration) even before the
predictor (4–5) exists.

---

## 7. Why No Competitor Has This

A competitor using neural voice processing cannot copy this,
because:
- Neural output isn't reproducible → isn't certifiable.
- Neural confidence is a guess, not a measurable likelihood.
- The trust MOAT (verifiable, deterministic, honest-about-
  uncertainty) is only reachable through the deterministic path.

The same thing that makes Markov/HMM look "old-fashioned" to the ML
world makes it NECESSARY for certifiable audio. Determinism isn't a
limitation here — it's the moat.

---

## 8. One Design Principle to Remember

> Certainty is not a feature. It is a horizontal thread that runs
> through DSP intensity, JINI narration, and the certificate.
> The system already computes uncertainty — it just throws it away
> at the enum boundary. Stop throwing it away.

---

*Exploratory spec. Emerged from Creator OS Phase 8 podcast streaming
work. Not scheduled. To be assigned a subsystem folder + ID by the
Lead Architect.*

---

## 9. Technologies & Building Blocks

Recon of the current codebase (Phase 8). The foundation is >80%
present — this is not a from-scratch build. Legend: ✓ HAVE /
~ PARTIAL / ✗ NEED.

### ✓ HAVE — MFCC & Mel-Spectrogram Extraction
Fully parameterised, tested module at
`lineos/m1/lineos-corpus/src/mfcc.rs` (`MfccAnalyzer`). Computes
Mel-band energies, applies log, uses DCT to extract MFCC
coefficients. Already used in corpus tests. This is the
"observations" layer of the HMM — already built.

### ✓ HAVE — FFT & STFT Primitives
Full spectral toolkit via `realfft` (wrapper over `rustfft`), used
heavily in `m1/xaak` and `m1/sp314-dsp`. `RealFftPlanner` and
`Complex<f32>` are solved problems. Windowing/STFT already in place.

### ✓ HAVE — Mathematical Determinism
The `m1` code is strictly deterministic. All trig/exp functions
(`powf`, `expf`, `sinf`, `log10f`, `sqrtf`) are called explicitly
via `libm::` (never std) for bit-identical ARM-vs-x86 output. New
code MUST follow this exact pattern — it is what makes the output
certifiable.

### ~ PARTIAL — Markov / State-Model Infrastructure
Strong infrastructure already exists in
`lineos/m1/lineos-corpus/src/model.rs` and `store.rs`:
`UserMarkovModel`, `TransitionMatrix`, `EmissionHistogram`. It
tracks state sequences and keeps transition counts (for the user
"footprint" / Tinder-for-Mastering learning).

**This is the key discovery:** the Markov machinery that powers the
corpus is GENERAL — it already measures transition probabilities and
emission histograms. It is exactly the substrate a phoneme/sound-
class HMM needs.

**What's missing:** the decoder. The system counts transition
probabilities but has no algorithm to infer the MOST LIKELY hidden
state from noisy observations. It reads the chain "forward"
(learning); it can't yet read it "backward" (given observations →
best hidden path + likelihood).

### ✗ NEED — Viterbi Decoder (the Certainty Engine)
To answer "how certain am I?", the system needs an HMM decoder.
Given the `libm` constraint, the cleanest path is an in-house
Viterbi decoder (~150 lines of Rust) built ON TOP of the existing
`TransitionMatrix` / `EmissionHistogram` from `lineos-corpus`.

The Viterbi log-likelihood IS the certainty signal: a best-path
score well above the alternatives → high certainty; a flat
distribution across states → low certainty; all states below a
likelihood floor → "I don't recognize this" (the explainable
"I don't know" from §2).

This single ~150-line component is what unlocks the entire
certainty-propagation principle (§1). It is the smallest, highest-
leverage piece.

### ✗ NEED — Acoustic Sound-Class Model (Plosive/Sibilant)
The HMM states for the podcast use case (§4): silence, voiced,
plosive, sibilant, fricative, breath. This is a small, hand-
specifiable state set with acoustic emission profiles (MFCC/band-
energy signatures) — NOT a trained language model. Nearly language-
independent (§4.1). Can start rule-seeded and refine emission
histograms from real audio using the existing corpus machinery.

### ✗ NEED (only if voice packs) — Backend WASM Sandbox
Only `wasm-bindgen` exists today (front-end browser target in
cockpit/loom). The backend (`m0-daemon`/`lineos`) has NO wasm
sandbox (`wasmtime`/`wasmer`). This is ONLY needed if the future
direction includes loading untrusted voice packs as sandboxed
plugins — not required for the certainty/prediction core. Deferred
until/unless voice-pack loading is on the table.

### Summary Table

| Building block | Status | Location / Action |
|----------------|--------|-------------------|
| MFCC features | ✓ HAVE | lineos-corpus/mfcc.rs |
| FFT / STFT | ✓ HAVE | realfft in m1 |
| Deterministic math | ✓ HAVE | libm everywhere in m1 |
| Transition/emission matrices | ~ PARTIAL | lineos-corpus/model.rs (no decoder) |
| Viterbi decoder | ✗ NEED | ~150 lines, in-house, on existing matrices |
| Sound-class HMM states | ✗ NEED | small hand-specified set + corpus refinement |
| Certainty types (contract) | ✗ NEED | (verdict, certainty) across analysis |
| Backend WASM sandbox | ✗ NEED* | *only if voice packs; defer |

### The Honest Takeaway
The observations layer (MFCC), the spectral toolkit (FFT), the
determinism discipline (libm), and the Markov substrate
(transition/emission matrices) all EXIST. The certainty principle
is unlocked by one small, well-scoped component — an in-house
Viterbi decoder over matrices you already have. The rest
(certainty in the analysis contract, JINI axis, certificate fields)
is plumbing that carries the signal, not new science.

>80% of the foundation is already on the shelf. This is why the
spec is R&D-plausible rather than research-hard.
