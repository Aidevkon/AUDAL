# NMFD Streaming Spec (S3) — 2026-07-31

Status: DESIGN, binding for the future Rust NMFD in the render path.
Ground truth: research/erlangen-nmfd (libnmfd 1.0.0 golden masters) +
tests/nmfd_ref (f64 reference, exactness 1.88e-15 — commit bda1f18).
Recon basis: nmf.rs fit/transform split, SlidingOverlapReader,
TwoPassEngine (65536-sample chunks, 2048/512 STFT), e2e_latency oracle.

## 1. Invariants (non-negotiable)

- **W is FROZEN in the stream.** transform() keeps its read-only
  `w: &[f32]` contract. No re-fit, ever, mid-stream — a mid-file W
  change is a glitch and a determinism break. The NMFD stays dumb,
  fast, stable; smart decisions live in the control layer above it.
- **Determinism, full doctrine:** seeded init from chaos_seed; fixed
  iteration count (no convergence checks — float compares are
  platform-sensitive branches); reductions in fixed order. Output
  feeds buses => it is INSIDE the hashed path; no exceptions.
- **Zero-latency preserved via the finalization horizon (§3), not a
  latency-declaration mechanism.** m0d is a FILE renderer: "zero
  latency" means output-file alignment (e2e_latency_compensation's
  first-1000-samples assertion), not real-time delay. The horizon
  keeps alignment intact. (If a live monitoring path ever exists, a
  latency registry becomes a real project — noted, out of scope.)

## 2. Two phases

- **W_speech / W_music: offline pre-learned 2D templates** (the anatomy of a
  consonant or sung vocal prior does not change at minute 5). Shipped as data, versioned
  (e.g., `w_speech_v1`, `w_music_v1.bin`), hashed into the certificate like chaos_seed.
  (Updated 2026-08-18: K=8 podcast baseline remains intact; K=14 music profile activates
  `w_music_v1.bin` under music UserProfile AND weighted_lean < 0.35).
- **W_noise: seeded from the scout** — the AcxCheck quietest-window
  spectrum (Y-doc §2; analyzer exposes the window position, Phase-2
  step 4). Static per file.
- **W_music / W_ambience: fitted on the 30s scout proxy** (12 kHz
  mono, existing budget ~nmf_fit 200-250ms), fixed iterations,
  seeded.
- **Escalation, not adaptation:** if the running residual/cost says
  the noise escaped its prior (fridge turns on at minute 5), the
  stream FINISHES unchanged and raises a flag; the control layer
  decides (dynamic-MCRA post-processor, or a re-run with new W).
  Never a mid-stream re-fit.

## 3. Stream mechanics

- **Transport: SlidingOverlapReader, extended — overlap >= tau_max.**
  The H update for frame t needs V[t..t+tau); equivalently, frame t's
  mask is FINAL only once V through t+tau is seen. In the
  backward-looking architecture this is a finalization horizon: when
  chunk N+1 arrives, the last tau frames of chunk N (already inside
  the reader's history) get their final masks. Output stays
  sample-aligned; no forward peeking, no LookaheadRing needed.
  tau=8 @ 512 hop = 4096 samples = ~85ms of horizon, well inside the
  existing history window.
- **Warm-start: overlap frames ONLY.** Their H values from the
  previous chunk seed their recomputation (this is where seams live
  and where clicks die). NEW frames get fresh seeded init. Full-file
  H carry-over is deliberately rejected v1: it chains every chunk to
  all predecessors (accumulating drift, NaN contagion, uncharacteriz-
  able acceptance). If the seam measurement (§5) demands it, full
  warm-start is the next MEASURED step.
- **Iteration budget: fixed 15-20 per chunk** (blueprint), same count
  in overlap and new regions. Constant recorded in the certificate.
- **Memory: O(1)** — ring of tau_max finalization state + per-chunk
  H; must fit INV-ST-3 (<= 50MB) alongside existing buffers.

## 4. Parallelism

Existing shape survives: **parallel across K rows** (temporal
tau-convolutions live WITHIN a row — no cross-row writes, no races).
Determinism cost is accumulation order only: **pairwise fixed-order
sums within each row** — the numpy scheme the S2 oracle measured and
the reference already implements. K=4 keeps this cheap; measure how
much of the 11.9x survives, don't assume.

## 5. Acceptance

- **The Erlangen oracle runs in chunked mode vs whole-file mode** on
  the golden fixture: seam drift is measured and characterized, not
  assumed zero (boundary effects are intrinsic). The spec's gate
  shape follows S2's three-role pattern: exactness where exactness is
  possible (interior frames far from seams), drift pins at seams.
- **Determinism pin:** chunked render twice => identical H bytes.
- **e2e_latency_compensation stays green untouched** — the horizon
  scheme's proof of alignment.

## 6. Non-goals (decided elsewhere, do not reopen here)

- Transient bypass lives OUTSIDE the NMFD (time-domain, pre-mask,
  routed by the Y branch — ratified). The math engine has no
  "is this a consonant?" branches.
- MCRA is a future post-processor behind the escalation flag, not a
  stream component (Y-doc §2).
- The duck curve is a recorded control input (Y-doc §1.5), produced
  from VAD posteriors (F-061: narration-grade today), never computed
  inside the render.
