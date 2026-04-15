# Cockpit — Design Tokens v1.0

**Document:** `apps/stillair/cockpit/design-system/design-tokens-v1.0.md`
**Version:** 1.0
**Date:** 2026-04-14
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 · Cockpit State Machine v1.0
**Rendering spec:** `appendix-a-oklch.md` (OKLCH conversion pipeline)

---

## 1. Purpose

This document defines the canonical color tokens for the Still Air Cockpit.

All values are defined in HEX (sRGB). These are the contractual values.
OKLCH conversion happens at render-time per `appendix-a-oklch.md` §A3.
No component or spec may reference OKLCH values directly — only these tokens.

---

## 2. Token Rules

- Tokens are immutable. Changes require a constitution amendment + version bump.
- All tokens are HEX (sRGB). No HSL, no RGB tuples, no OKLCH in token definitions.
- Every color used in the Cockpit UI must reference a token. No inline hex literals.
- The rendering engine converts to OKLCH internally — the UI never sees OKLCH.

---

## 3. Background Tokens

```css
--bg-base:        #111318;   /* App background — deepest layer */
--bg-panel:       #15181f;   /* MFD panel background */
--bg-elevated:    #1e2128;   /* Cards, dropdowns, modals */
--bg-overlay:     #0d0f14;   /* Fault overlay, export lock */
```

---

## 4. Panel Accent Tokens

Each MFD panel has one canonical accent color.
Used for: panel title text, title border-bottom, active state indicators.

```css
--accent-session:   #FFD60A;   /* Left MFD  — The Session  — Amber */
--accent-insights:  #00d1ff;   /* Center MFD — The Insights — Cyan */
--accent-coach:     #4ade80;   /* Right MFD  — The Coach   — Green */
```

---

## 5. Severity Tokens

Used by the Coach panel and rule-engine findings display.
Mapping to rule-engine `Severity` enum is canonical and immutable.

```css
--severity-high:    #ef4444;   /* Severity::High   — red    */
--severity-medium:  #f59e0b;   /* Severity::Medium — amber  */
--severity-low:     #fde047;   /* Severity::Low    — yellow */
--severity-info:    #6b7280;   /* Severity::Info   — gray   */
```

**Severity → token mapping (canonical):**

| Severity | Token | Usage |
|----------|-------|-------|
| `High` | `--severity-high` | True peak exceeded, phase issues |
| `Medium` | `--severity-medium` | LUFS compliance, DC offset |
| `Low` | `--severity-low` | Dynamic range, minor loudness |
| `Info` | `--severity-info` | LRA advisory, informational |

---

## 6. Text Tokens

```css
--text-primary:     #e8eaf0;   /* Main readable text */
--text-secondary:   #8b8fa8;   /* Labels, metadata, secondary info */
--text-muted:       #4a4d5e;   /* Disabled states, placeholders */
--text-inverse:     #111318;   /* Text on bright accent backgrounds */
```

---

## 7. Border Tokens

```css
--border-subtle:    #2A2D35;   /* Panel dividers, grid lines */
--border-active:    #3d4155;   /* Focused/active element borders */
--border-accent:    currentColor; /* Inherits panel accent color */
```

---

## 8. State Tokens

```css
--state-idle:       #4a4d5e;   /* FM0 — cold & dark */
--state-active:     #4ade80;   /* FM1-FM1.5 — online */
--state-running:    #00d1ff;   /* FM2 — mastering in progress */
--state-complete:   #FFD60A;   /* FM3-FM5 — mastered */
--state-fault:      #ef4444;   /* FM-ERR — fault */
--state-locked:     #374151;   /* FM6 — export locked */
```

---

## 9. Transport Bar Tokens

```css
--btn-master-bg:       #FFD60A;
--btn-master-text:     #111318;
--btn-master-disabled: #2A2D35;
--btn-abort-bg:        #ef4444;
--btn-abort-text:      #ffffff;
--btn-neutral-bg:      #1e2128;
--btn-neutral-text:    #8b8fa8;
--btn-neutral-border:  #2A2D35;
```

---

## 10. Typography Tokens

```css
--font-mono:   'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace;
--font-sans:   'Inter', 'SF Pro Display', system-ui, sans-serif;

--text-xs:     0.625rem;   /* 10px — LED labels, micro metadata */
--text-sm:     0.75rem;    /* 12px — panel titles, tags */
--text-base:   0.875rem;   /* 14px — body text */
--text-lg:     1rem;       /* 16px — values, readings */
--text-xl:     1.25rem;    /* 20px — primary metrics (LUFS display) */
--text-2xl:    1.5rem;     /* 24px — large readings */

--tracking-wide:   0.05em;
--tracking-wider:  0.1em;
--tracking-widest: 0.15em;  /* Panel titles */
```

---

## 11. Spacing Scale

```css
--space-1:   0.25rem;   /*  4px */
--space-2:   0.5rem;    /*  8px */
--space-3:   0.75rem;   /* 12px */
--space-4:   1rem;      /* 16px */
--space-5:   1.25rem;   /* 20px */
--space-6:   1.5rem;    /* 24px */
--space-8:   2rem;      /* 32px */
```

---

## 12. CSS Custom Properties Block (Agent Reference)

The agent MUST declare all tokens in `index.html` or `styles.css` as CSS
custom properties under `:root`. No inline hex literals in component CSS.

```css
:root {
    /* Backgrounds */
    --bg-base:           #111318;
    --bg-panel:          #15181f;
    --bg-elevated:       #1e2128;
    --bg-overlay:        #0d0f14;

    /* Panel accents */
    --accent-session:    #FFD60A;
    --accent-insights:   #00d1ff;
    --accent-coach:      #4ade80;

    /* Severity */
    --severity-high:     #ef4444;
    --severity-medium:   #f59e0b;
    --severity-low:      #fde047;
    --severity-info:     #6b7280;

    /* Text */
    --text-primary:      #e8eaf0;
    --text-secondary:    #8b8fa8;
    --text-muted:        #4a4d5e;
    --text-inverse:      #111318;

    /* Borders */
    --border-subtle:     #2A2D35;
    --border-active:     #3d4155;

    /* State indicators */
    --state-idle:        #4a4d5e;
    --state-active:      #4ade80;
    --state-running:     #00d1ff;
    --state-complete:    #FFD60A;
    --state-fault:       #ef4444;
    --state-locked:      #374151;

    /* Transport buttons */
    --btn-master-bg:     #FFD60A;
    --btn-master-text:   #111318;
    --btn-abort-bg:      #ef4444;
    --btn-abort-text:    #ffffff;
    --btn-neutral-bg:    #1e2128;
    --btn-neutral-text:  #8b8fa8;
    --btn-neutral-border:#2A2D35;

    /* Typography */
    --font-mono:  'JetBrains Mono', 'Fira Code', monospace;
    --font-sans:  'Inter', system-ui, sans-serif;

    /* Spacing */
    --space-1: 0.25rem;
    --space-2: 0.5rem;
    --space-3: 0.75rem;
    --space-4: 1rem;
    --space-6: 1.5rem;
    --space-8: 2rem;
}
```

---

## 13. Forbidden Patterns

```
❌ Inline hex literals in component CSS (must use token variables)
❌ OKLCH values in token definitions (OKLCH is render-time only)
❌ HSL or RGB tuples as token values
❌ Hardcoded font family strings in components (use --font-mono / --font-sans)
❌ px units for text sizes (use rem scale)
❌ px units for spacing (use --space-N scale)
❌ Severity colors used for non-severity purposes
❌ Panel accent colors used outside their designated panel
❌ New tokens added without a version bump
```

---

## 14. Rendering Pipeline Reference

OKLCH conversion: `apps/stillair/cockpit/design-system/appendix-a-oklch.md`

```
token HEX value
    │
    ▼ (render-time, deterministic, libm-only)
OKLCH → browser paint
```

The agent does not implement the OKLCH pipeline in Phase 5.
CSS custom properties are sufficient. OKLCH pipeline is Phase 6+.

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-04-14 | Initial canonical token set — backgrounds, panel accents, severity, text, borders, state indicators, transport, typography, spacing |

---

**Lead Architect:** Anestis
**System:** Creator OS — Still Air (A1)
**Document:** `apps/stillair/cockpit/design-system/design-tokens-v1.0.md`
**Version:** 1.0
**Status:** 🔒 LOCKED
