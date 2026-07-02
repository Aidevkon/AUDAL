# NOTE (Additive): Scout-Driven Platform Filtering
# lineos/docs/notes/scout-driven-platform-filtering.md
# Status: 🟡 BACKLOG — additive note, not a full spec yet
# Related: SPEC reference-driven-sonic-vision-podcast-v1_1.md (§3.5 conflict)
# Domain: JINI (Hangar) + Scout + content classification — NOT the reference resolver
# Date: 2026-07-01

---

## Why this note exists

The user journey for the reference spec exposed a real gap:

> User drops a file, picks **Spotify** as platform + **Podcast** as flavour.
> Spotify's *music* normalisation (~−14 LUFS, observed, not a codified spec)
> conflicts with the podcast delivery target (−16 LUFS, Apple art.893).
> Which wins?

Rather than add a platform-vs-content **resolution rule** to the reference
spec, the conflict is better solved **upstream**: never offer an invalid
combination in the first place.

## The idea (Anestis)

The **Scout already runs first** (the 30 s bounded `read_scout_sample` in the
O(1) Episode path, PR #38). It already classifies the content (spoken word →
Episode) and extracts the spectral profile.

Use that classification to have **JINI present only the platforms that fit
the detected content**:

- Detected podcast → offer Apple Podcasts (−16), Spotify **Podcasts** (−16),
  Broadcast/EBU (−23).
- Do **not** offer "Spotify Music (−14)" for podcast content — it never
  appears, so the conflict never arises.

JINI persona surface (illustrative, Hangar only):

> "Hi — drop a file and make yourself comfortable… ah, I see you've got an
> episode here. Which platform are you normalising for? Pick one and I'll take
> care of the rest."

The list JINI shows is **filtered by the scout classification**, not the full
static menu.

## Why this is clean

- **The conflict disappears by construction** — invalid platform/content
  combinations are never offered, so no resolution rule is needed.
- **No new heavy infrastructure** — the scout already runs and already
  classifies. The only new piece is a mapping `content_class → valid_platforms`
  and the JINI presentation filter.
- **Consistent with the reference spec's stance:** platform = the LUFS/TP hard
  constraint (Layer 1); the podcast *character* (LTASS reference) is
  **platform-agnostic** — the same reference applies whether the user ships to
  Apple or Spotify; only the loudness constraint changes.

## Architectural note (for whoever implements)

- **Run scout once, cache it.** Order becomes:
  `drop → scout (30 s, bounded) → classify + cache spectral features →
   JINI presents valid platforms → user picks → render reuses the CACHED
   scout` (no second scout pass). The scout sample is already bounded, so
  caching it keeps the whole flow O(1) and avoids double work.
- Content classification lives with **Scout / content-type detection**; the
  platform-filtering + persona copy lives with **JINI (Hangar)**. Per the
  onboarding architecture, the JINI persona surface stays in the Hangar and
  never enters the Cockpit.

## Scope boundary

- This note does **not** design the JINI scheme (that is its own work).
- It does **not** change the reference resolver.
- It records the decision that **the platform/content conflict is resolved
  upstream by scout-driven JINI filtering**, so the reference spec does not
  need a resolution rule — only a one-line pointer to this note.

## Suggested one-line pointer in the reference spec (§3.5)

> Platform/content loudness conflicts (e.g. Spotify-music −14 vs podcast −16)
> are resolved **upstream** by scout-driven JINI platform filtering (see
> `notes/scout-driven-platform-filtering.md`): invalid combinations are never
> offered, so no in-resolver rule is required. The podcast character (LTASS
> reference) is platform-agnostic; the platform contributes only the LUFS/TP
> hard constraint.

---

**Owner:** Anestis / Strategist: Claude
**Status:** Backlog note — promote to full spec when the JINI scheme work begins
