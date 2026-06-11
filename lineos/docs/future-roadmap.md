# Creator OS — Future Roadmap (R&D Backlog)
# lineos/docs/future-roadmap.md
# Version: 2.0
# Date: 2026-06-11
# Status: 📋 LIVING DOCUMENT
# Owner: Lead Architect (Anestis) / Strategist: Claude

---

## Implementation Priority Order
🟢 NOW:       UI — Cockpit-Dioxus (see Section 1)
🟡 NEAR:      Corpus Learning CB-P1..P9
🟡 NEAR:      EDL Non-Destructive Editing
🔵 FUTURE:    Everything in Section 3

---

## Section 1 — UI SCOPE (Cockpit-Dioxus) 🟢 NOW

### 1.1 Flavour Presets Widget
**"Instant Sound Character"**
- 6 cards/buttons: Neutral, Warm Analog, Club Punch, Radio Edit,
  Cinematic Wide, Clean & Clear
- Instant ArcSwap switch — zero dropout
- Most impressive demo feature — sounds change in < 1ms
- **Backend:** ✅ Complete (FL-P1..P6)
- **API:** POST /mix/flavour, GET /mix/flavours

### 1.2 Sound Tinder (Swipe-to-Sound)
**"Find Your Sound in 30 Seconds"**
- 8 variation cards, user swipes Like/Nope
- AI computes weighted centroid → "My Sound" branch
- Binary choice = zero cognitive load for artist
- **Backend:** ✅ Complete (FL-T1..T4)
- **API:** GET /tinder/variations, POST /tinder/like, POST /tinder/result

### 1.3 Audio Git — A/B/C Branch Switcher
**"Keep Every Version of Your Mix"**
- Branch list UI (main, club_punch, my_sound, etc.)
- Commit history with messages
- Checkout = instant sound change via ArcSwap
- Revert button
- **Backend:** ✅ Complete (AudioRepo)
- **API:** /mix/state, /mix/commit, /mix/checkout, /mix/branch, /mix/revert

### 1.4 TB-P7: Live Telemetry (Spectrum + Goniometer)
**"See Your Sound in Real Time"**
- requestAnimationFrame loop → invoke("get_live_telemetry_realtime")
- 64-band spectrum canvas
- 32-pair Lissajous goniometer
- Auto-stop when window hidden
- **Backend:** ✅ TB-P1..P6 complete
- **Spec:** telemetry-bridge-spec-v1_3.md

### 1.5 FC-P1..P2: Per-Finding Touch Controls
**"The Scalpel, Not the Paintbrush"**
- FC-P1: FindingTouchControl + FindingPatch types
- FC-P2: Per-timestamp finding extraction from Level 3 corpus
- Scan 100ms windows where finding condition is true
- Extract start_ms, end_ms per finding → attach to HudFinding
- **Spec:** per-finding-touch-controls-spec-v1_0.md ✅

### 1.6 Phase 8d: 5.1 Spatial Mixer Widget
**"Live Control of the Spatial Field"**
- 5 faders: voice / drums / bass / harmonics / ambience
- Real-time MixLevels adjustment via Web Audio API + SSE
- SpatialFirewall integration
- **Backend:** ✅ PreviewStore complete
- **API:** POST /preview, GET /preview/:id/:stem
- **Spec:** spatial-mixer-widget-v1_0.md ✅

### 1.7 Album Mastering UI (Aether Black)
**"The Holographic Album Experience"**
- Track list with per-track progress
- EarFatigue indicator:
  "Track 2 mastered softer because Track 1 was aggressive"
- PhantomMaster distance bar per track
  "Your track is X% from album center"
- Gapless preview player (ArcSwap morphs during playback)
- Album Certificate download (PDF)
- **Backend:** ✅ AB-P1..P7 complete
- **API:** POST /master/batch, GET /album/:id/certificate.pdf

### 1.8 EarFatigue Visualization
**"AI Explains Its Decisions"**
- Per-track card showing:
  Previous track LUFS → fatigue detected → adjustments applied
  ducking_multiplier, width_multiplier values
- Makes AI decisions transparent to artist
- **Backend:** ✅ EarFatigueModel complete

### 1.9 Mastering Certificate Widget
**"Your Proof of Quality"**
- PDF download button per track
- QR code display (scan to verify)
- Album certificate with SHA-256 hash
- **Backend:** ✅ Complete
- **API:** GET /blob/:id/certificate.pdf
         GET /album/:batch_id/certificate.pdf

### 1.10 Projects & Tracks Browser
**"Your Mastering History"**
- List all projects from SurrealDB
- Per-project: track list with LUFS, flavour, date
- Click track → load into mastering session
- **Backend:** ✅ DB-P1..P5 complete
- **API:** GET /projects, POST /projects,
         GET /projects/:id/tracks

---

## Section 2 — DSP SCOPE 🟡 NEAR

### 2.1 Corpus Learning CB-P1..P9
- UserMarkovModel evolution
- Global corpus aggregation
- **Spec:** corpus-learning-spec-v1_2.md

### 2.2 WavChunkReader.seek()
- Scout reads only 30s proxy window from disk
- Completes INV-ST-3 for scout phase
- **Depends on:** Phase 2 mmap ✅

### 2.3 EDL Non-Destructive Editing
- **Spec:** edl-spec-v1_0.md

### 2.4 StoredBlob v3 + SurrealDB Persistence
- Album mastering fields:
  phantom_distance, ear_fatigue_applied,
  anchor_track_idx, morph_curve_applied, album_id
- Blobs persist to SurrealDB (survive restart)
- POST /tracks → persist to DB

---

## Section 3 — FUTURE 🔵

### 3.1 Psychoacoustic Collision Matrix
**"The End of Muddy Sound"**
- Cross-stem state comparison per chunk
- Auto micro-ducking at collision points
- **Depends on:** CB-P5..P6

### 3.2 Spatial Markov — Generative 3D / Atmos Upmix
**"Automatic Dolby Atmos from Stems"**
- Train corpus-spatial-cinema preset
- Auto upmix Stereo/Stems → 7.1.4 Atmos
- **Depends on:** CB-P1..P9 + Spatial Markov extension

### 3.3 Certificate Chain of Trust
**"Cryptographic Quality Notary"**
- SurrealDB graph: [User] → [EBU Report] → [File Hash]
- Public verification: verify.creator-os.com
- **Depends on:** SurrealDB cloud

### 3.4 Proof of Creation
**"Anti-AI-Spam Rights Management"**
- Record every DSP command as timestamped event
- Cryptographically signed manifest
- Proves human work to labels
- **Depends on:** EDL + SurrealDB cloud

### 3.5 SurrealDB Cloud Infrastructure
**"The Global Brain Backend"**
- Global corpus aggregation
- User authentication
- Graph queries for contribution tracking
- Local operation remains 100% offline

### 3.6 BPM Detector (YIN Algorithm)
- Deterministic pitch tracking
- Used for: Tinder variations, gapless transition timing
- EarFatigue: High BPM + high LUFS = maximum fatigue

### 3.7 DB-P6 Auth/Scopes
- SurrealDB multi-user scopes
- Each user sees only their projects
- **Depends on:** SurrealDB cloud infrastructure

### 3.8 Mastering Tinder UI
- Full swipe interface in Dioxus
- Audio playback per variation
- **Depends on:** 1.2 Tinder Widget + PreviewStore

### 3.9 Qualitative Tests with Real Audio
- Run INV-QA-1..3 with real mastering output
- Prove residual < -30 dBFS with actual DSP
- **Depends on:** test audio fixtures

---

## Backend Status (as of 2026-06-11)
✅ DSP Pipeline (sp314-dsp)
✅ AudioRepo + ArcSwap (zero-latency)
✅ Flavour Presets FL-P1..P6
✅ Mastering Tinder FL-T1..T4
✅ Album Mastering AB-P1..P7 (Aether Black)
✅ EarFatigueModel + MorphCurve + PhantomMaster
✅ AlbumConductor + AlbumCertificate
✅ Certificate Export (per-track + album PDF)
✅ PreviewStore (5 stems)
✅ SurrealDB DB-P1..P5
✅ CI Gates (justfile + GitHub Actions)
✅ Qualitative DSP Tests INV-QA-1..3
✅ cargo test --workspace: 0 FAILED

---

## The Full Vision
Creator OS is not just a mastering tool.
It is:
A deterministic DSP engine            ✅ done
A zero-latency audio state machine    ✅ done
A swipe-to-sound AI (Tinder)          ✅ done
A holographic album orchestrator      ✅ done
A cryptographic quality notary        🔵 future
A proof-of-human-creation system      🔵 future
A generative spatial audio engine     🔵 future
A learning AI platform (corpus)       🟡 near
Every feature builds on the previous.
Foundation: mmap + BlobStore + AudioRepo + SurrealDB

---

**Lead Architect:** Anestis
**Strategist:** Claude
**Last Updated:** 2026-06-11
