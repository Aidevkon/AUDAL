# Audal — PRD v7.2

2026-09-18 · @Someone

## Project: Audal — Offline Audio Delivery Compliance

**Version:** 7.2 · **Status:** Draft for review · **Supersedes:** 7.1

**Name:** "Audal" is the working name; it becomes final on a clean TMview/EUIPO check in classes 9 and 42. audal.com is registered. The B2B edition's name is open (Q2) and must not read as a tier.

**Target users:** One app, one promise, three roles. Audio practitioners who deliver on behalf of others (primary — resolves ties); ACX narrators; independent podcasters. **Studio edition:** post-production and localization vendors, legal transcription, healthcare audio.

### What changed from v7.1

Six measurements taken 2026-09-17 against the live tree. The product strategy is unchanged and no decision is reversed.

- **Stage ordering is settled by measurement, not by argument.** Spacing conform moves whole-file level by at most 0.043 dB; channel conform moves it by 1.3 to 6.5 dB on decorrelated material. Stage 12 therefore runs before the level measurement that stage 13 consumes; stage 11 may run either side (R5b).
- **Stage 7 is declared last resort.** It lowers the noise floor, which is the activation criterion of stages 1, 2 and 3.
- **The non-convergence message names the conflicting criteria from the rows** rather than asserting one cause, since the conflict may be between two spec criteria or between the pre-encode measurement and the encoded file (F-077).
- **Goal 3 regains the qualifier v7.0 had and v7.1 dropped:** acceptance is observed, never measured directly, because the platform returns no values.
- **Three citations corrected.** The voice/music classifier fell in F-108 and F-110; F-114 measured the two voice detectors. Different instruments, different findings.
- **The engine has no mono path** (F-117) — recorded in R9, since it shapes what the channel guarantee means.
- **The segmenter and its classifier are named in the prune** (§6.4) and in FUTURES: the delivery chain has no segment boundaries.

  **The destination's requirements page was read at the source and sealed in the tree** (F-119). Three consequences: spacing conform trims and never pads, since the requirement is an upper bound only; channel conform keeps the input's layout wherever the set already agrees, since the page asks for consistency rather than mono; and K0 gains a second question — does the stage add or destroy — which is what puts trimming behind approval.

### Governing principles

**D11.** The PRD states what must be true; the code reports how far it is; when they disagree the code changes — unless the requirement was impossible or self-contradictory, in which case the requirement changes by recorded decision. Code that exists but serves no persona earns no requirement by existing.

**R0, extended.** Every number on screen and in a statement is a measurement of the current file, and every sentence to the user is a **fact of the signal, not an attribution** — "level drops 7 LU at 12:40 and returns at 14:05", never "speaker B is 7 LU below A".

## 1 · Problem statement

Audio delivered to a platform, a rights holder, a court or a client must pass rigid, destination-specific technical gates. The people who run these delivery passes — freelance and in-house practitioners, and the narrators and podcasters who do it themselves — either do it by hand in a DAW, pay per-file for cloud services, or submit blind and get rejected.

Two costs compound. **Rejection cost:** the platform names the violated rule but not the measurement or the file, so a thirty-chapter rejection becomes a manual search with review windows of up to ten business days per resubmission. **Privacy cost:** for a growing share of this work, uploading audio to a third-party server is a breach — TPN obligations, attorney-client privilege, health-data law. For these users cloud tools are not inconvenient; they are unavailable.

### 1.1 The specific gap — measured, not assumed

The platform's automated rejection **names the failed specification but not the measurement**: *"The RMS level of your files is too low"* — the rule, without the value and often without the file. Free checkers exist. So the gap is:

1. Which of my thirty chapters, and by how much.
2. Bring it into spec, not just tell me it is out.
3. Consistency across the title — required by the platform, evaluated by no checker.
4. **Proof a third party can verify — produced by no competing product.**

*Evidence: rejection wording from public narrator forums and 2025–2026 guides; review window and consistency requirement from the platform's published submission requirements, retrieved 2026-09-06. Rejection-rate statistics could not be traced to a primary source and are not used. Interviews open — §9.*

### 1.2 Where privacy is a requirement, not a preference

Filter for the Studio market: **uploading audio to a cloud service constitutes a contractual or legal breach.** Segments, in order of fit: post-production and localization on pre-release content (TPN) → legal transcription → healthcare → newsrooms → air-gapped public sector. Beachhead: the first. *Validation pending — Q10.*

## 2 · Goals

**User goals**

1. A practitioner delivers a compliant, proven title for a client in **one pass**: drop, run, hand over outputs and signed statements.
2. A first-time narrator or podcaster gets a compliant file in **under 3 minutes**, no manual tuning.
3. Files marked compliant are accepted by the destination's check **≥ 99%**, measured against a validated corpus and reported beta outcomes. The platform returns no values, so acceptance is observed, never measured directly.
4. A 100-file title (\~12 h) completes on a mid-range laptop in **under 15 minutes**, UI responsive throughout.
5. Zero bytes of audio leave the machine — verifiable by the user.
6. The user sees at a glance which files passed, were corrected, cannot be corrected, and by how much each measurement missed.
7. **A third party verifies the signed statement without this application.**

**Business goals**

8. Default "final step before delivery" among freelance practitioners within 12 months.
9. **Three paid Studio pilots** within 6 months of consumer launch.

## 3 · Non-goals

| Non-goal | Rationale |
| --- | --- |
| Editing / DAW features | We are the step after the bounce, not the editor. |
| **Mixing — balancing two signals against each other** (ducking, same-stem speaker balancing, stem summing, separation-based processing) | K0. Correction moves one signal toward one criterion; mixing balances two signals. Mixing is the bounce, and belongs to the console line (§6.5). |
| Creative mastering — Warmth/Air/Punch, reference and EQ targets, "taste" | Opposed to "change the minimum and prove it". Console line. |
| **Content detection at drop** | The voice/music classifier fell (F-108, F-110). Container probing is measurement and stays; the user chooses the profile. |
| Restoration as a category (de-reverb, spectral repair, ML enhancement) | Fails K1/K2. The word "restoration" does not appear in the product; the statement lists *corrections applied*. |
| File transfer / delivery platform | Contradicts the offline guarantee; competes with channels buyers have certified. |
| Direct upload to any platform | Requires a network stack. |
| LLM narrative / personas | Non-deterministic text is a liability in a proof product. Guidance is deterministic templates. |
| VST/AU/CLAP hosting · Mobile and web · Cloud sync, accounts, telemetry · Subscription pricing | As v7. |

## 4 · Personas, stories, journey and market

### 4.1 Personas

**P1 — The Practitioner** (primary). Delivers for 2–5 clients with different specs; bills by the hour; knows LUFS and noise floor; owns a DAW; has clients who forbid cloud; influences tool choice without holding budget. The signed statement is proof of work for them and chain of custody for their client.

**P2 — The Narrator.** 20–40 chapters per title; needs a green light and a file that passes.

**P3 — The Podcaster.** Weekly; per-track files from remote guests; needs consistent loudness and a saved profile. Gets *delivery* from Audal; gets *sound* from the console line.

**P4 — The Studio Buyer.** Approves rather than operates; needs an auditor-acceptable answer to "where does the audio go", a proof artefact the QC pipeline reads, a site licence.

**Removed:** the bedroom musician (D6).

"Primary" means: when two roles want opposite things from one screen, the practitioner wins; and the practitioner is the path to Studio. It does not mean "only".

### 4.2 User stories

As v7 §4.2, with three changes: the podcaster story "music bed ducked under speech" is **removed** (K0); a podcaster story "per-track files from remote guests are leveled to one target as a set" is **added** (stage 13); a practitioner story is added — "the input's timecode, scene/take and other metadata are in the output".

### 4.3 User journey — today vs target

*Measured 2026-09-06, 09-08, 09-09.*

| # | Step | Today | Target |
| --- | --- | --- | --- |
| 1 | Drops 30 chapters | Keeps the first, discards 29 | All 30 in a queue |
| 2 | Sees what was understood | "Album. N tracks." on a fixed timer | 30 rows with container facts (channels, rate, duration) — **no content detection** |
| 3 | Picks a destination | Six offered; everything the UI triggers goes to `/master/streaming`, which has **no limiter and no target**; only the ACX export enforces spec afterwards | Profile per title from a single registry, remembered per client |
| 4 | Waits | Captions on a timer | "22 of 30 · \~2 min · you can close this window" |
| 5 | Outcome | Six LEDs not matching destinations | passed / corrected / needs attention; detail by role (D1) |
| 6 | Why a file failed | Nothing | Fact plus action: "floor −52, limit −60. Gating can't close this. Re-record." → `_needs_attention/` (D2) |
| 7 | The title as a whole | Nothing | Consistency card, pending until the set is measured |
| 8 | The proof | Certificate view without verdict; JSON sidecar exists | Signed statement, four disclosure levels, three renderings (§4.6) |

### 4.4 Product structure — one engine, two products (D7)

|  | **Audal** (consumer) | **Audal \[Studio name TBD\]** |
| --- | --- | --- |
| Form | Desktop app + CLI + watch folder, engine in-process | CLI + SDK (Rust crate, C ABI) + optional desktop shell |
| Price | One-time; practitioner tier €149–249, narrator/podcaster tier lower; 30-day trial, watermark only in the statement | Annual site or seat licence, four-figure, offline activation |
| Exclusive | Onboarding, presets | Compliance dossier, broadcast and streaming-video profile packs, stem-aware delivery, embedded `axml`, C2PA export, keychain keys, SLA |
| Shared | Engine (`deliver` crate), profile registry, statement format, test corpus, determinism guard |  |

**Rule:** two packagings, not two roadmaps. A Studio request the shared engine cannot serve triggers a strategy review.

### 4.5 Competitive landscape

*Searched 2026-09-08 and 2026-09-21.* NUGEN AMB (tabbed queues, watch folders, logs; \~£700 core; enterprise scripting) — has compliance, lacks proof. DAW-resident meters (VisLM, Nuendo, Pro Tools) — **measurement is table stakes; we cannot sell it.** Auphonic Leveler — abandoned on modern macOS; position vacated. *Confirmed 2026-09-21, and the reason is structural, not neglect: Auphonic's own help pages say its newer algorithms run on Auphonic's own hardware and therefore cannot update the desktop programs, which do not work from macOS Ventura and above (auphonic.com/help/desktop, /leveler, read 2026-09-21); a 04/2026 report (ekiwi-blog.de) adds that activation itself fails because the license check uses outdated protocols modern servers reject, and because it is hardcoded it cannot be fixed locally. For us: "no plugin" is not only the isolated-department requirement — it is a documented argument to the consumer.* C2PA — export target, four-level disclosure model, browser verification without upload. Legal and healthcare — the competitor is "nothing". *Confirmed 2026-09-21, from the manufacturer directly, not a reseller: most NUGEN products now carry native Apple silicon support — AMB is the one exception, incompatible with macOS 11 and above and does not run under Rosetta (nugenaudio.com/frequently-asked-questions; the same on the product's own page). Big Sur is November 2020; post is Pro Tools, and Pro Tools is Mac. Architectural, not licensing — every other NUGEN product moved. Two numbers for the comparison: AMB processes at up to 100× real time and measures Leq(m) for trailers — both absent here.* **LM-Correct** — the real Phase 1 competitor, and smaller than assumed: $399, standalone and plugin, ITU/EBU/CALM, up to 7.1.4 only, DynApt as an optional extension (nugenaudio.com/lm-correct, price confirmed from three resellers). Level and true peak. No noise, no hum, no sibilance, no spacing, no paper — one of R5b's seven stages. *Where the narrator breaks today: 2026 guides (tomevox 03/2026 and 06/2026, narrationbox 07/2026) agree QC is automated before a human ever listens, and that noise-floor violations are consistently the most common rejection reason — and the market's standard fix for it is iZotope RX, not Auphonic, not LM-Correct. Our own F-096 sits exactly there: two measurements sharing a name, 43 dB apart, open since August.* *The disagreement mechanism: a 01/2026 guide (iwrity) states the author has the right to request corrections on any file that does not meet ACX's standards, that most contracts carry one or two correction rounds, and that ACX has a dispute-resolution process. The paper is not for the platform — it is for the correction round between author and narrator. Market feature, not our position; it belongs here.*

⚠ Not resolved here, and `§1.1` does not change for it: the two sources disagree on the review window — up to 10 business days per ACX's own help page, 14 per the 07/2026 guide.

**Positioning:** *after the bounce, not instead of the plugins.* Against NUGEN we sell proof; into the Auphonic gap, existence; toward C2PA, compatibility. NUGEN anchors Studio upward; the practitioner's hourly rate anchors consumer. Nuendo at \~€829 shows what practitioners pay for tools.

### 4.6 Interface direction

*Wireframes 2026-09-08; intake flow reused.*

- Tabs are titles and clients. The per-file table is the product; columns are engine measurements and change with the profile. Progress with a number and permission to leave. Set consistency is a separate card. Offline badge with build hash always visible.
- **Onboarding is one screen, once:** the role question (R11) and the first drop. Copy describes what the user gets back — *"Both get files fixed to spec. This only sets how much detail you see."* Never "checked", "analysed", "detected".
- The live intake flow of the cockpit (`components/jini/*`: drop → destination → progress) is the seed, **renamed `intake`**, with the flavour step removed, the fake timers replaced by real events, and "Album. N tracks." replaced by N rows.
- Statement view follows C2PA's four levels: (1) verified plus verdict; (2) one deterministic paragraph of what happened; (3) per-criterion table including `missing`; (4) forensic. Actions: HTML verifier, PDF, JSON, copy verification command.
- Nothing in these screens requires a webview — recorded for the Blitz/Slint contingency (R17).

## 5 · Requirements

Status vocabulary: **ready** · **exists-unwired** · **exists-partial** · **wrong** · **absent**. Sources: recons 09-06, 09-08 (ENGINE, METER, DSP chain), 09-09 (routing), 09-11 (desktop and addendum), 09-17 (conform ordering, source channels).

### 5.1 Must-have (P0) — Phase 1 MVP

#### R0 · The interface states only what was measured

As v7, extended by the governing principle: text is a fact of the signal, never an attribution.

#### R1 · Delivery profiles — one registry

- **One registry.** The `presets.rs` CATALOGUE is the source; `bmr-128.schema.json` is generated from it or deleted. *Measured: two registries with opposite gaps — `acx` missing from the schema (renders at −16/−1 instead of −20.5/−3), `apple_music` and `apple_podcast` missing from the CATALOGUE (fall back to Spotify).* **An unknown profile is an error, never a default.**
- Every profile carries `source_url`, `retrieved_at`, `spec_version`; the interface shows the date. *Measured: 13/13 sources point to the ACX URL; four profiles have none; the `atscA85` alias points to R128 values — removed until sourced.*
- Profiles are data files, not consts. Multiple saved profiles per client; one-action switch. Quarterly owner-check.

| Profile | Loudness | Peak | Noise floor | Spacing each end | Container | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Audiobook (ACX) | RMS −23…−18 dBFS | ≤ −3 dBFS | ≤ −60 dBFS | ≤ 5 s required, 1–5 s recommended | MP3 ≥192 CBR, 44.1 k, one format across the set, either one | **ready** (measure, verdict, export); render targets wrong until R5. Source sealed in the tree: acx-audio-submission-requirements-20260918.pdf, retrieved 2026-09-18. Peak: the page says peak, we measure true peak at 4× — stricter, and declared as ours |
| Podcast | platform target ±1 LU | ≤ −1 dBTP | — | — | MP3/AAC | gain target only; **no limiter, no verdict** |
| Streaming music (long tail) | −14 LUFS | ≤ −1 dBTP | — | — | WAV/FLAC/MP3 | gain target only |
| Broadcast (Studio, P1) | −23 LUFS ±1 | ≤ −1 dBTP | — | — | BWF 48 k | placeholder |

#### R2 · Analysis and per-file verdict

- Measured before and after: sample peak, true peak, integrated LUFS, RMS, noise floor, spacing both ends, duration, channels, rate, bitrate, hum lines, fundamental (for stage 3 only), **inter-channel correlation and mono-sum cancellation in dB**.
- Per criterion: measured, required, margin, outcome ∈ {pass, fail, advisory, missing}. One composer. *Ready.*
- **Verdict generalised over `DeliverySpec`**, not `AcxCheckReport`. *Measured: `deliver.rs` forces `preset_id: "acx"`.* Phase 1 build.
- **Polarity is a criterion wherever the profile emits mono.** Correlation near zero or negative in narration is almost always a polarity fault — a cable, a mic pair — and the mono sum the destination requires does not lower the voice, it cancels it. No gain repairs this. The file goes to `_needs_attention` with the fact and both numbers: *"L/R correlation −0.4; the mono sum this destination requires cancels 9 dB of the signal."* The gate is the **cancellation in dB against the profile window**, not the correlation figure — correlation explains, the loss decides. Its guardrail is K3, from a narration corpus; the 0.5 of the music survey is a music number and does not transfer.
- Detail by role (R11). Guidance is deterministic template lookup keyed by criterion × outcome × role. **Advisories state signal facts with times, never attributions.**
- Batch summary CSV/JSON — **not a statement, unsigned, proves nothing**, labelled so in the UI.

#### R3 · Batch processing

- ≥ 500 files accepted and processed; parallel across files, sequential within a file; UI under 100 ms at load; per-file and batch progress; pause, resume, cancel with no partial outputs; ≥ 50× realtime per core.
- **Input limit accommodates 120 min of 24-bit stereo 48 k via streaming decode.** *Measured: `MAX_FILE_BYTES = 500 MB` rejects at the gate.*
- **Batch is an in-process API call**, not 30 HTTP POSTs: `deliver_batch(title, profile, progress) -> Vec<Outcome>`. *Measured: no batch endpoint; `project_id` mandatory and never sent; `MAX_BLOBS = 16`.*

#### R4 · Set consistency — the judge

*Generalised from "title"; absorbs v7's R14.* Applies to any set of files delivered under one profile: an audiobook title, a podcast's per-track files, a client's batch.

- Channel layout identical across the set; mismatch fails the set (corrector: stage 12).
- Level spread, tonal drift and spacing outliers measured across the set and named (corrector: stage 13 target selection).
- Set verdict distinct from per-file verdicts; the set's statement carries `set_hash`.
- LTASS (Byrne) is used here **as an instrument only**: an outlier detector against natural speech and against the set's own median. Never as a processing target (D14).
- Two speakers at different levels **within one file** get an advisory stating the level event with times. Correcting it needs diarization and is mixing (K0). Out.

*Status: set-level processing exists and is live (EarFatigue via `/album/master`), `AlbumCertificate` exists, `AlbumConductor` orphaned, set verdict **absent**. R4 is build on live plumbing. EarFatigue itself serves no persona and is not enabled.*

#### R5 · The delivery chain — written explicitly

The chain is a **handwritten topology per profile family** with every parameter read from `DeliverySpec`. The Router/Flavor/Pipelineforge system is the flavour machinery and stays in the console line.

*Measured 09-08 and 09-09:* three routes exist; the UI sends everything to `/master/streaming` (since 706d237, 2026-07-17), which has **no limiter and no loudness target**; the Episode route hardcodes ceiling −1.0 and reads target from the wrong registry; the Music route reads ceiling from spec correctly; `Limiter`, `MultibandCompressor`, `Harmonic`, `SoftClipper` and `GlueChain` are all **dead**; MaskingEQ and Reverb are bit-exact neutral; **Width leaves a −145.8 dB residual** that breaks byte-identity; **LTASS correction is the only live coloration on speech**; POXVoice (gate, de-hum, AutoLevel) is unreachable; the live de-esser threshold is 0 dBFS (inert). Tests guard a −0.5 ceiling nothing applies.

- Speech chain and music chain per §6.0. **No Width, no LTASS, no MaskingEQ** in any delivery profile.
- `BrickwallLimiter` at `spec.max_true_peak_db` in **every** route; a test per profile asserts `ceiling == spec`.
- Encode → measure the delivered file → correct → re-measure, **at most N = 2** (K3 threshold). Non-convergence writes the file to `_needs_attention` with the conflicting criteria **named from the rows** — "RMS −23.4, required ≥ −23.0; raising it puts true peak at −2.8 against a −3.0 ceiling" — since the conflict may be between two spec criteria or between the pre-encode measurement and the encoded file (F-077).
- Deterministic: same input, parameters and engine version ⇒ bit-identical `data` chunk (D10). Every stage's activation decision is itself deterministic, with thresholds from corpus.
- Dead nodes and their tests are removed in the prune (§6.4).
- The statement records **render targets applied** alongside measured results, so a render/export mismatch is visible.

#### R5b · Corrective stages (D12)

Admission test, per stage, in order. A stage that fails any column does not enter the chain, and no stage has a user slider.

- **K0a — correction or mixing?** Correction moves *one signal* toward *one criterion*. Mixing balances *two signals* against each other. Mixing is out, however good the DSP.

  **K0b — does it add or destroy?** A stage that only alters what is already there may run unattended: a wrong verdict is still recoverable from the output. A stage that **removes** samples cannot — a breath, a creak, the first syllable of a word at −55 is gone — and a stage that **generates** samples that were never recorded cannot either, because it has to invent them. Removing runs only on explicit approval, carried as a **parameter of the request** so the same input and the same approval still produce the same bytes. Generating needs a deterministic, stated source before it may even be proposed.
- **K1 — named criterion.** Brings the file toward a criterion the profile names.
- **K2 — bounded and drawable.** Its effect is written in the statement as events with time and dB.
- **K3 — thresholds from corpus.** Activation and guardrail thresholds come from a fixture-factory corpus (R12), never from theory; provisional and labelled until then.

| # | Stage | K0 | K1 criterion | K2 recorded as | K3 corpus | Priority | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | DC removal | ✓ | always safe | offset dB | ✓ | **P0** | exists (DcRemoval biquad) — wire |
| 1 | Noise gate / expander | ✓ | noise floor ≤ −60 | dB plus margin | ✓ | **P0** | exists-unwired (POXVoice) |
| 2 | De-hum 50/60 Hz and harmonics | ✓ | noise floor | ratio and frequency per notch | ✓ | **P0** | exists-unwired |
| 3 | Low-cut, conditional | ✓ | noise floor (rumble) | dB; corner ≤ 70 Hz, 2nd order; **only if sub-60 Hz energy > threshold and corner < fundamental** | ✓ | **P0** | exists-unwired |
| 6 | De-esser as guard | ✓ | protects against our own gain and limiting | GR event count and dB | ✓ | **P0** | exists; threshold 0 dBFS inert → −24 |
| 10 | Interior noise analysis | — | feeds 1, 2, 3, 7 | analysis only | ✓ | **P0** | exists |
| 11 | **Spacing conform** — trim room tone to the destination's ceiling. **Never pads:** the requirement is an upper bound only (F-119), so adding serves no criterion and would have to generate samples. Destroys, so K0b — runs only on approval carried in the request | ✓ | ACX spacing | ms removed or added, room-tone source | ✓ | **P0** | **absent** — measured and judged, never corrected |
| 12 | **Channel conform** — one layout across the set, **keeping the input's** wherever the set is already consistent. ACX requires consistency, not mono (F-119); converting by choice costs up to 6.5 dB on decorrelated material | ✓ | set consistency | the conversion and its cost: "stereo→mono, (L+R)/2, correlation 0.81, level −1.1 dB" | ✓ | **P0** | absent |
| 13 | **Level conform = target selection** — `target_file = clamp(set_reference, profile_window)`, then one gain via AutoLevel. **Moves only inside the window; unreachable within the peak ceiling → needs attention** | ✓ | set consistency (ACX ±2 dB practice) | profile window, set reference, gain applied | ✓ | **P0** | absent (AutoLevel exists-unwired) |
| 4 | De-click / de-crackle | ✓ | ACX human QA | count, positions, max ms each | fixture needed | **P1** | absent |
| 5 | Plosive / LF thump | ✓ | ACX human QA | HPF at positions, dB | fixture needed | **P1** | absent |
| 7 | Spectral noise reduction, bounded | ✓ | noise floor ≤ −60 (floors −60…−50) | dB ≤ 6–10, same profile across set, FAIL beyond artifact threshold | fixture plus **listening jury (ABX)** | **P1 → P0 if beta shows floor is the dominant FAIL** | absent |
| — | Ducking (music bed under speech) | **✗ mixing** | — | — | — | **out** → Studio stem-aware (R17), where the ratio is a client spec; console otherwise | ducking topology exists in the streaming route |
| — | Same-stem speaker balancing | **✗ mixing** (needs diarization) | — | — | — | **out** → advisory only | — |
| 8 | De-reverb | ✓ | none | not boundable | — | **rejected** | — |
| 9 | Spectral repair | ✓ | none | manual by nature | — | **rejected** | — |
| — | LTASS / reference targets | ✓ | none (taste) | — | — | **rejected** for processing (D14); instrument in R4; console feature | live today — removed from delivery |

**Ordering, measured 2026-09-17.** Spacing conform is harmless to level: trimming two seconds from each end of nine full chapters moves whole-file RMS by at most 0.043 dB and integrated LUFS by 0.006 dB, against a five-decibel window. The mechanism is real — the destination's RMS is unweighted and excludes nothing, not even silence — but four seconds inside half an hour is a fraction of a percent. Stage 11 may run either side of the level measurement. *Measured on whole chapters only: on three-minute material the same trim is ten times the fraction and has not been measured.*

**Channel conform is not.** ACX takes mono — `emitted_channels: Some(1)`, actively checked — so the downmix is required rather than optional, and production sums both sides at half amplitude: identical channels lose nothing, uncorrelated channels lose up to three decibels, out-of-phase ones more. Across ten licensed tracks with correlation below 0.5, every one moved between 1.3 and 6.5 dB; a quarter of that corpus of 437 sits below 0.5, median 0.806. Stage 12 therefore runs **before** the level measurement that stage 13 consumes — which is what today's `export_mp3_acx` already does inside a single function. What is not measured is how often a real chapter carries enough decorrelated material to matter. The ordering costs nothing to get right and the worst case exceeds the window, so it is fixed by order rather than by frequency.

**Stage 7 is last resort.** It lowers the noise floor, which is the activation criterion of stages 1, 2 and 3. It therefore runs after them and only if the floor is still outside the limit; if it runs, the statement records that the floor was corrected twice and by which stages. Stage 10 feeds all four, but the order is fixed: cheap and bounded first, spectral last.

**Sensors that survive the K0 cut:** the macro classifier as a **container and heuristic probe only** — its voice/music classification fell (F-108: six segments it labelled music were plain narration by ear; F-110: one of its two features scores better alone than both together); **micro-VAD as a voice-presence detector** (F-062, 0.12% chop on podcast — which of the two instruments was measured is unconfirmed, F-113; both failed as classifiers, F-114). Dialogue-gated loudness is *declared, unmeasured in this role* (Q12).

#### R6 · Offline guarantee

- **Opens no connection — not even loopback.** With the engine in-process (D9) the consumer app has no daemon, no Caddy, no axum, no `reqwest`. *Measured: HTTP and TLS crates enter via Tauri and SurrealDB; both leave. The WebKit HTTP stack (soup3) remains as a rendering-engine capability with no call site — stated in the dossier.*
- No update check, crash reporting or analytics. Offline Verification panel with build hash and a one-page auditor procedure.
- Ollama pre-warm and all `localhost:11434` call sites are **not in the delivery build**.

#### R7 · Signed delivery statement — primary differentiator

- Contents as v7; **three bindings, three roles (D10):** `deliverable_sha256` = bytes of the delivered file; `data`-chunk/PCM hash = determinism and embedding; `master_sha256` = provenance. *All three exist.*
- `audio_origin` provenance. *Ready.* Tamper test. *Ready.*
- **Omitted until fixed:** `lra`, `short_term_lufs`, `momentary_lufs` — computed on unweighted PCM and therefore wrong. Shown as `missing`. Integrated LUFS is correct.
- One signed JSON per file **and one per set**. Renderings: **HTML** self-contained verifier (drop the audio, WebCrypto hash, Ed25519, no network); **PDF** deterministic archival; **ADM `axml`** sidecar and, for WAV/BW64, **embedded** in the file (D13, P1) with an ADM-parser tolerance test. CSV is not a rendering.
- Role-matched explanation; off in the delivery folder for narrators, on for practitioners; always in the project folder. Trial watermark only here.
- Signing key per install, `0o600`. Consumer: acceptable. Studio: OS keychain (R17).
- The statement model is **C2PA-shaped now** — assertions, hash bindings, ingredients — so C2PA export is a serializer later.

**The statement says what it does not cover.** Four of the destination's published requirements are not machine-measurable and are never implied by a verdict: human narration with synthesis and generated voices explicitly prohibited, opening and closing credits, a section header in every file, and the absence of extraneous sounds. The page itself says a human review follows the automated one (F-119). The statement carries the measurable criteria and names the rest as outside its scope — a proof that quietly reads as broader than it is would be the first thing an auditor breaks.

#### R8 · Storage — SQLite, no authentication layer

- **SQLite via `rusqlite` (bundled)**, one file per OS user in the app data directory. *Measured: builds clean beside the workspace; SurrealDB used 2 of 8 tables (`projects`, `tracks`); the other six are the old product's schema and are not migrated.* Tables: `projects`, `tracks`, `profiles`, `statements`.
- **No authentication layer.** The OS user is the user. Licensing is a signed file; keys live in the keychain (Studio); the SDK has no users.
- Disk-streaming pipeline, bounded RSS, user-controlled working directory. *Ready.*

#### R9 · Formats, channels and metadata

- Input: WAV/BWF, AIFF, FLAC, MP3. Output Phase 1: WAV/BWF, FLAC, MP3.
- One resampler crate, pinned. *Measured: two rubato versions.* Decoder pinned exactly. *Measured: symphonia caret.*
- **Metadata preserved, named:** `bext` (TimeReference, OriginationDate and Time, Originator, UMID, `CodingHistory` **extended** per EBU R98, the five loudness fields written with **measured** values), `iXML` as an opaque blob, `cue` and `LIST-INFO`; FLAC Vorbis comments and PICTURE; MP3 ID3v2 (title, artist, chapter, cover). *Measured: nothing is read from input; `bext` is written only on the ADM path, with constants and `0x7FFF` loudness fields.* Roundtrip test required.
- **The engine is stereo internally and has no mono path** (F-117). Three places pin the channel count at two — `TARGET_CHANNELS`, the standardised stream's own reporter, and the raw source the trunk pass opens — and a mono input is doubled immediately after decode. Of the nine reference narrations, five declare one channel and are doubled by us, three declare two and are already identical, one is genuinely stereo. This is declared rather than hidden, and it is why the channel guarantee is a property of the **output container**, not of the internal path. What the doubling costs is unmeasured: there is no mono path to compare against, and the analysis already collapses to mono before the scout.
- Determinism is asserted on the `data` chunk so headers may vary (D10).

#### R10 · Platform packaging

- macOS Universal notarized; Windows 10/11 x64 signed; Linux AppImage and `.deb` **with a declared `Depends:` list** (`libwebkit2gtk-4.1-0`, `libgtk-3-0`, `libxdo3`). *Measured 0.7.10: the `.deb` ships with zero Depends and fails on `libxdo.so.3`.*
- Installers are tested on **clean desktop VMs**, not headless containers — a container without X11 or GL is not a user's machine.
- **Memory:** v7's "idle ≤ 60 MB" is withdrawn; a webview idles at \~155–165 MB on Linux (measured). The requirement is **no growth**: RSS over a 12-hour batch stays within X MB of idle. Absolute idle per OS is recorded as a baseline.
- LAME dynamic with LGPL notice. *Ready.* `global-hotkey`/`libxdo` is an unconditional Dioxus dependency with no use — declared in the dossier.
- **CI matrix (ubuntu, macos, windows)** runs the determinism suite and `dx bundle` on every push. *Measured: CI is weekly, 8/8 failures, blocked at `cargo fmt`; no macOS or Windows machine exists.* This is the first commit of the new repository.

#### R11 · Role declaration and failed-file policy

- **Single first-launch screen:** "Who do you deliver for?" — *Myself* (fix my files, tell me only about the ones that can't be saved) or *Clients* (fix, show every measurement, sign a statement I can hand over). Local, changeable, never transmitted. Never shown again; each title's empty state is the drop zone.
- Uncorrectable files go to `_needs_attention/` with a `_FAILED` suffix and a statement recording the failure (D2).
- Contact summary generated locally only when the user chooses to contact us.

#### R12 · Measurement validation and the fixture factory

- Integrated LUFS against **EBU Tech 3341/3342 official vectors at ±0.1 LU**; true peak per BS.1770 Annex 2. *Measured: no official vectors; ±0.5/±1.0 LU tolerances.*
- Comparison with `libebur128` and `ffmpeg ebur128` on real speech (60 LibriSpeech files exist).
- LRA, short-term and momentary fed K-weighted audio, with a **spectrally varying fixture** so the test can fail. *Measured: constant-tone fixtures hide the error.*
- Sample-rate guard: `LufsMeter::new(sr)` with 44.1 k coefficients or an assert. *Measured: hardcoded 48 k, silent misreport otherwise.*
- ACX RMS matched to `acx-check.ny` with the one deviation declared. *Ready — keep.* Spacing and floor known-answer tests. *Ready.*
- **Standing jury:** verdict accuracy, meter agreement, determinism, metadata roundtrip, stage residual (blake3 plus Welch per band, as in the 09-09 measurement). No juror grades itself. **Listening jury** (ABX on fixtures) for stage 7.
- All of it runs on push in the CI matrix.

### 5.2 Nice-to-have (P1) — Phase 2: practitioner and Studio

#### R13 · Parameter transparency

Every resolved parameter read-only by default, editable on request, edits recorded in the statement as applied. Not a creative control.

#### R15 · Watch folder

Included in the consumer edition (D5). One per client profile.

#### R16 · Command-line interface

Included in the consumer edition (D5). `audal --profile <id> <files>`; exit codes reflect verdicts; JSON output. Comes for free from the in-process `deliver` crate.

#### R17 · Studio edition

- **Compliance dossier:** data-flow statement, offline procedure, licence audit (SQLite and rusqlite public domain; `libxdo` declared; soup3 stated), build reproducibility, verification script.
- **Profile packs:** EBU R128, ATSC A/85 (−24 LKFS, −2 dBTP, sourced), major streaming-video specs — each sourced, dated and verdict-producing. The 5.1 meter exists and is correct (weights verified against ffmpeg ±0.2 LU) but is wired to a hardcoded Apple −18; rewire to profiles.
- **Stem-aware delivery:** measure each stem (dialogue, music, effects) and the mix; dialogue stem as anchor; **dialogue-to-background ratio as a client-spec criterion** — here, and only here, ducking is a correction (K1 real). Dialogue-gated loudness: declared, unmeasured in this role (Q12).
- **Embedded `axml` certificate** in WAV and BW64 (D13); ADM-parser tolerance test (Q11). RF64 and BW64 verified with a real consumer.
- **C2PA export**, self-signed and labelled "unrecognized signer" until a CA certificate is obtained.
- **Keys** in OS keychain, DPAPI or secret-service.
- **WebView2-free shell contingency:** Dioxus Native (Blitz — present in 0.7.10, alpha) first, Slint second; decide only if a pilot objects to WebView2.
- **Licensing:** annual site or seat, offline activation.
- Exit: one external pilot completes a real delivery via CLI or SDK; its QC accepts the ADM or C2PA artefact.

### 5.3 Future considerations (P2)

Shareable profile packs; plugin extensions; spatial and immersive (shelved — the post beachhead may pull it); MXF I/O if interviews show it is the deliverable (Q13); a verification endpoint for transfer platforms — partnership, on request only.

## 6 · Technical architecture

### 6.0 Shape of the machine

```
  WHAT THE CUSTOMER BUYS
  "I hand over a title. I get files that pass,
   and paper that proves it to a third party."

  THREE DOORS, ONE MACHINE
  ┌───────────┬──────────────┬──────────────────┐
  │   APP     │ WATCH FOLDER │   CLI / SDK      │
  │ drop 30   │ drop and go  │ 40 times a week  │
  └─────┬─────┴──────┬───────┴────────┬─────────┘
        └────────────┴────────────────┘
                     ▼
        ┌────────────────────────────────┐
        │ PROFILE = DATA, ONE REGISTRY   │
        │ ACX · Podcast · Netflix ·      │
        │ Client B                       │
        │ source_url · retrieved_at      │
        │ ⇒ the user chooses.            │
        │   no CONTENT detection.        │
        │   unknown profile = error.     │
        └───────────────┬────────────────┘
                        ▼
  ┌──────────────────────────────────────────────────┐
  │ A · MEASURE EVERY FILE                            │
  │   container: channels · rate · duration · bitrate │
  │   signal: floor · LUFS · true peak · RMS · hum    │
  │           spacing · fundamental (stage 3 only)    │
  │   ⇒ one pass, O(1) memory                         │
  │   ⇒ no content detection: the probe is measurement│
  └───────────────────────┬──────────────────────────┘
                          ▼
  ┌──────────────────────────────────────────────────┐
  │ B · JUDGE THE SET  (R4) — two parts               │
  │   B1 shape, from container and spacing only:      │
  │      channel layout · spacing policy              │
  │      ⇒ independent of level                       │
  │   B2 level reference — closes after conform,      │
  │      because channel conform moves RMS            │
  └───────────────────────┬──────────────────────────┘
                          ▼
  ┌──────────────────────────────────────────────────┐
  │ C · CORRECT AND DELIVER  (R5, R5b)                │
  │   every stage wakes from its own measurement      │
  │   and writes what it did, with time and dB        │
  │                                                   │
  │   DC removal                                      │
  │     ▼                                             │
  │   spacing conform   ← over the ceiling only (B1)  │
  │     │  destroys ⇒ approval in the request,        │
  │     │  else the file goes to needs-attention      │
  │     ▼                                             │
  │   channel conform   ← one layout, the input's     │
  │                       where the set agrees   (B1) │
  │     ▼                                             │
  │   RE-MEASURE LEVEL on the conformed signal        │
  │     ▼  ⇒ B2 set reference closes here             │
  │   expander          ← floor > limit               │
  │     ▼                                             │
  │   de-hum            ← mains line found            │
  │     ▼                                             │
  │   low-cut           ← sub-60 Hz > threshold,      │
  │                       corner < fundamental        │
  │     ▼                                             │
  │   ┄┄┄ P1 stages insert here ┄┄┄                   │
  │   de-click · plosive · bounded denoise            │
  │   (denoise last: it moves the floor the           │
  │    three stages above were gated on)              │
  │     ▼                                             │
  │   target select     ← set_reference ∩ window      │
  │     ▼                                             │
  │   AutoLevel         ← one gain to target_file     │
  │     ▼                                             │
  │   de-ess guard      ← our own gain, fixed −24     │
  │     ▼                                             │
  │   LIMITER (spec ceiling) → dither → ENCODE        │
  │     ▼                                             │
  │   MEASURE THE DELIVERED FILE (not the internal)   │
  │     ▼                                             │
  │   converges? ── yes ──▶ deliver                   │
  │     │ no, max N = 2 (K3 threshold)                │
  │     ▼                                             │
  │   _needs_attention, naming the criteria that      │
  │   conflict, from the rows                         │
  └───────────────────────┬──────────────────────────┘
                          ▼
  WHAT COMES OUT
  ┌───────────────────────┬──────────────────────────────┐
  │ THE FILES             │ THE PAPER                    │
  │ _delivery/            │ one signed JSON per file     │
  │   ch01..ch30 in the   │ + one per set                │
  │   destination's       │   renderings:                │
  │   container           │   HTML  ← self-contained     │
  │ _needs_attention/     │            verifier          │
  │   ch14 "floor −52,    │   PDF   ← archival           │
  │   limit −60.          │   ADM axml ← studio QC       │
  │   Re-record."         │ inside: hashes · per-        │
  │                       │ criterion · verdict ·        │
  │                       │ corrections as events ·      │
  │                       │ render targets · Ed25519     │
  └───────────────────────┴──────────────────────────────┘
  ┌──────────────────────────────────────────────────────┐
  │ CSV — batch summary. NOT a statement. Unsigned.       │
  └──────────────────────────────────────────────────────┘
                          ▼
  ┌──────────────────────────────────────────────────────┐
  │ A THIRD PARTY OPENS IT WITHOUT US                     │
  │ drops the file onto the HTML in a browser             │
  │ ⇒ WebCrypto hash · Ed25519 · zero network             │
  └──────────────────────────────────────────────────────┘

  THREE THINGS THAT MAKE THE DIAGRAM TRUE
  · zero outbound connections — verifiable by eye
  · same input ⇒ same data-chunk bytes — on a second machine
  · what was not measured is said, not silenced

  TODAY, IN THREE LINES
  · the app receives 30 and keeps 1
  · the verdict is produced for one destination,
    whichever you chose
  · the paper exists, without the verdict and without
    the renderings
  ⇒ the engine is ahead of the shell
```

### 6.1 Architecture (D9 revised, locked 2026-09-11)

- **Engine:** the `conformance` crate — measure, judge, correct, encode, sign. Conformance is not one of those five but the purpose they serve; the crate is named for the purpose. Entry points: `execute_streaming_plan` for a render, `run_deliver_core` for a delivery. The engine depends on no server, no database and no audio device, and nothing that does may enter it.
- **Shell:** **Dioxus 0.7.10 desktop, pinned. No Tauri, no wasm, no daemon.** The shell calls the engine as a function in the same process, shares type definitions with it, and the table holds a reference to the struct that was signed.
- **Scheduler:** batch queue, work-stealing pool, cancellation, progress events throttled to what the table needs — a row changes when a file completes; one progress bar at ≤ 4 Hz.
- **Storage:** SQLite (R8). **Packaging:** `dx bundle` → `tauri-bundler`; fallback to a direct bundler or per-OS scripts if runtime breakage persists on 0.7.x.
- `/master/streaming`, axum, Caddy, SurrealDB, the Router/Flavor system, JINI and the cockpit **stay in the archived repository** (§6.4).

### 6.2 Communication paths

| Path | Mechanism |
| --- | --- |
| Shell ↔ engine | direct function calls, shared types |
| Progress | engine → shell channel, throttled |
| Live parameter preview (R13) | shared-memory atomics |

### 6.3 Determinism

- Per-file single-threaded DSP; parallel across files. *Measured: `rayon .par_iter` in the ISP limiter — prove order-independence or make it sequential.* `libm` for all transcendental calls. *Measured: 5 `f32::exp`.* Encoder, decoder and resampler pinned exactly.
- Asserted on the `data` chunk (D10). Width removed (−145.8 dB residual). Stage residual test: blake3 identity for neutral stages.
- **Cross-platform:** the CI matrix is the second machine. Blocking for Phase 0.

### 6.4 The new repository (D9 revised)

The delivery product lives in a **new repository, forked with full history**; the current repository is tagged `stillair-archive-<date>` and frozen read-only for the console and live line. The engine flows one way: the delivery repository owns it; the archived line consumes it by tag.

**Prune pass 1** happens in the new repository, as its first commits — never in the archive. The archive then stays exactly as it was at the tag, a true snapshot of what existed and a console line with every piece it needs; each deletion becomes a visible act with a message, and the new repository's log opens with the reasons rather than with an already-cleaned tree. Scope (Appendix B as guide): cockpit, JiniPanel and Ollama, Router/Flavor/Pipelineforge, dead DSP nodes and their tests, spatial path, third upmix, cut-repair, EarFatigue model (keep the set plumbing R4 needs), fuzzer, Tauri, SurrealDB and the six unused tables, Caddy, axum — **and the segmenter with its classifier** (`SegmentScout`, `smooth_and_segment`, escalation flagging, the onset detector's Otsu threshold): the delivery chain has no segment boundaries, and every stage is file-global or event-based.

| Reused | Left in the archive |
| --- | --- |
| `types.rs` (compiles as-is) | Everything wasm and Tauri-bound: `ipc.rs` (deleted, not rewritten — the boundary no longer exists), `neon_canvas`, knob, `__TAURI__` listeners, `gloo-timers` |
| Intake flow (`components/jini/*` → `intake`), 5 subcomponents compile as-is | Flavour selector, DSP chain demo panel, CompliancePanel LEDs, fake timers |
| `AlbumMatrix` and `pdf_preview` **as design**, not code | JiniPanel, Wizard finding catalogue |
| Deterministic guidance strings | Real-time cockpit FSM and HUD (the live line's asset) |

*Measured: 150 wasm and Tauri hits in 15 files; \~70% of UI files do not compile on desktop. The reuse table above is what survives.*

### 6.5 Documents and lines that do not govern this product

- `cockpit/wizard-constitution.md`, `cockpit/jini-spec.md`, `ONBOARDING1_6.md` and the 4-bus console design belong to the **console and live line**. Their surviving principles — absence stated never defaulted; deterministic guidance keys; the user confirms every action; teacher offline and student as constants; the Measurement Court — are restated in R0, R2, R12 and in `ENGINEERING_PRINCIPLES.md` of the new repository.
- `docs/FUTURES.md` in the archive records the console (Byrne reference targets, stems-in mixing for podcasters), the live monitor (plugin-first, distilled on-device model, network posture undecided — if online, OIDC and JWT, DB auth only for the app), album cohesion, the teaching cockpit — and which code serves each. **It also records the segmentation work:** two centroids, a projection and an escalation gate feeding stem separation, measured out of the delivery product by K0 and K1 and left working — the onset detector now takes its threshold from the signal rather than a constant, which removed an audible zipper and cut false boundaries by a sixth to a quarter on real books. F-107 through F-116 record what was measured, including the six candidate measures that did not work.

**`ENGINEERING_PRINCIPLES.md`** (new repo): D11 · R0 and the fact-not-attribution rule · K0–K3 · **one registry per number, one path, one test — a second source is a bug even when it agrees** · no number without a hashed log · comments that assert routing must cite the test that proves it.

## 7 · Success metrics

As v7, with three additions: "meter agreement with EBU vectors ±0.1 LU, CI green" at Phase 0 exit; **"RSS growth over a 12-hour batch ≤ X MB"** replacing the absolute idle target; "statement verified by a third party, at least one client per practitioner beta".

## 8 · Risks and mitigations

| Risk | L | I | Mitigation |
| --- | --- | --- | --- |
| Live chain has no limiter or target; ACX saved only by export | **Present** | High | R5 explicit topology; ceiling test per profile |
| LTASS colours speech today | **Present** | High for the proof claim | Removed from delivery (D14) |
| Two registries; unknown profile silently defaults | **Present** | High | R1 one registry; error on unknown |
| Statement publishes wrong LRA, short-term and momentary | **Present** | High | Omit until fixed; varying fixture |
| Metadata loss disqualifies post use | **Present** | High (Studio) | R9; D10 |
| CI guards nothing; no macOS or Windows machine | **Present** | High | CI matrix as first commit |
| Channel conform moves level out of the window on decorrelated material | **Measured** | Medium | Stage 12 before the level measurement (R5b); needs-attention on unreachable |
| `dx bundle` 0.7.x runtime breakage | Medium | Medium | Clean-VM test; bundler fallback; not a framework change |
| Dioxus desktop table throughput 4.5× slower in 0.7 (stress test) | Measured | Low | Realistic load test; virtualization only if needed |
| Width residual breaks byte-identity | **Present** | Medium | Width removed |
| Consumer price anchors B2B | Medium | High | Separate Studio name and exclusives |
| Roadmap drift toward console features | High | Medium | K0; non-goals; D11; separate repositories |
| Beachhead wrong | Medium | High | Q10 before Studio build |
| Platform changes specs | Medium | High | Sourced, dated profiles; quarterly check |

## 9 · Decisions and open questions

### 9.1 Decisions

D1–D8 as v7. Then:

| # | Decision | Source |
| --- | --- | --- |
| **D9 (rev.)** | Dioxus 0.7.10 desktop, pinned; no Tauri, no wasm; engine in-process; new repository forked with history; stillair archived read-only. Framework spike cancelled; the only spike is "installer runs on a clean desktop VM on three OS". | Recons 09-11; addendum 0.7.10 |
| **D10** | Binding roles: delivered-file hash = identity; `data`-chunk hash = determinism and embedding; master hash = provenance | Recon 09-08 |
| **D11** | Product drives code | Discussion 09-08 |
| **D12** | Corrective stages admitted by K0–K3, shown as a table; "restoration" is not a word in the product | Discussion 09-13/17 |
| **D13** | Certificate travels with the file (embedded `axml` P1, sidecar, C2PA later); no transfer platform | Discussion 09-08 |
| **D14** | LTASS and Byrne removed from delivery processing; instrument in R4; reference-target library is a console feature | Discussion 09-11 |
| **D15** | SQLite via rusqlite; no authentication layer; two of eight tables migrate | Recon 09-11 Q6 |
| **D16** | Ducking and same-stem speaker balancing are mixing (K0); out of the consumer edition; ducking returns only in Studio stem-aware delivery as a client-spec ratio | Discussion 09-17 |
| **D17** | Stage order is fixed by the worst measured case, not by frequency: channel conform precedes the level measurement, spacing conform may run either side | Measurement 09-17 |

### 9.2 Open — blocking before the Phase 1 build

1. **\[Eng\]** CI matrix green on three OS (replaces "second machine").
2. **\[Product\]** Studio edition name.
3. **\[Legal\]** TMview and EUIPO check for "Audal", classes 9 and 42; Greek national filing for the 6-month priority; EU filing before the first public post.
4. **\[Eng\]** Installer runs on clean desktop VMs (macOS notarization, Windows signing, Linux Depends).

### 9.3 Open — resolve during build

5. Working-directory sizing. 6. Narrator and podcaster interviews. 7. Multi-production speech corpus. 8. Role-question wording. 9. Spotify podcast target source. 10. **Five post and localization vendor interviews** (tool today; auditor ever rejected a cloud tool; MXF or WAV). 11. ADM parser tolerance of a certificate namespace. 12. Dialogue-gated loudness: declared, unmeasured in this role. 13. MXF as deliverable. 14. Ed25519 local keys vs notarization and export control. 15. **\[Q1 drag test\]** pending a 60-second human drag on the 0.7.10 scratch app — 30 files, a folder, 500 files, a Greek filename. 16. Dioxus 0.7 table-throughput cause (realistic load test). 17. Which micro-VAD instrument F-062 measured (F-113). 18. **How often a real chapter carries enough decorrelated material for channel conform to move level** — the ordering is fixed either way, but the frequency is unknown.

## 10 · Timeline and phasing

| Phase | Scope | Exit criteria | Duration |
| --- | --- | --- | --- |
| **0 — Fork and make the guards real** | Tag and fork; prune pass 1; **first commit: CI matrix (3 OS) running determinism and bundle**; `deliver` crate extraction; one registry; explicit delivery topology with `ceiling == spec` tests; Width and LTASS out; `f32::exp` → libm; limiter sequential; pins; LRA/ST/M fix plus varying fixture; EBU vectors ±0.1; sample-rate guard; D10 hash scope; SQLite (2 tables plus 2 new); `MAX_FILE_BYTES` lifted; `.deb` Depends; installer on clean VMs; Ollama gone | Golden `data`-chunk hashes match on 3 OS in CI; EBU vectors green; a 30-file set yields 30 outputs and 30 statements through `deliver_batch`; installers run on clean VMs | 5–6 weeks |
| **1 — MVP (P0)** | New shell (`intake`, table, statement view) on Dioxus desktop; R2 generic verdict; R4 set verdict; R5b P0 stages wired and logged (0, 1, 2, 3, 6, 10, 11, 12, 13) **in the measured order**; R9 metadata passthrough and the channel declaration; R11; HTML verifier and PDF; three platforms | Corpus at 99% (ACX via beta narrator files under NDA); 10 beta narrators complete a title; 5 beta practitioners each deliver to a client who receives the statement | 12–16 weeks |
| **2 — Practitioner and Studio (P1)** | R13, R15, R16; stages 4, 5 and 7 with the fixture factory; embedded `axml`; profile packs (verdict-producing); stem-aware delivery; dossier; C2PA-shaped export; keychain. Gated by Q10 | One external pilot completes a real delivery via CLI or SDK; its QC accepts the artefact | 12–16 weeks after MVP |
| **3 — (P2)** | Packs, plugins, spatial, MXF | Pilot demand | TBD |

**Before leaving the archive (one day):** inspect the external `FINDINGS.md` change; tag; write `docs/FUTURES.md`; extract `ENGINEERING_PRINCIPLES.md` for the new repo; fork.

## 11 · Business assumptions and kill criteria

As v7 §11. Priors after all recons: **A (consumer viable) 40–50%; B (Studio company) 10–15%; A or B \~50%.** The six tests and kill signals are unchanged; none of the engineering findings since v7 moves them — they move only cost and dates. The consumer core (ACX path, hashes, signing, integrated LUFS, spacing, floor) came out more solid than assumed; the Studio assets (embedded certificate, set verdict, library entry point) came out as build rather than wiring.

## Appendix A — Changes v7.1 → v7.2

| v7.1 | v7.2 | Evidence |
| --- | --- | --- |
| Stage order 11 → 12 → 13 after the level measurement | Channel conform **before** the level measurement; spacing conform either side; D17 added | Measurement 09-17: spacing ≤ 0.043 dB, channel 1.3–6.5 dB on ten decorrelated tracks |
| Stage 7 positioned but unexplained | Declared last resort; it moves the floor the three stages above were gated on | R5b |
| "RMS and peak cannot both be met" as the fixed non-convergence message | Criteria named from the rows; the conflict may also be the encoder gap | F-077 |
| Goal 3: "≥ 99%" | "≥ 99%, measured against a validated corpus and reported beta outcomes; acceptance is observed, never measured directly" | §1.1 — the platform returns no values |
| Classifier failure cited as F-114 in three places | F-108 and F-110 for the macro classifier; F-114 for the two voice detectors | Register |
| Channel handling unstated | R9: the engine is stereo internally and has no mono path; five of nine narrations are doubled by us | F-117 |
| Prune list without the segmenter | Segmenter, classifier and the onset detector's Otsu threshold named in the prune and in FUTURES | §6.0 has no segment boundaries |
| — | Q18 added: how often real chapters carry decorrelated material | Ordering fixed by worst case, frequency unknown |

## Appendix B — What exists, per recons through 2026-09-17

**Ready and tested:** ACX RMS, peak, floor and spacing matched to the de facto reference; margin-aware verdict composer (pass, fail, advisory, missing); integrated LUFS structurally correct (48 k only); 5.1 meter with verified weights; three hash levels including the delivered file; Ed25519 signing with engine version and commit; tamper test; `audio_origin`; JSON sidecar with envelope verification; measure-after-encode loop (unbounded); disk-streaming pipeline; LAME dynamic with notice; byte-identity test (one machine); `projects` and `tracks` schema; ADM BWF writer (`bext`, `chna`, `axml`, `ds64`); engine crates free of server dependencies; Music route ceiling from spec; MaskingEQ and Reverb bit-exact neutral; Dioxus file-drop API correct by source (0.7.10: `Vec<FileData>` with `.path()`); rusqlite builds clean; **the mono-to-stereo duplication has two implementations with a parity test proving them byte-identical** — the only duplicate found with a guard.

**Exists, unwired or partial:** POXVoice speech nodes (gate, de-hum, AutoLevel, de-esser) — unreachable; `RestorationChain`; DcRemoval; EarFatigue set processing (live, not required); `AlbumCertificate` (facts, no verdict); `AlbumConductor` (orphaned); BW64 write (unverified); `run_deliver_core` and `execute_streaming_plan` (plain functions inside `m0d`); intake flow; `types.rs`; Blitz renderer (compile-only).

**Wrong:** LRA, short-term and momentary unweighted; `bext` loudness fields `0x7FFF`; `atscA85` alias; live de-esser threshold 0 dBFS; Episode ceiling −1.0 hardcoded; ACX renders at −16 LUFS from the wrong registry; `flavor.rs:116` "orphaned path" comment (false); `graph.rs` "dynamic via NMF stems" comment (false); tests guarding a −0.5 ceiling nothing applies; `.deb` with no Depends; the macro classifier as a voice/music gate (F-108, F-110); both micro-VAD instruments as classifiers (F-114 — one answers "voice" everywhere, the other flattens nine books to single segments); **two de-essers running together with different channel linking** (F-117).

**Absent:** generic verdict; batch API; set verdict; metadata read and write; EBU vectors; sample-rate guard; running CI; macOS and Windows machines; stages 11, 12, 13, 4, 5 and 7; embedded certificate; HTML verifier; PDF from statement; explicit delivery topology; one registry; **a mono path of any kind**.

**Sensors with evidence:** micro-VAD as a presence detector (F-062, 0.12% chop on podcast; instrument unconfirmed, F-113); micro-VAD as a dialogue gate (unmeasured in this role).

**Deliberately left in the archive:** cockpit, JINI, Wizard, Router and Flavor, dead DSP nodes, spatial path, third upmix, cut-repair, `/master/streaming`, the daemon stack, SurrealDB, the 4-bus console design, and the segmenter with its classifier.
