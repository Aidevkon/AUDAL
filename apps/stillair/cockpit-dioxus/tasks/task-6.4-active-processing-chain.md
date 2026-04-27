# Task 6.4 — ActiveProcessingChain Component
# apps/stillair/cockpit/components/active-processing-chain/

**Document:** `apps/stillair/cockpit/tasks/task-6.4-active-processing-chain.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED — AGENT TASK
**Role:** Senior Dioxus Engineer
**Authority:** HUD Constitution v1.0 · Session Phase Diagram v1.0
**Prerequisite:** None — implement before Task 6.5

---

## Objective

Implement the `ActiveProcessingChain` component — the right-side strip of the
`CockpitWorkLayer` that displays the mastering chain state (EQ / Compressor /
Limiter) as read-only visualization panels.

This component is render-only. It receives data. It displays data.
It has zero business logic, zero DSP interaction, zero knobs.

---

## Position in Tree

```
CockpitWorkLayer
├── SiamesePanels          ← existing
└── ActiveProcessingChain  ← THIS TASK (new subtree)
    ├── EQModule
    ├── CompressorModule
    └── LimiterModule
```

`CockpitWorkLayer` layout: CSS grid, two columns.
```css
.cockpit-work-layer {
    display: grid;
    grid-template-columns: 1fr 260px;  /* siamese | chain */
    height: 100%;
    min-height: 0;
}
```

---

## Component Props

```rust
#[derive(Props, Clone, PartialEq)]
pub struct ActiveProcessingChainProps {
    pub eq: EQState,
    pub compressor: CompressorState,
    pub limiter: LimiterState,
}

#[derive(Clone, PartialEq)]
pub struct EQState {
    pub low_db: f32,       // −12.0..+12.0
    pub mid_db: f32,
    pub presence_db: f32,
    pub air_db: f32,
    pub curve_points: Vec<(f32, f32)>,  // normalized 0..1 for SVG path
}

#[derive(Clone, PartialEq)]
pub struct CompressorState {
    pub threshold_db: f32,  // −40..0
    pub ratio: f32,          // 1.0..20.0
    pub gain_reduction_db: f32,
    pub makeup_db: f32,
    pub curve_points: Vec<(f32, f32)>,
}

#[derive(Clone, PartialEq)]
pub struct LimiterState {
    pub ceiling_dbtp: f32,   // −3.0..0.0
    pub release_auto: bool,
    pub isp_factor: u8,      // 4 or 8
    pub curve_points: Vec<(f32, f32)>,
}
```

---

## Visual Specification

### Container

```css
.active-processing-chain {
    display: flex;
    flex-direction: column;
    background: #060a0f;
    border-left: 1px solid #0e1c28;
    min-height: 0;
}

.chain-header {
    padding: 5px 10px 4px;
    border-bottom: 1px solid #0e1c28;
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-shrink: 0;
}

.chain-header-title {
    font-family: 'Barlow Condensed', sans-serif;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: .14em;
    color: #2e5570;
    text-transform: uppercase;
}

.chain-ro-badge {
    font-size: 6px;
    color: #2e5570;
    letter-spacing: .1em;
    border: 1px solid #0e1c28;
    padding: 1px 5px;
}
```

### Module (shared structure for EQ / Comp / Limiter)

Each module = flex column, fills 1/3 of chain height.

```css
.chain-module {
    flex: 1;
    padding: 7px 10px 5px;
    border-bottom: 1px solid #0e1c28;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
}
.chain-module:last-child { border-bottom: none; }

.cm-header {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-shrink: 0;
}
.cm-num {
    font-size: 7px;
    color: #009898;
    letter-spacing: .1em;
    font-family: 'Share Tech Mono', monospace;
}
.cm-name {
    font-family: 'Barlow Condensed', sans-serif;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: .1em;
    color: #b8d4e8;
    text-transform: uppercase;
}
.cm-type {
    font-size: 7px;
    color: #2e5570;
    letter-spacing: .08em;
    margin-left: auto;
}

/* Plot area */
.cm-plot {
    flex: 1;
    min-height: 0;
    position: relative;
    background: #03060a;
    border: 1px solid #0e1c28;
    overflow: hidden;
}
.cm-grid {
    position: absolute;
    inset: 0;
    background:
        repeating-linear-gradient(90deg,
            transparent, transparent 19px,
            rgba(14,28,40,.7) 19px, rgba(14,28,40,.7) 20px),
        repeating-linear-gradient(0deg,
            transparent, transparent 11px,
            rgba(14,28,40,.7) 11px, rgba(14,28,40,.7) 12px);
}
.cm-svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
}

/* Readouts */
.cm-reads {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    flex-shrink: 0;
}
.cr { display: flex; flex-direction: column; gap: 1px; }
.cr-l { font-size: 6px; color: #2e5570; letter-spacing: .08em; font-family: 'Share Tech Mono', monospace; }
.cr-v { font-size: 9px; color: #7aaac4; letter-spacing: .04em; font-family: 'Share Tech Mono', monospace; }
.cr-v.lit { color: #009898; }
```

### EQ Curve SVG

Zero line at y=50% of plot height. Curve drawn from `eq.curve_points`.

```rust
// Generate SVG path from curve_points
// curve_points: Vec<(f32, f32)> normalized 0..1
// Map to plot dimensions at render time

fn eq_path(points: &[(f32, f32)], width: f32, height: f32) -> String {
    if points.is_empty() { return format!("M0,{} L{},{}", height/2.0, width, height/2.0); }
    let first = points[0];
    let mut path = format!("M{},{}", first.0 * width, first.1 * height);
    for p in &points[1..] {
        path.push_str(&format!(" L{},{}", p.0 * width, p.1 * height));
    }
    path
}
```

Curve color: `#7755ee` stroke, `rgba(119,85,238,.07)` fill to baseline.
Zero line: `#0e1c28` 0.5px horizontal at 50%.

### Compressor Curve SVG

Input/output transfer curve. Threshold line at computed x position.
Curve color: `#009898`. Fill: `rgba(0,152,152,.06)`.
Threshold dashed line: `rgba(0,152,152,.2)` 0.5px dasharray `2,2`.

### Limiter Curve SVG

Brickwall ceiling as horizontal dashed line.
Curve approaches ceiling asymptotically.
Ceiling line: `rgba(232,56,32,.35)` 1px dasharray `3,2`.
Curve: `#e83820` stroke. Fill: `rgba(232,56,32,.05)`.
Ceiling label: value in `dBTP` at right edge, `rgba(232,56,32,.45)` 6px.

---

## Dioxus Component Structure

```rust
// apps/stillair/cockpit/components/active-processing-chain/mod.rs

use dioxus::prelude::*;

#[component]
pub fn ActiveProcessingChain(props: ActiveProcessingChainProps) -> Element {
    rsx! {
        div { class: "active-processing-chain",
            div { class: "chain-header",
                span { class: "chain-header-title", "Mastering Chain" }
                span { class: "chain-ro-badge", "READ-ONLY" }
            }
            EQModule { state: props.eq.clone() }
            CompressorModule { state: props.compressor.clone() }
            LimiterModule { state: props.limiter.clone() }
        }
    }
}
```

```rust
// apps/stillair/cockpit/components/active-processing-chain/eq.rs

#[component]
pub fn EQModule(state: EQState) -> Element {
    rsx! {
        div { class: "chain-module",
            div { class: "cm-header",
                span { class: "cm-num", "01" }
                span { class: "cm-name", "EQ" }
                span { class: "cm-type", "4-Band PAR" }
            }
            div { class: "cm-plot",
                div { class: "cm-grid" }
                svg { class: "cm-svg", view_box: "0 0 240 60",
                    // zero line
                    line { x1: "0", y1: "30", x2: "240", y2: "30",
                           stroke: "#0e1c28", stroke_width: "0.5" }
                    // curve fill
                    path {
                        d: "{eq_fill_path(&state.curve_points)}",
                        fill: "rgba(119,85,238,.07)"
                    }
                    // curve stroke
                    path {
                        d: "{eq_stroke_path(&state.curve_points)}",
                        fill: "none",
                        stroke: "#7755ee",
                        stroke_width: "1.5"
                    }
                }
            }
            div { class: "cm-reads",
                ReadoutItem { label: "LOW", value: format!("{:+.1}", state.low_db), lit: state.low_db.abs() > 0.5 }
                ReadoutItem { label: "MID", value: format!("{:+.1}", state.mid_db), lit: false }
                ReadoutItem { label: "PRES", value: format!("{:+.1}", state.presence_db), lit: false }
                ReadoutItem { label: "AIR", value: format!("{:+.1}", state.air_db), lit: state.air_db.abs() > 0.5 }
            }
        }
    }
}
```

Same pattern for `CompressorModule` and `LimiterModule`.

```rust
// Shared readout item
#[component]
fn ReadoutItem(label: &'static str, value: String, lit: bool) -> Element {
    rsx! {
        div { class: "cr",
            span { class: "cr-l", "{label}" }
            span { class: if lit { "cr-v lit" } else { "cr-v" }, "{value}" }
        }
    }
}
```

---

## File Structure

```
apps/stillair/cockpit/components/
└── active-processing-chain/
    ├── mod.rs          ← ActiveProcessingChain root component
    ├── eq.rs           ← EQModule
    ├── compressor.rs   ← CompressorModule
    ├── limiter.rs      ← LimiterModule
    ├── readout.rs      ← ReadoutItem (shared)
    └── types.rs        ← EQState, CompressorState, LimiterState
```

---

## DoD Gates

```bash
# 1. Component renders with mock data
# Supply hardcoded EQState/CompressorState/LimiterState — must render without panic

# 2. Read-only — zero event handlers on plot areas
grep -r "onclick\|onmousedown\|on_click\|on_mouse" \
  cockpit/components/active-processing-chain/ → 0 results in plot elements

# 3. No business logic
grep -r "CoachFindings\|Rule\|severity\|MasteringManifest" \
  cockpit/components/active-processing-chain/ → 0 results

# 4. Grid column integrates with CockpitWorkLayer
# Visual check: chain strip is exactly 260px wide, fills full height

# 5. Three modules divide height equally (flex:1 each)
# Visual check: EQ / Comp / Limiter are equal height sections

# 6. Curve paths render without panic on empty curve_points
cargo test eq_empty_curve compressor_empty_curve limiter_empty_curve
```

---

**Lead Architect:** Anestis
**System:** Creator OS / Still Air (A1) / Cockpit
**Document:** `apps/stillair/cockpit/tasks/task-6.4-active-processing-chain.md`
**Version:** 1.0
**Date:** 2026-04-27
**Status:** 🔒 LOCKED — AGENT TASK
