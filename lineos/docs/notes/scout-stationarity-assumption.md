# NOTE (Additive): Scout Stationarity Assumption — Known Limitation
# lineos/docs/notes/scout-stationarity-assumption.md
# Status: 🟡 BACKLOG — additive note, not a full spec yet
# Related: reference-driven-sonic-vision-podcast-v1_1.md (§3.5 scout), scout-driven-platform-filtering.md, future-roadmap.md §2.2
# Domain: Scout / PreAnalyzer — the 30s proxy analysis that locks DSP intent
# Date: 2026-07-03
---

## Why this note exists

The scout analyses a bounded 30s slice and, from that alone, configures the
DSP for the WHOLE file. This encodes a silent assumption: that the audio is
spectrally homogeneous (stationary) across its entire duration. That
assumption is fine for typical spoken word but breaks on real-world material,
and the failure is invisible — no error, just a wrong EQ curve applied to the
parts the scout never heard. This note records the assumption, where it holds
and breaks, the partial mitigation already in the code, and the planned real
fixes, so none of it is lost.

## The assumption

`read_scout_sample` returns a single 30s slice. `PreAnalyzer` / `scout_node`
derive the spectral profile (and streaming_features feeding
`DspAdapter::build_graph_only`) from that slice, and the resulting EQ/target
is applied for the full render. Implicit claim: *the 30s slice is
representative of the whole file*.

## Where it holds / where it breaks

HOLDS: stable single-source speech — same speaker, mic, and room. The typical
podcast episode. A voice's spectral signature does not drift much, so a slice
represents the rest.

BREAKS:
- Podcast with an intro jingle / music bed: 25s of hip-hop beat + then voice.
  A slice landing on the beat sets the EQ to fight bass the voice does not
  have — the voice then sounds thin / telephone-like.
- Music / DJ set with structural change: a 2-hour techno set opening with 4
  min of ambient pads (no kick). A slice with no low-end energy leads to a
  standing low-end boost; when the drop lands, that boost drives clipping.
  The 30s slice cannot represent drops that occur elsewhere.

## Current mitigation (PARTIAL — and itself a caveat)

`read_scout_sample` does NOT read the first 30s. It seeks to the MIDDLE:
`start_frame = (total_frames - scout_frames) / 2`. This dodges the
intro-jingle trap (the opening is skipped). But it is only a partial fix:

1. It does not solve mid/late structural changes (a DJ drop can be anywhere,
   not conveniently at the midpoint).
2. It depends on `total_frames_hint()`. For an MP3 WITHOUT frame metadata
   (pure CBR without Xing/LAME, live stream), the hint is `None`, and
   `read_scout_sample` early-returns `None` → `run_dsp` hard-errors with
   "could not read audio" on an otherwise valid file. See the related
   known-gap below. So the "middle" behaviour is not even deterministic
   across files: files with a hint get the middle, files without get
   rejected outright.

## Why NOT fixed now

- Fix Level 3: it changes analysis semantics, so it wants a spec, not an
  ad-hoc patch.
- For the current podcast pilot, mid-file speech is representative enough.
- The music case (where this becomes critical) belongs to Wave 3's music
  path, where it is solved properly rather than patched here.

## Planned real solutions (from R&D — do not lose these)

- Podcast (Wave 2.x): Smart VAD scout. Instead of taking the spectral centroid
  blindly over 30s, run the slice through a Voice Activity Detector (or the
  stem extractor) and derive the profile ONLY from the frames that are clean
  speech — ignoring jingle/silence within the slice. If only 2s of the 30s is
  clean voice, profile from those 2s.
- Music (Wave 3): Sparse sampling + Dynamic EQ. The Go distributor requests a
  sparse read (e.g. 60 × 1s spread across the whole file → an accurate energy
  histogram in <0.5s, fooled by neither ambient intros nor drops), and the EQ
  becomes adaptive at render time via the 64-band OLA analysis rather than a
  static decision locked by the scout. This is a core reason the 64-band FFT
  side-chain and OLA buffers are being built.

## Related known-gap (shared root cause)

MP3 without `total_frames_hint()` → `read_scout_sample` returns `None` →
`run_dsp` hard error on a valid file. Documented in
`lineos/m0/m0-daemon/tests/e2e_tier1_abort.rs`. Needs its own fix (a scout
fallback that reads a slice without knowing total length). Wave 3 streaming
(OLA) does NOT fix it — OLA is the render pass; the scout stays a bounded 30s
proxy that must know where to read from (future-roadmap.md §2.2 keeps the 30s
proxy as a distinct entity). This gap and the stationarity assumption share
the same root: the scout's dependency on `total_frames` to locate its slice.

---

**Owner:** Anestis
**Status:** Backlog note — promote to spec when Smart VAD scout (Wave 2.x) or the Wave 3 music path is scheduled.
