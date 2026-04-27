# Task 6.5 — Motorized Intent Bay Reveal
# apps/stillair/cockpit/components/intent-bay/

**Document:** `apps/stillair/cockpit/tasks/task-6.5-intent-bay.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED — AGENT TASK
**Role:** Senior Dioxus Engineer
**Authority:** HUD Constitution v1.0 · Session Phase Diagram v1.0

---

## Objective

Implement the IntentBay component — the motorized "drawer" that reveals
the four macro-intent knobs (Tone / Dynamics / Space / Loudness) with
a hydraulic damper reveal mechanic and a saturation dim on the background
instruments.

---

## Component Identity

```
IntentBay
├── Role: Cockpit Presentation Layer — render-only
├── Owner: Cockpit (Siamese / MainChassis subtree)
├── Z-Index: above Hangar (JiniPanel), inside MainChassis
├── State: driven externally (HudPayload.trigger or user action)
└── Zero business logic — receives open: bool signal only
```

The IntentBay does NOT decide when to open.
It receives a signal and renders accordingly.
All trigger logic lives in the Rule Layer.

---

## Z-Index Stack (Immutable)

```
z-index: 100  PFR Transport Bar        ← NEVER covered
z-index:  60  HUD Overlay              ← findings projection
z-index:  40  IntentBay (open state)   ← covers Hangar 10–15% only
z-index:  20  Siamese / WorkLayer      ← dims during IntentBay open
z-index:  10  Hangar (JiniPanel)       ← partially covered when open
z-index:   1  MainChassis background
```

---

## Visual Specification

### Surface Texture

The IntentBay drawer surface must match the MainChassis:
- Background: `#080c10` (dark brushed steel equivalent)
- Subtle noise texture via SVG filter (no external image dependency):

```rust
// SVG filter for chassis texture — inline in component
let noise_filter = r#"
<filter id="chassis-noise" x="0%" y="0%" width="100%" height="100%">
  <feTurbulence type="fractalNoise" baseFrequency="0.65" numOctaves="3"
                stitchTiles="stitch" result="noise"/>
  <feColorMatrix type="saturate" values="0" in="noise" result="grey"/>
  <feBlend in="SourceGraphic" in2="grey" mode="overlay" result="blend"/>
  <feComposite in="blend" in2="SourceGraphic" operator="in"/>
</filter>
"#;
```

- Top edge: `border-top: 1px solid #1a2e42`
- Engraved pattern: fine horizontal lines at 4px intervals, 3% opacity
  ```css
  background-image: repeating-linear-gradient(
    0deg,
    transparent,
    transparent 3px,
    rgba(255,255,255,0.018) 3px,
    rgba(255,255,255,0.018) 4px
  );
  ```

### Hydraulic Damper Illustration

Render a central mechanical element on the drawer top edge — purely decorative SVG,
communicates the "drawer has physical weight and resistance".

```
Drawer top edge:
┌────────────────────────────────────────────────────────────────────┐
│         │    │                                         │    │      │
│   ═══╪═══   │  ←── tension spring (left)              │         │  │
│   ║PISTON║  │                                    ║PISTON║   │     │
│   ═══╪═══   ├── hydraulic cylinder (center) ────┤         │      │
│         │    │                                         │    │      │
└────────────────────────────────────────────────────────────────────┘
```

SVG spec (inline, no external assets):
```rust
// Damper SVG — centered on drawer top edge
// Dimensions: 120px wide × 16px tall
// Color: #1a2e42 (chassis metal tone)
// Elements:
//   - Left spring: zigzag path, 5 teeth, stroke #1e3448 1px
//   - Center cylinder: rect 8×12px, fill #0e1c28, stroke #1a2e42
//   - Center piston rod: rect 2×6px, fill #2a3e52
//   - Right spring: mirror of left
//   - Hydraulic lines: 1px horizontal lines from cylinder edges
let damper_svg = r#"
<svg width="120" height="16" viewBox="0 0 120 16">
  <!-- Left spring -->
  <polyline points="10,8 16,4 22,12 28,4 34,12 40,8"
            fill="none" stroke="#1e3448" stroke-width="1"/>
  <!-- Left hydraulic line -->
  <line x1="40" y1="8" x2="50" y2="8" stroke="#162738" stroke-width="1"/>
  <!-- Center cylinder -->
  <rect x="50" y="4" width="20" height="8" fill="#0e1c28" stroke="#1a2e42" stroke-width="1"/>
  <!-- Piston rod -->
  <rect x="58" y="2" width="4" height="12" fill="#2a3e52"/>
  <!-- Right hydraulic line -->
  <line x1="70" y1="8" x2="80" y2="8" stroke="#162738" stroke-width="1"/>
  <!-- Right spring -->
  <polyline points="80,8 86,4 92,12 98,4 104,12 110,8"
            fill="none" stroke="#1e3448" stroke-width="1"/>
</svg>
"#;
```

---

## Animation Specification

### Hydraulic Damper Easing

```
Motion profile: fast initial travel → controlled deceleration → clean stop
Easing: cubic-bezier(0.4, 0, 0.2, 1)
Duration open:  340ms
Duration close: 260ms (faster close — drawer retracts under spring tension)
```

NO overshoot. NO bounce. NO spring toy behavior.
The drawer has mechanical weight. It stops precisely where it should.

### CSS Implementation

```css
.intent-bay {
  height: 0;
  overflow: hidden;
  transition:
    height 340ms cubic-bezier(0.4, 0, 0.2, 1),
    border-top-color 200ms linear;
}

.intent-bay.open {
  height: 100px; /* exact height — no layout shift */
}

.intent-bay.closing {
  transition-duration: 260ms;
}
```

### Dioxus Signal Pattern

```rust
// In parent component (MainChassis or CockpitWorkLayer)
let intent_open = use_signal(|| false);

// Trigger from HudPayload (Rule Layer → Cockpit)
// severity == high → auto open
// user action → manual toggle

// Pass down as prop
rsx! {
    IntentBay {
        open: intent_open(),
        on_close: move |_| intent_open.set(false),
    }
}
```

### Content Reveal Animation

Knobs and labels fade + translate inside the drawer:

```css
.intent-knobs {
  opacity: 0;
  transform: translateY(8px);
  transition:
    opacity 240ms linear 100ms,       /* 100ms delay — drawer must travel first */
    transform 280ms cubic-bezier(0.4, 0, 0.2, 1) 100ms;
}

.intent-bay.open .intent-knobs {
  opacity: 1;
  transform: translateY(0);
}
```

No stagger between knobs. All four appear together.
Stagger would suggest the knobs are independent — they are not.
They are a single control surface.

---

## Saturation Dim

When IntentBay is open, the Siamese panels and spectral analyzer lose color.
The knobs become the only full-color elements in the viewport.

### What dims

- Siamese waveforms (A/B)
- Spectral analyzer
- Left meter bars
- Chain panel curves

### What NEVER dims

- PFR Transport Bar (system truth — immutable)
- PFR Timecode (remains AMBER, full luminosity)
- IntentBay knobs (they are the focus)
- HUD overlay (if visible)
- Hangar JINI status dot

### Implementation

```css
/* Applied to .cockpit-work-layer when IntentBay is open */
.cockpit-work-layer.intent-active {
  filter: saturate(0.15) brightness(0.72);
  transition: filter 300ms steps(3, end); /* stepped — no smooth color fade */
}

/* Exempt elements — applied directly, override parent filter */
#pfr {
  /* PFR is outside .cockpit-work-layer — never affected */
  /* Ensure z-index: 100 is maintained */
}

.intent-bay {
  filter: none; /* knobs are exempt — full color */
}
```

**Stepped transition** — `steps(3, end)`. Not smooth. Three discrete steps.
Matches the avionics philosophy: state changes are discrete, not continuous.

### Dioxus class signal

```rust
// In CockpitWorkLayer
let intent_open = use_context::<Signal<bool>>();

rsx! {
    div {
        class: if intent_open() { "cockpit-work-layer intent-active" }
               else { "cockpit-work-layer" },
        // ... instruments
    }
}
```

---

## Trigger Rules (from Session Phase Diagram v1.0)

```
Phase 3 — Threshold Binding:
  IF severity == high:
    → intent_open.set(true)   // auto-trigger
    → HUD auto-trigger (separate signal)
  IF severity < high:
    → intent_open stays false
    → user can open manually (button or shortcut)

Phase 4 — Playback:
  User may open/close manually at any time.

Phase 5 — Mastering Execution:
  intent_open.set(false) // force close during mastering
  IntentBay does not accept open signal during MASTERING_EXECUTION
```

---

## Overlap with Hangar

When open, the IntentBay bottom edge must overlap the Hangar top by exactly
10–15% of the Hangar height.

```rust
// Hangar height: 52px
// Overlap: 6–8px
// IntentBay must be positioned with:
//   position: relative (in normal flow)
//   margin-bottom: -7px (pulls Hangar up by 7px under the drawer)
// OR:
//   IntentBay bottom sits at Hangar top + 7px
//   implemented via negative bottom margin or absolute positioning
```

The Hangar JINI identity and status remain visible below the overlap.
The narrative text may be partially obscured — acceptable.
The JINI name and status dot must never be covered.

---

## Knob Specification

Four knobs. Identical component. Props differ only in label and range text.

```rust
#[derive(Props, Clone, PartialEq)]
struct IntentKnobProps {
    label: &'static str,       // "TONE"
    range: &'static str,       // "Warm ↔ Bright"
    highlighted: bool,         // true = teal accent (matched to HUD suggestion)
    initial_angle: f32,        // -135.0 to 135.0 degrees
    on_change: EventHandler<f32>,  // emits normalized -1.0..1.0
}
```

### Knob visual

```
Outer ring: 46px diameter
  - border: 2px solid #142435 (normal) / #009898 (highlighted)
  - background: radial-gradient(circle at 38% 32%, #0b131e, #060a0f)
  - box-shadow when highlighted: 0 0 12px rgba(0,152,152,.3)

Indicator line:
  - position: absolute, top: 5px, left: 50%
  - width: 2px, height: 10px
  - background: #2e5570 (normal) / #009898 (highlighted)
  - transform-origin: 50% calc(100% + 13px)
  - rotates with knob angle
```

### Drag interaction

```rust
// Mouse drag — vertical delta → angle
// 1px drag = 1.2 degrees of rotation
// Clamp: -135° to +135°
// Emit normalized value: angle / 135.0 → -1.0..1.0

fn on_mouse_move(angle: &mut f32, start_y: &mut f32, e: MouseEvent) {
    let delta = (*start_y - e.client_y() as f32) * 1.2;
    *angle = (*angle + delta).clamp(-135.0, 135.0);
    *start_y = e.client_y() as f32;
}
```

### Highlight logic

Only one knob is highlighted at a time.
The highlighted knob corresponds to the `guidance_key` in the active HudPayload.

```
guidance_key: "reduce_warmth"  → highlight: Tone
guidance_key: "increase_punch" → highlight: Dynamics
guidance_key: "widen_stereo"   → highlight: Space
guidance_key: "reduce_gain"    → highlight: Loudness
guidance_key: none             → no highlight
```

---

## DoD Gates (Agent must verify before closing task)

```bash
# 1. IntentBay renders without business logic
grep -r "severity\|CoachFindings\|Rule" cockpit/components/intent-bay/ → 0 results

# 2. Saturation dim applies to work layer only, not PFR
# Manual visual check: PFR timecode remains AMBER when IntentBay open

# 3. Animation easing matches spec
# Manual check: no bounce, no overshoot, clean damped stop

# 4. Knob drag emits normalized -1.0..1.0
cargo test intent_knob_normalization

# 5. Z-index: IntentBay above Hangar, below PFR
# Inspect computed styles: .intent-bay z-index == 40

# 6. Hangar JINI name visible during overlap
# Manual check: JINI text not occluded when IntentBay open

# 7. IntentBay force-closes during MASTERING_EXECUTION
# Signal test: set PFR_STATE to MASTERING_EXECUTION → intent_open == false

# 8. No logo rendered
grep -r "shield\|logo\|OS.STUDIO" cockpit/components/intent-bay/ → 0 results
```

---

## File Structure

```
apps/stillair/cockpit/components/
└── intent-bay/
    ├── mod.rs          ← IntentBay component (root)
    ├── knob.rs         ← IntentKnob component
    ├── damper.rs       ← Hydraulic damper SVG (pure render)
    └── styles.css      ← Scoped styles (or CSS-in-Rust)
```

---

**Lead Architect:** Anestis
**System:** Creator OS / Still Air (A1) / Cockpit
**Document:** `apps/stillair/cockpit/tasks/task-6.5-intent-bay.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED — AGENT TASK
