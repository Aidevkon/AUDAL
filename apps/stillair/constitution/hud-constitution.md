# creator-os/contracts/hud-constitution.md

# Cockpit HUD Module Constitution v1.0
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.5 · LineOS Coach Constitution v1.3 · LLM Adapter Amendment v1.1
**Owner:** Lead Architect (Anestis)
**Date:** 2026-04-27
**Contract:** `creator-os/contracts/hud-payload.schema.json` v1.0

---

## §1 — Identity

### §1.1 What the HUD is

The HUD Module is a Cockpit Presentation Layer component.

It is:
- A render-only overlay
- Ephemeral and on-demand
- A visual surface for CoachFindings
- The ONLY mechanism by which Rule Layer output enters the main viewport

It is not:
- A persona surface
- A business logic layer
- A state owner
- A rule engine
- A JINI component
- A PFR component

### §1.2 The Canonical Separation

```
[ TRANSPORT BAR / PFR ]     ← LineOS M1 — deterministic system truth
[ SIAMESE / WORK LAYER ]    ← Cockpit — analysis, chain, session
[ HUD MODULE ]              ← Findings overlay — presentation only
-----------------------------------------------
[ HANGAR ]                  ← JINI persona surface — never enters cockpit
```

The JINI persona surface is confined to the Hangar.
The cockpit sees findings, not the persona.

---

## §2 — Layer Boundary (Immutable)

The HUD Module belongs exclusively to the Cockpit Presentation Layer.

**Permitted:**
- Rendering `HudPayload` (from `hud-payload.schema.json`)
- Displaying severity indicators (Stepped Pulse)
- Displaying findings with params
- Displaying context badges (from PFR state — read-only)
- Firing action events (dispatched by ID — never interpreted)
- Displaying short deterministic guidance strings (resolved from `guidance_key`)

**Forbidden:**
- Business logic of any kind
- Interpreting finding severity (reads it, never derives it)
- Calling Rule Engine, JINI, or Aether directly
- Holding finding state (resolved/unresolved lives in Rule Layer)
- Displaying persona, narrative, or JINI identity
- Displaying NarrativeResponse output
- Accessing raw audio
- Accessing Golden Blob directly
- Auto-dismiss for `severity: high` (no timeout permitted)
- Modifying the `HudPayload` it received

---

## §3 — Input Contract

The HUD Module accepts exactly one input type: `HudPayload`.

**Schema:** `creator-os/contracts/hud-payload.schema.json` v1.0

The HUD must validate the incoming payload against the schema at render time.
Schema validation failure = hard error. HUD does not render partial payloads.

```
Rule Layer (coach-core)
        │
        │  HudPayload (schema-validated)
        ▼
HUD Module (render only)
```

No other data source is permitted. The HUD has no direct access to:
- `CoachFindings` raw struct
- `AnalysisReport`
- `GoldenBlob`
- PFR state (context is passed through `HudPayload.context`)
- WorkspaceEngine state

---

## §4 — Trigger Rules (Hybrid Model)

### §4.1 Auto-trigger (Rule Engine → HUD)

Condition: `severity == high` in any finding from the current ingest.

The Rule Engine emits a `HudPayload` with `trigger.source: "rule_engine"` and
`trigger.kind: "auto"`. The Cockpit renders the HUD immediately.

No user action required. No confirmation. Immediate display.

This is a safety invariant: high severity findings must never be silently ignored.

### §4.2 Manual trigger (User → HUD)

Condition: User explicitly requests detail view (UI button, keyboard shortcut,
hardware button) when `severity == low` or `severity == medium`.

The Cockpit dispatches a request to the Rule Layer, which responds with a
`HudPayload` with `trigger.source: "user_request"` and `trigger.kind: "manual"`.

For `severity == info`: manual trigger only, and `lifecycle.auto_dismiss_ms`
applies (default: 6000ms).

### §4.3 Re-ingest trigger

When a new ingest event occurs (new file, session replace, explicit Re-Analyze),
the prior HUD is dismissed immediately. A new `HudPayload` is generated once
the ingest pipeline completes.

`trigger.source: "re_ingest"`, `trigger.kind: "manual"`.

### §4.4 Trigger ownership

The Rule Engine (coach-core) is the sole authority for emitting `HudPayload`.
The Cockpit never constructs a `HudPayload` itself.
The HUD never requests its own data.

---

## §5 — Lifecycle

### §5.1 Dismiss rules

| Severity | Auto-dismiss | Persist until resolved | Manual dismiss |
|----------|-------------|------------------------|----------------|
| `info`   | Yes — 6000ms default | No | Always available |
| `low`    | Optional — 8000ms | No | Always available |
| `medium` | Optional — 8000ms | Optional | Always available |
| `high`   | **NEVER** | Yes (default) | Always available |

Auto-dismiss for `high` is a constitutional violation.

### §5.2 Resolved state

Finding resolved/unresolved state lives in the Rule Layer, not the HUD.

When a user fires an action (e.g. "Apply Fix"), the HUD dispatches the
`action.id` as an event. The Rule Layer processes the action and, if
the finding is resolved, emits a new `HudPayload` (or an explicit dismiss
signal). The HUD never decides when a finding is resolved.

### §5.3 Multiple findings

A single `HudPayload` may carry up to 8 findings. The HUD renders all of
them. The `severity` field of the payload is the aggregate (highest severity
across findings). Stepped Pulse is driven by the aggregate severity.

### §5.4 Stacking

Only one HUD instance is visible at a time.
If a new `HudPayload` arrives while the HUD is visible, the prior HUD is
replaced. No stacking. No queue. The new payload supersedes the old.

---

## §6 — Severity Display: The Stepped Pulse

The HUD uses a quantized severity indicator. No breathing. No continuous
animation. Discrete steps only.

### §6.1 Stepped Pulse spec

```css
/* Stepped Pulse — 4 discrete states, quantized via steps() */
/* Forbidden: ease, linear, any continuous animation */

@keyframes stepped-pulse {
  0%   { opacity: 1.0; }
  50%  { opacity: 0.4; }
  100% { opacity: 1.0; }
}

.severity-info   { animation: none; }
.severity-low    { animation: stepped-pulse 2.4s steps(2, end) infinite; }
.severity-medium { animation: stepped-pulse 1.6s steps(2, end) infinite; }
.severity-high   { animation: stepped-pulse 0.8s steps(2, end) infinite; }
```

### §6.2 Color mapping (from Design Tokens)

| Severity | Token | HEX |
|----------|-------|-----|
| `info`   | `color.severity.info` | #00AAFF |
| `low`    | `color.severity.low` | #00CC66 |
| `medium` | `color.severity.medium` | #FFAA00 |
| `high`   | `color.severity.high` | #FF3B30 |

HEX values are canonical (Design Tokens v1.0).
OKLCH conversion at render time — see `cockpit/design-system/appendix-a-oklch.md`.

### §6.3 Visual standards (Glass Cockpit)

Per the Jini HUD & PFR Fault Architecture spec:

- Transparency: 70–85% opacity
- Inner border: 1px solid #1a1a1a (Mechanical Bezel)
- Outer border: 1–1.5px #00A3A3 (Desaturated Teal — non-neon)
- Corner radius: 2px max
- Zero blur
- Zero box-shadow

---

## §7 — JINI Strip (Phase dependency)

### §7.1 Phase 1 / 1.2 (no NarrativeResponse)

The JINI strip is absent. The HUD bottom area shows a minimal
deterministic status line resolved from `guidance_key`:

```
ANALYSIS COMPLETE — [finding count] FINDINGS — [preset_name]
```

No persona. No name. No narrative. Pure findings summary.

### §7.2 Phase 4+ (NarrativeResponse available — future)

Progressive enhancement only. If `NarrativeResponse` is available,
the HUD may display a 1–2 line summary in the bottom strip.

**Rules:**
- NarrativeResponse text is supplemental — never replaces structured findings
- The strip never changes severity display
- The strip never adds new findings
- The strip never uses the JINI name or persona identity
- NarrativeResponse must have passed `validate_output()` before HUD receives it

The HUD never calls JINI, Aether, or adapter-runtime directly.
NarrativeResponse arrives pre-validated via `HudPayload` extension (Phase 4 amendment required).

---

## §8 — Ingest Event Integration

The HUD is downstream of the ingest pipeline. It has no knowledge of ingest internals.

```
User drops file
      │
      ▼
M0 boundary event
      │
      ▼ (LineOS M1 integrity + decode)
      │
      ▼ (Aether pre-analysis)
      │
      ▼
CoachFindings (Rule Engine)
      │
      ▼
HudPayload (emitted by Rule Layer)
      │
      ▼
HUD Module (render)
```

The HUD is only populated after the full ingest pipeline completes.
There is no "loading" HUD. There is no "partial findings" HUD.
Either the `HudPayload` is complete and schema-valid, or the HUD does not render.

---

## §9 — Invariants (Unbreakable)

1. The HUD is render-only. It never modifies data.
2. The HUD has no state. Finding state lives in the Rule Layer.
3. The HUD never displays persona, narrative, or JINI identity.
4. The HUD never constructs its own `HudPayload`.
5. The HUD never auto-dismisses `severity: high`.
6. The HUD never renders a partial or schema-invalid payload.
7. The Stepped Pulse uses `steps()` only — no continuous animation.
8. Only one HUD instance is visible at a time.
9. The JINI persona surface is confined to the Hangar.
10. The cockpit sees findings, not the persona.

---

## §10 — Amendment Process

Changes to invariants, the input contract, or layer boundaries require:
- Clear justification
- Impact analysis on determinism and layer separation
- Major version bump
- Update of `hud-payload.schema.json` if contract changes
- Update of all referencing documents

---

**Lead Architect:** Anestis
**System:** Creator OS / LineOS Cockpit
**Document:** `creator-os/contracts/hud-constitution.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED
