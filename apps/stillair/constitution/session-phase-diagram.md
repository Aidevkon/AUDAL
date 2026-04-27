# Still Air — Session Phase Diagram
# apps/stillair/constitution/session-phase-diagram.md

**Document:** `apps/stillair/constitution/session-phase-diagram.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 · LineOS Coach Constitution v1.3 · HUD Constitution v1.0
**Owner:** Lead Architect (Anestis)

---

## Overview

This document defines the canonical session state machine for Still Air (A1).
It is the authoritative reference for phase transitions, layer responsibilities,
and edge case handling.

Every phase has explicit preconditions. No phase may begin before its
preconditions are satisfied. This is a hard invariant.

---

## Phase Diagram

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                           PHASE 0 — USER INPUT                               │
└──────────────────────────────────────────────────────────────────────────────┘

User drops audio file into Hangar
        │
        ▼

┌──────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 1 — INGEST (M0 → M1 → Aether)                      │
└──────────────────────────────────────────────────────────────────────────────┘

M0 (WorkspaceEngine):
    • File boundary event
    • Path validation
        │
        ▼
LineOS M1:
    • Integrity check
    • Decode
    • Sample rate / bit depth extraction
        │
        ▼
Aether Pre-Analysis (coach-core):
    • Loudness map
    • Peak map
    • Spectral buckets
    • Stereo correlation
    • DC offset
    • Crest factor
    • Transient map
        │
        ▼
CoachFindings (platform-agnostic, source: "pre_master")
        │
        ▼

[ HANGAR — JINI FIRST CONTACT ]
    "PRE-FLIGHT COMPLETE
     Analysis ready for review."
        │
        ▼

STOP — No playback
STOP — No HUD
STOP — No thresholds
STOP — No severity

Precondition for Phase 2: CoachFindings produced, JINI acknowledged.

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 2 — USER SELECTS PLATFORM PRESET                   │
└──────────────────────────────────────────────────────────────────────────────┘

User chooses preset:
    • Spotify / Apple / YouTube / Tidal / Broadcast / RAW
        │
        ▼

Precondition for Phase 3: Preset selected.

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 3 — THRESHOLD BINDING (Rule Layer)                 │
└──────────────────────────────────────────────────────────────────────────────┘

Rule Layer receives:
    • CoachFindings (source: "pre_master")
    • Selected preset thresholds (from bmr-128.schema.json)
        │
        ▼
Threshold Binding:
    • Apply LUFS target
    • Apply True Peak ceiling
    • Apply LRA limits
    • Apply platform compliance rules
        │
        ▼
Severity Recalculation (aggregate across all findings)
        │
        ├── IF severity == high:
        │       → Emit HudPayload(trigger.source: "rule_engine", trigger.kind: "auto")
        │       → Cockpit HUD appears immediately (replace-only, no stack)
        │
        └── IF severity < high:
                → Silent state update
                → No HUD unless user requests details

Precondition for Phase 4: Threshold binding complete, severity calculated.

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 4 — PLAYBACK (Cockpit Work Layer)                  │
└──────────────────────────────────────────────────────────────────────────────┘

Playback allowed ONLY after Phase 3 completes.

Cockpit instruments update:
    • Analyzer
    • Spectral
    • Siamese Panels
    • PFR Context Badges (read-only, from HudPayload.context)

User interactions permitted:
    • Scrub / transport
    • A/B comparison
    • Manual HUD request (low / medium / info findings)
    • Preset change (see Preset Rebinding below)
    • Press MASTER (enter Phase 5)

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                   PRESET REBINDING (IN-PHASE DURING PHASE 4)                 │
└──────────────────────────────────────────────────────────────────────────────┘

IF user selects new preset WHILE phase == 4:
    → Re-enter Phase 3 logic (Threshold Binding)
    → Recalculate severity against new preset
    → IF severity == high: HUD replace (new HudPayload, trigger.kind: "auto")
    → IF severity < high: silent update, no HUD
    → Phase remains 4 (playback not interrupted)
    → PFR context badge updates to reflect new preset

CoachFindings are NOT re-run. Only threshold binding recalculates.

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     PHASE 5 — MASTERING EXECUTION (M1)                       │
└──────────────────────────────────────────────────────────────────────────────┘

Precondition: Phase 4 complete, user explicit MASTER action.

Step 1 — Rule Layer:
    • Produce MasteringManifest
        { lufs_target, true_peak_ceiling, lra_limits, platform_constraints }
    • MasteringManifest is the ONLY output of Rule Layer in this step
    • Rule Layer does not execute DSP

Step 2 — PFR state transition:
    PFR_STATE := MASTERING_EXECUTION
        • Timecode display → AMBER
        • ABORT → only active control
        • All other controls ignored by LineOS M1

Step 3 — LineOS M1 / sp314-dsp (E11):
    Receives MasteringManifest from Rule Layer via M0
    Executes deterministic 8-stage DSP chain:
        • Gain staging
        • EQ
        • Limiting
        • Stereo operations
        • Compliance enforcement
    Writes output to Golden Blob

Step 4 — Post-Master Analysis (Aether):
    Targeted compliance re-check only (not full ingest):
        • LUFS measurement
        • True Peak
        • LRA
        • Platform compliance rules
    Produces: CoachFindings(source: "post_master")
    Schema: identical to pre_master findings
    Scope: compliance fields only — no spectral, no transient map

Step 5 — Cockpit:
    Render progress only (no logic)

Step 6 — Hangar / JINI:
    Narrative summary available (Phase 4+ only)
    Per-session Finding History appended

Step 7 — PFR state transition:
    PFR_STATE := NORMAL_OPERATION

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     MASTERING ABORT PATH                                     │
└──────────────────────────────────────────────────────────────────────────────┘

IF user presses ABORT during MASTERING_EXECUTION:
    → PFR_STATE := NORMAL_OPERATION
    → Discard partial output (Golden Blob write aborted)
    → Return to Phase 4 (Playback)
    → Preserve preset selection
    → Preserve CoachFindings (source: "pre_master") — unchanged
    → HUD: no new trigger
        (severity unchanged — no new analysis occurred)

ABORT is an expected path, not an error.
No error state. No error HUD. Clean rollback to Phase 4.

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     RE-INGEST (HARD RESET FROM ANY PHASE)                    │
└──────────────────────────────────────────────────────────────────────────────┘

IF new file dropped WHILE phase ∈ {2, 3, 4, 5}:
    → Force transition to Phase 1
    → Stop playback immediately
    → Dismiss HUD (if visible)
    → Clear Siamese buffers
    → Reset PFR timecode to 00:00:00.000
    → Invalidate preset selection
    → Clear Finding History (session-scoped — see below)
    → Require new preset after ingest completes

IF Phase 5 (MASTERING_EXECUTION) is active during re-ingest:
    → Implicit ABORT (same rules as Mastering Abort Path)
    → Then proceed with Phase 1 reset

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     HANGAR — FINDING HISTORY                                 │
└──────────────────────────────────────────────────────────────────────────────┘

Hangar (JINI Panel) maintains a read-only session-scoped finding log:

    FINDING HISTORY
    ────────────────────────────────────────
    [14:32] True Peak Exceeded (+0.8 dBTP)
    [14:32] LUFS -11.2 (Target -14.0)
    [14:33] Stereo Correlation Weak (0.21)
    [14:45] Post-Master: LUFS -14.0 ✓
    [14:45] Post-Master: True Peak -1.0 ✓

Scope rules:
    • Session-scoped — cleared on re-ingest (Phase 1 hard reset)
    • Cleared on new session
    • NOT persistent across sessions
    • NOT a database — persona-level memory only

Invariants:
    • History does not affect severity
    • History does not affect Rule Layer state
    • History does not affect HUD payload
    • HUD remains stateless — history lives only in Hangar

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     LAYER RESPONSIBILITIES (IMMUTABLE)                       │
└──────────────────────────────────────────────────────────────────────────────┘

Layer                   Owns                                    Does NOT own
──────────────────────────────────────────────────────────────────────────────
M0 / WorkspaceEngine    File I/O, ingest events                 DSP, analysis
LineOS M1               Decode, DSP chain, Golden Blob          Findings, HUD
Rule Layer (Aether)     CoachFindings, MasteringManifest,       DSP, rendering
                        HudPayload construction, severity
Cockpit Work Layer      Playback, Siamese, Analyzer             Findings logic
HUD Module              Render HudPayload (stateless)           State, logic
PFR / Transport Bar     System truth, PFR_STATE, ABORT          Findings, persona
Hangar                  JINI persona surface, Finding History   Cockpit state

──────────────────────────────────────────────────────────────────────────────

┌──────────────────────────────────────────────────────────────────────────────┐
│                     LAYER BOUNDARIES (IMMUTABLE)                             │
└──────────────────────────────────────────────────────────────────────────────┘

[ PFR / Transport Bar ]     ← LineOS M1 — system truth, mastering lock
[ Cockpit Work Layer ]      ← Playback, Analyzer, Siamese, HUD render
[ HUD Module ]              ← Stateless findings overlay (presentation only)
───────────────────────────────────────────────────────────────────────────
[ Hangar ]                  ← JINI persona surface + session Finding History
───────────────────────────────────────────────────────────────────────────
[ Aether / Rule Layer ]     ← CoachFindings, Threshold Binding, Manifests
───────────────────────────────────────────────────────────────────────────
[ M0 / WorkspaceEngine ]    ← I/O, ingest events, sandbox

The JINI persona surface is confined to the Hangar.
The cockpit sees findings, not the persona.

──────────────────────────────────────────────────────────────────────────────

## Invariants (Unbreakable)

1. No playback before Phase 3 completes.
2. No mastering before Phase 4 (explicit user action required).
3. PFR_STATE := MASTERING_EXECUTION blocks all controls except ABORT.
4. ABORT is a clean rollback — no error state, no error HUD.
5. Re-ingest hard resets all session state including Finding History.
6. CoachFindings schema is identical for pre_master and post_master — source tag only.
7. Post-master analysis is targeted (compliance only) — not a full ingest.
8. MasteringManifest is the only Rule Layer output in Phase 5 — Rule Layer never executes DSP.
9. HUD is stateless and replace-only — no stacking, no queue.
10. JINI persona surface never enters the cockpit.
11. Finding History is session-scoped — not persistent, not a database.
12. Preset rebinding recalculates severity only — CoachFindings are not re-run.

──────────────────────────────────────────────────────────────────────────────

**Lead Architect:** Anestis
**System:** Creator OS / Still Air (A1)
**Document:** `apps/stillair/constitution/session-phase-diagram.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED
