# NOTE (R&D): Voice Character Presets
# lineos/docs/notes/voice-character-presets-rnd.md
# Status: 🔬 R&D / 🟡 BACKLOG — design note, not implemented
# Related: reference-driven-sonic-vision-podcast-v1_1.md (§2.6, the pilot pattern)
#          scout-driven-platform-filtering.md (JINI lifecycle)
# Domain: Sonic Vision — aesthetic character layer (Layer 2 intent)
# Date: 2026-07-02

---

## 0. What this is

The podcast pilot (`reference-driven-sonic-vision-podcast-v1_1.md`) ships a
single **neutral reference**: the LTASS shape (Byrne 1994) that brings any
voice to the measured baseline of natural speech. That answers *"make my
voice correct."*

This note designs the **next layer**: named **voice characters** that answer
*"make my voice sound like ___"* — Late Night Radio, News Anchor, Warm
Intimacy. Each is an aesthetic **deviation from the neutral LTASS baseline**,
with a documented acoustic mechanism.

The names in the UI are **literal, not metaphorical**: "Late Night Radio"
says exactly what the preset does. The user hears the name and knows the
result — no jargon (`cinematic_wide`), no explanation needed.

---

## 1. Where it sits in the pipeline (the ordering matters)

```
drop file
  → scout (30s, cached)                    [content detection]
  → JINI: "which platform?"                [scout-filtered options]
  → LTASS mastering                        [NEUTRAL baseline — correct]
  → JINI: "want a voice character?"        [optional, this note]
  → character filter                       [FLAVOUR — style]
  → platform loudness constraint           [LAW]
  → certificate
```

**Character applies AFTER LTASS, never before.** This is a hard ordering
decision:

- LTASS first brings *every* input (muddy, thin, whatever) to the same known
  neutral baseline.
- The character then applies its deviation on top of a **known** starting
  point, so "Late Night Radio = +proximity" always means the same thing.
- If character applied *before* LTASS, it would stack on an unknown input
  (muddy + bass boost = double bass = chaos), and LTASS would then try to
  correct it — fighting and cancelling the character.

Rule: **straighten first (LTASS), then style (character).** You cannot style
a crooked line.

---

## 2. The two-tier honesty (same as the pilot)

Each character has:

- **A public, citable MECHANISM** (Tier 1.5): proximity effect,
  intelligibility presence, low-mid warmth — real acoustics/psychoacoustics.
- **Proprietary NUMBERS** (Tier 2 craft): the exact dB/Q per band. No
  standards body publishes "Radio DJ = X dB at Y Hz" (verified — see §6).
  The amounts are tuned craft, documented internally, not published.

So the certificate can say: *"Late Night Radio: low-frequency emphasis
below 200 Hz (basis: microphone proximity effect); exact curve is Creator OS
craft."* Mechanism public, recipe proprietary. "In dark, not hidden."

**Important distinction from the LTASS pilot:** LTASS had *measured numbers*
(Byrne table) → Tier 1. These characters have *mechanism only* → the numbers
are craft. Do not present character numbers as if they were a standard.

---

## 3. The three pilot characters (design matrix)

Band layout (from the 8-band analysis profile):

| idx | band | range (Hz) | CF (Hz) |
|----:|------|-----------|--------:|
| 0 | Sub | 20–80 | 50 |
| 1 | Bass | 80–250 | 150 |
| 2 | LowMid | 250–500 | 350 |
| 3 | MidLow | 500–1000 | 750 |
| 4 | MidHigh | 1000–2000 | 1500 |
| 5 | HighMid | 2000–4000 | 3000 |
| 6 | Treble | 4000–8000 | 6000 |
| 7 | Air | 8000–20000 | 12000 |

### 3.1 🎙️ Late Night Radio

*The deep, intimate FM-DJ voice. Big low end, close and warm.*

- **Mechanism:** microphone **proximity effect** — close use of a directional
  (cardioid/figure-8) mic raises low-frequency response, strongest below
  ~200 Hz. This is what makes radio DJs sound "bigger" and more intimate.
  Physics-driven (pressure-gradient capsule), not a fixed curve.
- **Spectral move:** boost Bass/Sub, slight low-mid weight, gentle high tame.

| band | draft gain (dB) | Q | why |
|------|----------------:|----|-----|
| Sub (50) | +2.0 | 0.7 | subharmonic weight |
| Bass (150) | +3.5 | 0.7 | proximity core (<200 Hz) |
| LowMid (350) | +1.0 | 0.7 | "chest"/closeness |
| HighMid (3000) | −1.0 | 0.9 | soften edge, keep intimate |
| Air (12000) | −0.5 | 0.7 | pull back brightness |

### 3.2 👔 News Anchor

*Crisp, articulate, authoritative. Every consonant clear.*

- **Mechanism:** the **presence region** (2–5 kHz, esp. 2.5–4 kHz) carries
  consonant energy and aligns with peak auditory sensitivity for speech
  intelligibility. A broad presence lift = clarity/articulation. Backed by
  intelligibility research + broadcast practice (not a formal EQ standard).
- **Spectral move:** presence boost, controlled low end (no mud), slight air.

| band | draft gain (dB) | Q | why |
|------|----------------:|----|-----|
| Bass (150) | −1.0 | 0.7 | tighten, remove boom |
| LowMid (350) | −1.5 | 0.8 | de-mud for clarity |
| HighMid (3000) | +3.0 | 0.9 | presence / articulation |
| Treble (6000) | +1.5 | 0.8 | consonant definition |
| Air (12000) | +1.0 | 0.7 | broadcast sheen |

### 3.3 ☕ Warm Intimacy

*Close, soft, unhurried. For interviews and deep talks.*

- **Mechanism:** **low-mid fullness** (120–250 Hz body, some 250–400 Hz
  "chest") reads as fuller/nearer; **softened highs** (gentle 3–8 kHz cut)
  reduce edge and listener fatigue while keeping enough 2–4 kHz for
  intelligibility. Perceptual convention, close-mic expressive effect.
- **Spectral move:** low-mid warmth, tamed harshness, preserved intelligibility.

| band | draft gain (dB) | Q | why |
|------|----------------:|----|-----|
| Bass (150) | +2.0 | 0.7 | body/warmth |
| LowMid (350) | +1.5 | 0.7 | "chest", nearness |
| HighMid (3000) | +0.5 | 0.9 | keep just enough clarity |
| Treble (6000) | −2.0 | 0.8 | soften edge (anti-fatigue) |
| Air (12000) | −1.5 | 0.7 | reduce harsh top |

> All numbers above are **draft craft starting points**, not standards. Tune
> against real voices; the mechanism (which bands, which direction) is the
> defensible part, the exact dB is proprietary.

---

## 4. Implementation shape (when it happens)

Reuses the pilot pattern exactly — **each character is a new JSON profile**:

- `shared/schema/reference-profiles/character-late-night-radio.json`
- `character-news-anchor.json`
- `character-warm-intimacy.json`

Same `ReferenceProfile` struct, same `ReferenceResolver`, same firewall
protection, same `EqSource` provenance (they'd carry a new
`EqSource::Character` variant, or reuse `Reference` with a profile id in the
ProofLog). Same test pattern (balanced/muddy/thin + firewall attack).

**Key difference from the LTASS profile:** these targets are *deviations*,
not a *neutral baseline*. Two design options to resolve in the spec:

- **Option A — absolute character target:** the character JSON encodes the
  full desired shape (LTASS + deviation baked in). The resolver runs once
  against the character profile instead of the neutral one.
- **Option B — additive deviation layer:** LTASS runs first (neutral), then
  the character applies its deviation as a second, smaller pass. Preserves
  the "straighten then style" ordering explicitly and keeps the neutral
  baseline reusable.

Recommendation: **Option B** — it matches §1's ordering, keeps LTASS as the
single source of "correct", and makes each character a thin, inspectable
deviation. But this needs its own spec (fix level 3) before building.

### Extension structure (beyond the pilot 3)

The matrix in §3 is a template. New characters = new rows + new JSON:
- 📻 "Vintage Tape" (rolled highs + low-mid saturation feel)
- 🔊 "Podcast Punch" (tight, forward, modern)
- 🌙 "ASMR Close" (extreme proximity, whisper-friendly)
Each needs its own mechanism justification (or it's pure taste → Tier 2,
fine, but label it honestly).

---

## 5. Naming & UX (positioning)

- **Names are literal and self-explanatory.** "Late Night Radio" > any
  internal codename. The user picks by *desired result*, not by learning a
  vocabulary.
- **The moat is authenticity, not transformation.** Competitors (Adobe
  Podcast, Descript) use generative AI that *repaints* the voice from
  scratch — risking a robotic result, lost accent/lisp, "underwater" timbre.
  Creator OS applies deterministic EQ to the user's **real** voice: keeps
  100% of the organic timbre, adds depth/air/presence. The pitch is *"the
  best version of your real voice,"* not *"a filter that changes you."*
- JINI offers the character step **optionally, after** LTASS mastering — the
  neutral master is always available; the character is a choice on top.

---

## 6. Primary-source register (what's citable, what's craft)

- **Proximity effect** (Late Night Radio): manufacturer + engineering
  literature (e.g. Neumann technical notes on directional-mic low-frequency
  rise; pressure-gradient capsule physics). Strongest below ~200 Hz;
  cardioid/figure-8 only, absent on omni. → mechanism citable, dB not.
- **Presence / intelligibility** (News Anchor): 2–5 kHz consonant/clarity
  region, supported by speech-intelligibility research and broadcast
  practice. → region citable, exact curve not.
- **Low-mid warmth / softened highs** (Warm Intimacy): perceptual convention
  (low-mid = fuller/nearer; reduced 3–8 kHz = less fatigue). → direction
  citable as convention, numbers not.

**Explicitly NOT found (honest gap):** no AES/ITU/EBU standard mandating an
EQ contour for any of these characters. The mechanisms are real; the exact
amounts are production heuristics / craft. Any character JSON must flag its
numbers as craft, not standard (INV-REF-5 discipline: no unattributed magic
numbers — here the attribution is "craft, mechanism per §6").

---

## 7. Scope boundary

- This note is **R&D + design only** — no code, no spec approval yet.
- Building it needs its own spec (Option A vs B, EqSource::Character, per-
  character JSON + tests) — fix level 3, spec-first per Dev Protocol §7.
- Belongs after the pilot is validated on real voices and after (or with)
  the JINI lifecycle work (scout → platform → LTASS → **character**).

---

**Owner:** Anestis / Strategist: Claude
**Status:** 🔬 R&D backlog — promote to spec when the character layer is scheduled
