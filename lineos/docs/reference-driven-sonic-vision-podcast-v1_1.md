# SPEC: Reference-Driven Sonic Vision — Podcast Pilot
# lineos/docs/reference-driven-sonic-vision-podcast-v1_1.md
# Version: 1.2
# Date: 2026-07-01
# Status: 📋 SPEC — Awaiting Lead Architect approval (§7 gate)
# Owner: Lead Architect (Anestis) / Strategist: Claude
# Authority: Development Protocol v1.0 §7 (Spec-First), PHILOSOPHY.md v1.0
# Roadmap: Section 2 — DSP SCOPE 🟡 NEAR (new entry: 2.6)
# Fix Level: 3 (Architectural — new resolver, new data contract, no core-algorithm change)

---

## 0. TL;DR

Today the "character" of a master (the Sonic Vision layer — the aesthetic
that makes a podcast sound *right*, not just *legal*) is produced by
hardcoded threshold numbers scattered across the analysis and aether-bridge
layers. Those numbers are guesses.

This spec replaces the guesses **for the podcast pilot** with a
**Reference Profile** whose every value is sourced from a published,
citable standard or peer-reviewed dataset (ITU-R BS.1770-5, EBU R128, the
Byrne et al. 1994 LTASS). A new deterministic `ReferenceResolver` sits
beside the existing `SemanticZoneResolver` in the aether-bridge and emits
EQ / dynamics corrections that close the measured distance between the input
and the reference target, using a **Spectral Balance Ratio (SBR)** metric
whose math is drawn from published patent + academic literature.

No ML. No black box. No commercial-master scraping. The certificate proves
the output honours published standards; the exact craft rules (weights,
correction curves, ordering) stay proprietary ("in dark, not hidden").

**v1.1 adds:** pinned primary citations (URLs/DOIs), the numeric LTASS
reference curve, the precise SBR definition, the deterministic matching-EQ
algorithm, and two critical corrections (SPL-vs-dBFS handling; the Byrne
band-range limit).

---

## 1. ADR — Architecture Decision Record

This section is the locked decision. The rest is the method that implements it.

### 1.1 Context

The industry standard for "automatic filter tuning" is **neural / ML** —
train a network on millions of (raw, mastered) pairs and infer moves (LANDR,
iZotope, the "top 8"). This is a black box: non-reproducible, non-certifiable,
unable to sign its own output.

Creator OS already performs **rule-based auto-tuning** (pre-analysis →
deterministic conditions → DSP config) — we simply had not named it that.
The open question: how do we aim those rules at a professional target without
adopting ML and without breaking the determinism / certificate contract?

### 1.2 Decision

**We do NOT adopt ML.** The Sonic Vision target is defined by a
**Reference Profile** sourced from published standards + peer-reviewed data,
and the existing deterministic rule engine is extended to aim at it.
Determinism is the differentiator: no neural competitor can cryptographically
sign a reproducible output. We can.

### 1.3 Decisions locked

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Pilot = Podcast** | Spoken word has rich *published* characteristics (LTASS); reuses today's O(1) streaming Episode path (PR #38). |
| D2 | **Target = standards/peer-reviewed**, not commercial scraping | Reproducible, copyright-clean, citable. Scraping is legally grey and non-deterministic. |
| D3 | **Reference lives in the Sonic Vision spectral baseline** | Platform (Layer 1) = law (untouched). Flavour (Layer 3) = spatial/dynamic paint (untouched). Intent knobs = interpolation around the reference. Reference defines *spectral character* only. |
| D4 | **Method = rule-based planning**, not iterative ML convergence | Deterministic, explainable, one-pass. Measured delta → defined correction. |
| D5 | **Reference Profile = published, citable numbers** | Every value has a source (ITU/EBU/JASA/patent) or is explicitly flagged as an internal design choice. |
| D6 | **No analyzer tool for the podcast pilot** | Podcast target is already published (LTASS + BS.1770). Offline analyzer deferred to non-standard flavours (cinematic/club). → BACKLOG. |
| D7 | **Oracle-TDD** (Python oracle → fixture → Rust contract) + independent FFmpeg loudness cross-check | Dev Protocol §4. FFmpeg = independent ear (PHILOSOPHY.md). |
| D8 | **Offline Python/`tools/`; shipped rules Rust/`libm`/`src/`** | Dev time full-armored, run time constitutional (PHILOSOPHY.md; boundary is `src/`). |
| D9 | **Whitepaper updated in-repo with pinned citations** | "Trust the math" is backed by named published sources, not assertion. |
| D10 | **Two-tier disclosure: public standards floor + proprietary craft ("in dark, not hidden")** | Standards are the public trust floor; correction curves/weights/ordering are proprietary but deterministic + internally documented — never a black box. |
| D11 | **Fix Level 3, spec-first** | New resolver + data contract; no core-algorithm/constitutional change. Approve, then implement. |

---

## 2. Problem Statement (Dev Protocol §2 — WHAT)

The Sonic Vision layer answers "what character should this master have?"
Today that is encoded as **hardcoded threshold constants** across:

- **L1 — analysis** (`sp314-dsp/src/analysis/`): boolean tonal-zone flags
  vs fixed magic numbers (`zone_cymbal_harsh = profile[band] > FIXED_DB`).
- **L2 — aether-bridge** (`shared/aether-bridge/src/lib.rs`):
  `SemanticZoneResolver::auto_carve()` reads flags → emits EQ `zone_bands`.
- **L3 — DSP** (`sp314-dsp/src/masking_eq/`): real-time stem-aware clamps on
  permitted cut/boost per block.

The magic numbers were chosen by intuition. There is no defensible answer to
"why that threshold?" This spec removes that weakness for the podcast pilot.

---

## 3. Proposed Solution — The Method (Dev Protocol §2 — WHY / HOW)

### 3.1 The Reference Profile (podcast pilot)

A `ReferenceProfile` is a small, versioned set of target metrics in the same
band language the analysis layer speaks. Every field is sourced:

| Metric | Podcast target | Source (see §13) |
|--------|----------------|------------------|
| Integrated loudness | −16 LUFS (Apple) / −23 LUFS (EBU delivery) | ITU-R BS.1770-5; Apple art.893; EBU R128 |
| True peak ceiling | ≤ −1.0 dBTP (−3 dBTP for lossy) | ITU-R BS.1770-5 (4× oversampling true-peak) |
| Loudness gating | absolute −70 LUFS; relative −10 LU | ITU-R BS.1770-4; EBU R128 |
| Metering windows | momentary 0.4 s; short-term 3.0 s | EBU Tech 3341 (EBU Mode) |
| Loudness Range (LRA) | **measured** parameter; dialogue typically ~5–8 LU | EBU Tech 3342 (LRA is measured, not a hard spec) |
| Spectral shape (tilt) | **universal LTASS shape**; tilt ≈ −3 to −5 dB/oct above ~800 Hz | Byrne et al. 1994 (JASA); tilt from LTASS-derived analyses |
| Noise floor | < −60 dBFS (pauses) — **internal design choice**, no international standard | (flagged: practice, not a codified spec) |

> **CORRECTION 1 — SPL vs dBFS (critical).** The LTASS values (§3.3) are in
> **dB SPL**, normalised to 65 dB SPL overall — an *acoustic* reference. The
> DSP operates in **dBFS** (digital). We therefore use LTASS for the
> **relative SHAPE** (band-to-band balance / tilt), NOT for absolute levels.
> The SBR (§3.2) is a *ratio*, so the absolute reference level cancels — an
> upper/lower ratio is identical in SPL or FS. Absolute loudness is governed
> entirely by the hard LUFS constraint, never by LTASS levels.

> **CORRECTION 2 — Byrne band range.** The open-access Byrne row (Table III,
> §13) publishes one-third-octave levels only up to **2520 Hz**. The upper
> SBR band therefore uses Byrne data up to ~2.5 kHz. Any extension above
> 2.5 kHz (toward the 8 kHz "air" region) MUST be labelled as a supplementary
> source (original Byrne tables via JASA, or the modern comparison study's own
> ≥3 kHz LTASS) — never silently interpolated.

**Absolute vs relative.** Absolute targets (LUFS, TP) are already Oracle-TDD-
verified in the DSP; the reference treats them as **hard constraints**. The
spectral shape (tilt / band balance) is the **soft target** the rules aim at,
*within* the hard constraints. Constrained optimisation, not free tuning.

### 3.2 The distance metric — Spectral Balance Ratio (SBR)

Let `X(f)` be the magnitude spectrum of the input and `H(f)` that of the
reference. Define two disjoint bands (aligned to LTASS CFs):

- Lower band `B_L` = 250–1000 Hz (body / core)
- Upper band `B_U` = 2000–2520 Hz (presence; upper bound limited by Byrne
  data per Correction 2 — extendable toward 8 kHz only with a supplementary
  source)

Band energies (linear):

```
E_L(X) = Σ_{f∈B_L} |X(f)|²      E_U(X) = Σ_{f∈B_U} |X(f)|²
E_L(H) = Σ_{f∈B_L} |H(f)|²      E_U(H) = Σ_{f∈B_U} |H(f)|²
```

Spectral Balance Ratio (dB form):

```
R_dB(X) = 10·log10( E_U(X) / E_L(X) )
R_dB(H) = 10·log10( E_U(H) / E_L(H) )
```

Relative spectral-balance error:

```
ΔR_dB = R_dB(X) − R_dB(H)
```

`ΔR_dB > 0` → input is brighter than the reference; `< 0` → darker;
`≈ 0` → tilt matches. Because SBR is a ratio, the absolute reference level
cancels (this is what makes the SPL→FS use in §3.1 valid).

*Source:* band-energy spectral-balance comparison per patent WO2007120453 A1;
generalised envelope matching per the INRIA asymmetrical spectral-envelope
matching report (§13).

### 3.3 Numeric LTASS reference curve (the spectral target)

Universal LTASS (Byrne et al. 1994), one-third-octave band levels normalised
to 65 dB SPL overall, as tabulated in the open-access comparison (§13, Table
III). Values used for the **shape** target (dB SPL — see Correction 1):

| CF (Hz) | dB SPL | CF (Hz) | dB SPL |
|--------:|-------:|--------:|-------:|
| 79 | 38.5 | 500 | 57.1 |
| 99 | 49.4 | 630 | 55.5 |
| 125 | 52.7 | 794 | 51.8 |
| 157 | 51.8 | 1000 | 48.7 |
| 198 | 55.2 | 1260 | 48.0 |
| 250 | 55.3 | 1587 | 47.0 |
| 315 | 54.0 | 2000 | 43.7 |
| 397 | 57.1 | 2520 | 43.1 |

Observations that anchor the target: energy peaks in the low-mids
(~400–630 Hz), then rolls off toward the highs — consistent with the
published −3 to −5 dB/oct tilt above ~800 Hz. These band values populate
`spectral_target_db` in `podcast-v1.json` (as a normalised shape, per
Correction 1). Bands above 2520 Hz are added at implementation only with a
labelled supplementary source (Correction 2).

### 3.4 The deterministic matching-EQ algorithm (the craft layer)

Given input and reference spectra, derive per-band gains that move the input
toward the reference shape — no ML.

1. **Analyse.** STFT → time-averaged envelopes `X̄(f)`, `H̄(f)`
   (power average, α=2). Partition into one-third-octave bands at LTASS CFs.
2. **Band levels.** `L_k(X) = 10·log10(Σ_{f∈B_k} X̄(f)²)`, same for `H`.
3. **Per-band gain.** `G_k = L_k(H) − L_k(X)` dB (boost if >0, cut if <0),
   clamped `|G_k| ≤ G_max`.
4. **SBR constraint.** Compute `ΔR_dB` (§3.2); bias upper/lower band gains so
   post-correction SBR moves toward `R_dB(H)`, within the per-band clamps.
   Optionally asymmetrical (penalise "too bright" more than "too dark") per
   the INRIA metric.
5. **Ordered passes (coupled params).** Apply tonal target first, then
   dynamics, then loudness compliance — each pass sees the previous. Closer to
   how an engineer works (tone → dynamics → loudness); more deterministic than
   independent knobs.
6. **Filter design.** Map `{(f_k, G_k)}` to peaking/shelving EQ at band CFs, or
   fit a smooth parametric curve. Feed as `zone_bands` into the existing
   `DspConfig` path.

**The proprietary boundary.** The *shape* of this algorithm is public (above).
The **exact `G_max`, per-band weights, the SBR-bias solver, the asymmetry
factor, and the pass ordering constants** are the proprietary craft layer —
deterministic, Oracle-TDD-tested, internally documented, but not published.
"In dark, not hidden": we prove the *result* complies with public standards
without exposing the *recipe*.

### 3.5 Where the control layers sit (no ambiguity)

| Layer | Role | Touched by reference? |
|-------|------|-----------------------|
| Platform / Target (Layer 1) | LUFS/TP law | ❌ hard constraint, untouched (Oracle-TDD) |
| Sonic Vision spectral baseline | tonal character target | ✅ **the reference lives here** |
| Intent knobs (Layer 2) | user steering | ↔ interpolates *around* the reference |
| Flavour (Layer 3) | spatial/dynamic paint (width, LFE, ducking) | ❌ independent, untouched |

> **Platform/content conflict — resolved upstream, not here.** When a platform
> choice and the detected content imply different loudness targets (e.g.
> Spotify-*music* ~−14 LUFS vs podcast −16 LUFS, Apple art.893), the conflict
> is resolved **before** the resolver runs: scout-driven JINI platform
> filtering offers only platforms valid for the detected content, so invalid
> combinations never arise and no in-resolver resolution rule is needed. The
> podcast character (LTASS reference) is **platform-agnostic** — the same
> reference applies regardless of destination; the platform contributes only
> the LUFS/TP hard constraint. See `notes/scout-driven-platform-filtering.md`
> (separate JINI-domain work).

---

## 4. Architectural Fit (Dev Protocol §7.3)

- **Crate:** `shared/aether-bridge` (planning layer — reference resolution is
  *planning*, consistent with "intelligence in planning, processing in DSP").
- **New module:** `shared/aether-bridge/src/reference_resolver.rs`
- **Insertion point:** same call site as `SemanticZoneResolver::auto_carve()`;
  emitted `zone_bands` / dynamics merge into the existing `DspConfig` build
  (`IntegrationFirewall::build`) — nothing downstream changes shape.
- **Reference data:** `shared/schema/reference-profiles/podcast-v1.json`
  (read-only, checked in), loaded via `include_str!` — no runtime file/network
  (constitutional).
- **No change** to L3 real-time clamps, the limiter, or any core algorithm.
  The resolver only *proposes*; hard constraints + stem-aware clamps still
  bound the result.

---

## 5. Data Contracts (Dev Protocol §7.4)

```rust
/// Published, citable target character for one Sonic Vision preset.
/// Absolute fields are HARD constraints; spectral fields are the SOFT target.
/// Spectral values are a normalised SHAPE (LTASS is dB SPL, not dBFS — see
/// spec §3.1 Correction 1). N_BANDS + layout to be locked against the real
/// PreAnalysisData during implementation recon (§10.1).
pub struct ReferenceProfile {
    pub id: &'static str,             // "podcast-v1"
    pub schema_version: u32,          // bump on any target change
    pub provenance: &'static str,     // citation keys (§13)

    // HARD constraints (Oracle-TDD verified in DSP; resolver never violates).
    pub target_lufs: f32,             // -16.0 (Apple) / -23.0 (EBU)
    pub true_peak_ceiling_dbtp: f32,  // -1.0
    pub gate_absolute_lufs: f32,      // -70.0
    pub gate_relative_lu: f32,        // -10.0

    // SOFT target — normalised SHAPE, not absolute levels.
    pub spectral_target_db: [f32; N_BANDS], // LTASS shape @ analysis band CFs
    pub spectral_tilt_db_per_oct: f32,      // ~ -4.0 (range -3..-5), >800Hz
    pub sbr_lower_band_hz: (f32, f32),      // (500, 1000)  = [3] Mid-Low
    pub sbr_upper_band_hz: (f32, f32),      // (1000, 2000) = [4] Mid-High
    // SBR compares the speech articulation zone
    // (1-2kHz) against the mud zone (500-1kHz) —
    // the delta that matters most for podcast voice.
    // Byrne data covers both bands fully (Table III
    // publishes up to 2520Hz). Correction 2 of
    // spec §3.1 no longer applies to the SBR bands
    // themselves (both within Byrne range).
    pub lra_target_lu: f32,                 // dialogue ~5-8 (measured, soft)
    pub noise_floor_dbfs: f32,              // -60.0 (internal design choice)
}
```

Contract rules:
- `spectral_target_db` MUST use the identical band layout the analysis layer
  emits (locked during recon) — otherwise deltas are meaningless.
- Spectral values are a **normalised shape** (Correction 1); absolute loudness
  is owned solely by the LUFS hard constraint.
- Profile is **read-only data**, not code. Changing a target = data change
  (schema_version bump), not logic rebuild.
- Every profile carries a **provenance block** (citation keys + schema_version)
  so the certificate can state which source/version each target came from.

---

## 6. Test Gate (Dev Protocol §4 — Oracle-TDD Triangle)

```
Python Oracle (scipy/numpy, tools/)  →  computes band levels, SBR, and the
                                          expected per-band gains from a
                                          fixture input + reference
        ↓ generates
Fixture (JSON, committed)            →  input envelope + reference + expected
  tests/fixtures/reference/…            gains + expected post-SBR
        ↓ validates
Rust Contract Test (aether-bridge)   →  ReferenceResolver reproduces the gains
                                          bit-close (libm)
```

Independent verification (PHILOSOPHY.md — not self-validation; see also `lineos-dsp-pipeline-compliance-v1_0.md` for the full empirical proof of LUFS/TP/phase compliance that underpins the hard constraints here):
- **FFmpeg loudnorm cross-check:** after a reference-tuned render, FFmpeg's
  EBU R128 meter agrees with our LUFS within ±0.2 LU; TP ≤ −1 dBTP.
- **SBR convergence:** post-render `|ΔR_dB|` within tolerance of 0 (output
  tilt matches reference) AND all hard constraints hold.
- **Regression guard:** fixture set runs in CI; any DSP change that shifts the
  reference match is caught (§4 regression benefit).

Test invariants: see §11.

---

## 7. Constitutional Compliance (Dev Protocol §7.6 / PHILOSOPHY.md)

| Rule | Compliance |
|------|-----------|
| Pure Rust in `src/` | ✅ resolver pure Rust; Python only in `tools/` (oracle) |
| `libm` for math | ✅ all resolver math via libm (log10/pow); no `std::f32` methods |
| No subprocess/network in `src/` | ✅ profile via `include_str!`; FFmpeg test-only |
| Deterministic x-platform | ✅ rule-based, libm, no rand |
| No ML weights in M1 | ✅ zero ML — the whole point (ML origin rule) |
| MIT/Apache2 licenses | ✅ no new runtime deps; scipy/numpy/FFmpeg dev-time only |
| CI gates | ✅ adds no runtime deps that fail `cargo deny` |

FFmpeg fixtures (if any) → `tests/fixtures/` with README documenting exact
command + version (PHILOSOPHY.md).

---

## 8. The Whitepaper Update (D9 + D10)

Two tiers:
- **Tier 1 — Public foundation (citable):** targets + their sources (§13).
  Anyone can verify the *targets* are legitimate.
- **Tier 2 — Proprietary craft ("in dark, not hidden"):** the existence and
  determinism of the correction rules is stated; the exact `G_max`, weights,
  SBR-bias solver, asymmetry, and ordering are NOT published. The certificate
  proves the *result* complies with Tier 1 without exposing the Tier 2 recipe.

Explicit competitor distinction: neural tools are *hidden* (even authors can't
reproduce a given output); Creator OS is *in dark* (fully deterministic,
internally documented, Oracle-TDD-proven — simply not given away).

---

## 9. Scope Boundary (what this spec does NOT do)

- Does **not** build the offline analyzer tool (→ backlog; only for
  non-standard flavours like cinematic/club without a published target).
- Does **not** touch the Music path, the limiter, or any L3 real-time clamp.
- Does **not** implement per-episode ML adaptation (explicitly rejected).
- Does **not** change platform (Layer 1) or flavour (Layer 3) behaviour.
- Covers **one pilot profile** (podcast). The mechanism generalises; each new
  profile is its own data + citations.

---

## 10. Implementation Order (post-approval)

1. **Recon** the exact `PreAnalysisData` band layout + `auto_carve` call site;
   lock `N_BANDS` and field names for the real contract.
2. Write `podcast-v1.json` (LTASS shape from §3.3, hard constraints, provenance).
   Add ≥3 kHz bands only with a labelled supplementary source (Correction 2).
3. Python oracle in `tools/` → generate committed fixtures (band levels, SBR,
   expected gains).
4. `ReferenceResolver` in `aether-bridge` (rule-based, libm, ordered passes,
   proprietary constants).
5. Rust contract tests (INV-REF-1..6) + FFmpeg cross-check.
6. Wire resolver into `DspConfig` build (behind the podcast profile).
7. Whitepaper two-tier section + roadmap entry 2.6.
8. Each step atomic, green before commit (Dev Protocol §1, §5).

---

## 11. Invariants

| ID | Invariant |
|----|-----------|
| INV-REF-1 | Reference tuning never violates a hard constraint (LUFS/TP/gating) |
| INV-REF-2 | Post-render `|ΔR_dB|` within tolerance (output moved toward reference tilt) |
| INV-REF-3 | Reference tuning is deterministic — same input → same corrections |
| INV-REF-4 | FFmpeg EBU R128 cross-check agrees within ±0.2 LU |
| INV-REF-5 | Every reference target carries a source (published citation OR explicit "internal design choice" flag) — no unattributed magic numbers |
| INV-REF-6 | No ML anywhere in the reference path (ML origin rule) |
| INV-REF-7 | Spectral target used as normalised SHAPE only; absolute loudness owned by the LUFS constraint (Correction 1) |

---

## 12. Roadmap Position

```
🟡 NEAR — Section 2.6 (new)
Opens after: ✅ O(1) Streaming Podcast Pipeline (PR #38)
Depends on:  aether-bridge SemanticZoneResolver ✅ ; pre_analysis spectral profile ✅
Pilot:       podcast (standards + LTASS, no tool)
Generalises: cinematic / club / openair (reference-based, needs tool → backlog)
```

---

## 13. Primary Source Register (Tier 1 public foundation)

Each target value maps to exactly one primary source. Paywalled items are
flagged; items with no codifying standard are flagged as internal choices.

**Loudness & true-peak**
- **ITU-R BS.1770-5** (2018), *Algorithms to measure audio programme loudness
  and true-peak audio level*. K-weighting (pre-filter + RLB), LFE excluded,
  true-peak via 4× oversampling. Landing: `https://www.itu.int/rec/R-REC-BS.1770`
  (select -5). Version history: -1 (2007), -2 (2011, gating via BS.1771),
  -3 (2012), -4 (2015, 4× TP), -5 (2018).
- **EBU R128 v5.0** (2023), *Loudness normalisation and permitted maximum
  level*. −23 LUFS, ±1 LU, TP ≤ −1 dBTP (−3 for lossy). Base PDF:
  `https://tech.ebu.ch/docs/r/r128-2014.pdf` (v5.0 preserves base values).
- **EBU Tech 3341** (EBU Mode metering): momentary 0.4 s, short-term 3.0 s,
  integrated gated (−10 LU relative). **3342** (Loudness Range — measured
  parameter, not a hard max). **3343** (production). **3344** (distribution).
  `https://tech.ebu.ch/docs/tech/tech3343.pdf` and the tech.ebu.ch index.
- **Apple Podcasts** *Audio requirements* (article 893): −16 dB LKFS ±1 dB,
  true-peak ≤ −1 dBFS, referenced to BS.1770-5.
  `https://podcasters.apple.com/support/893-audio-requirements`
- **Spotify / YouTube:** *no primary numeric spec* — Spotify's official doc
  describes normalisation behaviour, not a hard LUFS target (−14 is a widely
  observed operating point, not a standard); YouTube publishes no numeric LUFS
  spec. FLAGGED: not cited as authority.
- **AES SC-02-01:** true-peak aligns with BS.1770's oversampled definition;
  the AES text is paywalled. We cite **BS.1770-5** for true-peak.

**Speech spectral character**
- **Byrne D. et al.** (1994), *An international comparison of long-term average
  speech spectra*, JASA 96(4):2108–2120. DOI: **10.1121/1.410152**. Canonical
  LTASS (10 M + 10 F talkers, 12+ languages, one-third-octave, dB SPL).
- **Open-access LTASS comparison** (NCBI/PMC), *Table III* republishes the
  Byrne row normalised to 65 dB SPL — the numeric curve used in §3.3.
  `https://pmc.ncbi.nlm.nih.gov/articles/PMC11540443/table/t3/`
  FLAGGED: Byrne row shown to 2520 Hz; ≥3 kHz needs original Byrne (JASA) or
  the comparison study's own LTASS, labelled as supplementary (Correction 2).
- **Speech spectral tilt ≈ −3 to −5 dB/oct above ~800 Hz**, derived from Byrne
  LTASS (later analyses). Deterministic test tilts (±6 dB/oct, pivot 1 kHz,
  0.25–4 kHz) per ASHA JSLHR/JSHR shaping studies. Formula:
  `level_shift(f) = slope · log2(f / pivot)`.
- **IEC 60268-16:2020** (STI/STIPA male speech test spectrum) — paywalled;
  optional supplementary source for standardised test spectra.
- **ITU-T P.501 (04/2025)** — speech-like test signals (not a canonical LTASS
  table). `https://www.itu.int/rec/T-REC-P.501-202504-I/en`

**Noise floor**
- *No international standard* codifies a numeric dBFS noise-floor pass/fail for
  speech. The < −60 dBFS target is an **internal design choice informed by
  practice**, explicitly flagged as such (INV-REF-5).

**Spectral-balance / matching method**
- **WO2007120453 A1** (2007), *Calculating and adjusting the perceived
  loudness and/or the perceived spectral balance of an audio signal* — band-
  energy accumulation + reference comparison + band-gain correction (basis for
  SBR and matching-EQ). `https://patents.google.com/patent/WO2007120453A1`
- **INRIA report hal-00945296** (2014), *Robust similarity metrics between
  audio signals based on asymmetrical spectral envelope matching* — deterministic,
  non-ML envelope matching (basis for the asymmetry option in §3.4).
  `https://inria.hal.science/hal-00945296v1/document`

*(Published target values are facts, not reproductions of standard text.
Exact URLs/versions to be re-verified and pinned in the whitepaper at
implementation time.)*

---

## 14. Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-07-01 | Initial spec — ADR + method locked, awaiting §7 approval |
| 1.1 | 2026-07-01 | Pinned primary citations (BS.1770-5=2018, Apple 893, Byrne DOI, WO patent, INRIA); numeric LTASS curve (§3.3); precise SBR definition (§3.2) + matching-EQ algorithm (§3.4); Correction 1 (SPL vs dBFS); Correction 2 (Byrne band range); LRA reclassified as measured (not hard); Spotify/YouTube de-listed as authority; noise floor flagged internal; INV-REF-7 added |
| 1.2 | 2026-07-02 | N_BANDS upgraded 6→8 (surgical speech EQ, No Compromise decision). SBR bands updated: lower=(500,1000)=[3]Mid-Low, upper=(1000,2000)=[4]Mid-High. Both within Byrne data range — Correction 2 no longer applies to SBR bands. |

---

**Lead Architect:** Anestis
**Strategist:** Claude
**System:** Creator OS
**Document:** `lineos/docs/reference-driven-sonic-vision-podcast-v1_1.md`
**Version:** 1.2
**Status:** 📋 SPEC — Awaiting approval

---

*Standards are the public floor. Craft is the proprietary edge.*
*In dark — not hidden.*
