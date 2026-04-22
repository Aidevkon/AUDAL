# Still Air Cockpit — UI Agent Context Document

**Document:** `apps/stillair/cockpit/ui-agent-context.md`
**Version:** 2.1
**Status:** 🔒 LOCKED
**Authority:** Creator OS Constitution v2.6 · Amendment A-002 · sp314-dsp v2.7 spec
**Owner:** Lead Architect (Anestis)
**Date:** 2026-04-22
**Changes from v2.0:** VisualizationDataJson IPC type, SVG paths precomputed in backend,
low-power CSS flag, polling cancellation pattern.

---

## ⚠️ READ THIS FIRST — Non-Negotiable Rules

You are implementing the **UI layer only**. You are a rendering surface.

```
❌ No business logic in components
❌ No state computation — display only
❌ No SVG path computation in UI — paths come precomputed from backend
❌ No waveform downsampling in UI — backend delivers SVG paths
❌ No direct LLM calls
❌ No imports from sp314-dsp, telemetry, rule-engine, M0, or Aether
❌ No new Tauri commands — reuse existing only
❌ No stores / reducers / state machines — Dioxus signals only
❌ No serde_json::Value in component props — typed structs only
❌ No inline hex colors — CSS variables from design tokens only
```

If a task requires touching any core crate → **STOP and report**.

---

## §1 — Architecture Rules

### What the Cockpit is

The Cockpit is a **disposable rendering surface**. Its only jobs are:

1. Call Tauri IPC commands
2. Bind the response to Dioxus signals
3. Render those signals

It has no opinion about audio. It does not compute anything.
It does not know what LUFS means. It displays a number it received.

### What the Cockpit is NOT

- Not a state machine owner
- Not a data transformer
- Not a curve generator
- Not a waveform processor
- Not a business logic layer

### Layer boundary

```
Cockpit (Dioxus 0.6 web/WASM)
    │
    │  invoke("getSessionState")
    │  invoke("triggerMastering")
    │  invoke("exportAudio")
    │  invoke("playbackControl")
    │  invoke("getPlaybackState")
    │  invoke("getVisualizationData")   ← NEW — precomputed SVG paths
    ▼
Tauri commands (Rust backend)
    │
    ▼
SessionStateJson         ← loudness, quality, compliance, findings
CoachNarrativeJson       ← narrative text, findings
PlaybackStateJson        ← position_ms, duration_ms, is_playing
VisualizationDataJson    ← precomputed SVG paths — UI renders only
```

Every piece of data displayed must originate from one of these commands.
**No data is computed, derived, or assumed in the UI.**

---

## §2 — Data Sources

### SessionStateJson (existing — session.rs)

```rust
pub struct SessionStateJson {
    pub blob_id:    String,
    pub loudness:   LoudnessMetricsJson,
    pub quality:    QualityMetricsJson,
    pub compliance: ComplianceJson,
    pub findings:   CoachFindingsJson,
    pub narrative:  Option<CoachNarrativeJson>,
}

pub struct LoudnessMetricsJson {
    pub integrated_lufs:          f32,   // VU meter + MetricsReadout
    pub true_peak_dbtp:            f32,   // VU meter + MetricsReadout
    pub lra:                       f32,   // MetricsReadout RANGE
    pub short_term_lufs:           f32,
    pub momentary_lufs:            f32,
    pub spotify_compliant:         bool,
    pub youtube_compliant:         bool,
    pub apple_music_compliant:     bool,
    pub tidal_compliant:           bool,
    pub broadcast_compliant:       bool,
}

pub struct QualityMetricsJson {
    pub stereo_correlation: f32,   // MetricsReadout CORR
    pub stereo_width:       f32,   // passed to getVisualizationData
    pub dynamic_range_db:   f32,
    pub rms_db:             f32,
    pub clip_free:          bool,
    pub spectral_centroid:  f32,   // passed to getVisualizationData
    pub spectral_flux:      f32,   // passed to getVisualizationData
    pub spectral_flatness:  f32,   // passed to getVisualizationData
    pub crest_factor:       f32,
}
```

### VisualizationDataJson (NEW — precomputed in backend)

**Rule:** The backend computes all SVG paths and curve data.
The UI receives strings and renders them. Zero computation in UI.

```rust
// New Tauri command: getVisualizationData(blob_id: String)
// Computed in src-tauri/src/commands/visualization.rs
// Uses quality metrics from GoldenBlob to generate SVG path strings

pub struct VisualizationDataJson {
    /// Spectrum waveform — SVG path string, 400×200 viewBox
    /// Filled area curve from spectral_centroid + flux + flatness
    pub spectrum_svg_path:    String,

    /// Lissajous outer ellipse — SVG ellipse attributes
    /// Derived from stereo_width
    pub lissajous_outer_rx:   f32,
    pub lissajous_outer_ry:   f32,

    /// Lissajous inner ellipse — SVG ellipse attributes
    /// Derived from stereo_correlation
    pub lissajous_inner_rx:   f32,
    pub lissajous_inner_ry:   f32,

    /// Waveform snapshots for Mastered View (Phase 14 = placeholder)
    /// Phase 15: real before/after from PCM cache
    pub waveform_before_svg:  String,   // SVG path, 800×120 viewBox
    pub waveform_after_svg:   String,   // SVG path, 800×120 viewBox
}
```

**UI usage:**
```rust
// UI receives and renders — computes NOTHING
fn SpectrumDisplay(path: String) -> Element {
    rsx! {
        svg { view_box: "0 0 400 200",
            path { d: "{path}", fill: "url(#sg)", stroke: "var(--accent-cyan)" }
        }
    }
}
```

### PlaybackStateJson (existing — playback.rs)

```rust
pub struct PlaybackStateJson {
    pub blob_id:     String,
    pub position_ms: u64,    // TransportBar position
    pub duration_ms: u64,    // TransportBar duration
    pub is_playing:  bool,   // play/pause button state
    pub sample_rate: u32,
    pub channels:    u16,
}
```

---

## §3 — State Model

```rust
#[derive(Clone)]
pub struct CockpitState {
    pub session:        Signal<Option<SessionStateJson>>,
    pub viz:            Signal<Option<VisualizationDataJson>>,
    pub playback:       Signal<Option<PlaybackStateJson>>,
    pub is_mastering:   Signal<bool>,
    pub export_format:  Signal<String>,
}
```

### IPC fetch pattern

```rust
// Fetch session + viz after mastering completes
spawn_local(async move {
    let session = invoke::<SessionStateJson, _>("getSessionState",
        json!({ "blobId": blob_id })).await?;
    let viz = invoke::<VisualizationDataJson, _>("getVisualizationData",
        json!({ "blobId": blob_id })).await?;
    state.session.set(Some(session));
    state.viz.set(Some(viz));
});
```

### Playback polling (500ms, FM5 only)

```rust
// Poll only when in CoachReady state
// Cancel when mode transitions away from CoachReady
use_effect(move || {
    if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) {
        return;
    }
    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(500).await;
            // Stop if no longer in FM5
            if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) {
                break;
            }
            if let Ok(Some(ps)) = invoke::<Option<PlaybackStateJson>, _>(
                "getPlaybackState", ()
            ).await {
                state.playback.set(Some(ps));
            }
        }
    });
});
```

---

## §4 — Visual Specification

### 4.1 — Design Language: Industrial Hardware Emulation

This is NOT a web dashboard. It is a **physical rack-mounted device** rendered in software.

Every panel is a **rack unit** containing an **OLED screen**:
- The chassis is brushed dark aluminum (#141414)
- The OLED screen sits 2–3mm recessed behind a physical bezel
- The bezel has 4 corner screws (decorative)
- The OLED emits a faint colored glow matching its accent color
- All typography is monospace — this is an instrument
- Inactive VU segments are visible but dark — like physical LEDs at rest

**Reference:** SSL / Neve / API console aesthetic. Avionics + audio hardware.
Not material design. Not flat design. Physical depth.

---

### 4.2 — Color Palette

```css
/* BACKGROUNDS */
--bg-app:              #0d0d0d;
--bg-panel:            #141414;
--bg-panel-inset:      #0a0a0a;
--bg-track-active:     #1a3a38;
--bg-finding-row:      #181818;
--bg-meter:            #080808;

/* PANEL STRUCTURE */
--border-panel:        #2a2a2a;
--border-inset:        #1e1e1e;
--screw-face:          #252525;
--screw-slot:          #111111;
--bracket-color:       #2a2a2a;

/* CYAN — LUFS, active, insights */
--accent-cyan:         #00d4c8;
--accent-cyan-dim:     #007a74;
--accent-cyan-glow:    rgba(0, 212, 200, 0.25);
--accent-cyan-glow-sm: rgba(0, 212, 200, 0.12);

/* AMBER — PEAK, warning */
--accent-amber:        #c8a832;
--accent-amber-dim:    #7a6420;

/* GOLD — RANGE */
--accent-gold:         #d4a844;

/* MAGENTA — Lissajous inner trace */
--accent-magenta:      #e040a0;

/* SEVERITY */
--severity-high:       #ff3b30;
--severity-medium:     #ff9500;
--severity-low:        #ffcc00;
--severity-info:       #00d4c8;

/* TYPOGRAPHY */
--text-primary:        #e8e8e8;
--text-secondary:      #7a7a7a;
--text-label:          #4a4a4a;
--text-accent:         #00d4c8;

/* FONT */
--font-mono: "JetBrains Mono", "Courier New", monospace;
```

---

### 4.3 — Low-Power Mode

For low-end hardware: single CSS class disables all heavy effects.
This is CSS-only — no UI logic, no signals, no Tauri command.

```css
/* Applied by setting data-low-power="true" on root element */
[data-low-power="true"] * {
    box-shadow: none !important;
    filter: none !important;
    text-shadow: none !important;
    backdrop-filter: none !important;
}

[data-low-power="true"] .oled-screen {
    box-shadow: inset 0 1px 3px rgba(0,0,0,0.6) !important;
}
```

Detection: check `window.matchMedia('(prefers-reduced-motion: reduce)')` or
expose a CSS class toggle in settings. No business logic in component.

---

### 4.4 — Panel Shadow System

```css
.panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-panel);
    border-radius: 6px;
    box-shadow:
        0 4px 24px rgba(0,0,0,0.8),
        0 2px 8px rgba(0,0,0,0.6),
        inset 0 1px 0 rgba(255,255,255,0.04),
        inset 0 -1px 0 rgba(0,0,0,0.4);
    position: relative;
    isolation: isolate;   /* CRITICAL — prevents stacking bleed */
}

.panel-insights {
    border-color: var(--accent-cyan-dim);
    box-shadow:
        0 4px 24px rgba(0,0,0,0.8),
        0 2px 8px rgba(0,0,0,0.6),
        0 0 32px var(--accent-cyan-glow),
        0 0 64px var(--accent-cyan-glow-sm),
        inset 0 1px 0 rgba(255,255,255,0.04),
        inset 0 -1px 0 rgba(0,0,0,0.4);
}

.oled-screen {
    background: var(--bg-panel-inset);
    border: 1px solid var(--border-inset);
    border-radius: 3px;
    box-shadow:
        inset 0 2px 8px rgba(0,0,0,0.8),
        inset 0 1px 3px rgba(0,0,0,0.6);
}

/* mfd-bay MUST have isolation to prevent panel overlap into transport */
.mfd-bay {
    isolation: isolate;
}
```

---

### 4.5 — Corner Screws

```css
.screw {
    position: absolute;
    width: 10px; height: 10px;
    border-radius: 50%;
    background: radial-gradient(circle at 35% 35%,
        #3a3a3a 0%, #1a1a1a 60%, #0a0a0a 100%);
    border: 1px solid #111;
    box-shadow: inset 0 1px 0 rgba(255,255,255,0.1), 0 1px 3px rgba(0,0,0,0.8);
    pointer-events: none;  /* MUST NOT block clicks */
}
.screw-tl { top:8px; left:8px; }
.screw-tr { top:8px; right:8px; }
.screw-bl { bottom:8px; left:8px; }
.screw-br { bottom:8px; right:8px; }

.screw::after {
    content: "";
    position: absolute;
    top:50%; left:15%; width:70%; height:1.5px;
    background: var(--screw-slot);
    transform: translateY(-50%) rotate(45deg);
    pointer-events: none;
}
```

---

### 4.6 — VU Meters (Segmented)

**Physical intent:** Vintage broadcast VU meter. 30 LED segments.
Active = full color. Inactive = 8% opacity. Top 3 = red clipping zone.

Reference image: LUFS cyan left + PEAK amber right, scale labels on sides.

```css
.vu-meter {
    display: flex;
    flex-direction: column-reverse;
    gap: 2px;
    width: 28px;
}
.vu-segment { height: 4px; border-radius: 1px; }
```

```rust
fn VuMeter(value: f32, min: f32, max: f32, color: &'static str) -> Element {
    let total = 30usize;
    let filled = ((value - min) / (max - min) * total as f32)
        .clamp(0.0, total as f32) as usize;
    rsx! {
        div { class: "vu-meter",
            for i in 0..total {
                div {
                    class: "vu-segment",
                    style: if i < filled {
                        if i >= total - 3 { "background:#ff3b30;".to_string() }
                        else { format!("background:{};", color) }
                    } else {
                        format!("background:{}14;", color)
                    }
                }
            }
        }
    }
}
```

Scale labels: separate column of `<span>` elements at hardcoded positions.
Labels: 0, -3, -6, -12, -18, -20, -25, -30 (LUFS) / 0, -3, -6, -12, -16, -20 (PEAK).

---

### 4.7 — SpectrumDisplay

**Physical intent:** Frequency analyzer on OLED. Waveform curve, NOT bars.
**Data source:** `viz.spectrum_svg_path` — precomputed in backend.
**UI:** receives String, renders into SVG. Zero computation.

```rust
fn SpectrumDisplay(path: String) -> Element {
    rsx! {
        div { class: "oled-screen spectrum-display",
            svg { class: "spectrum-svg", view_box: "0 0 400 200",
                defs {
                    linearGradient { id: "sg", x1:"0", y1:"0", x2:"0", y2:"1",
                        stop { offset:"0%", stop_color:"var(--accent-cyan)" }
                        stop { offset:"100%", stop_color:"transparent" }
                    }
                }
                SpectrumGrid {}
                // Fill area — precomputed closed path from backend
                path { d: "{path}", fill: "url(#sg)", opacity: "0.4" }
                // Stroke — same path
                path { d: "{path}", fill: "none",
                    stroke: "var(--accent-cyan)", stroke_width: "1.5" }
            }
        }
    }
}
```

Grid: subtle cyan lines at frequency markers (20Hz, 100, 500, 1k, 5k, 10k, 20kHz).
Rendered as SVG `<line>` elements, opacity 0.08.

---

### 4.8 — StereoScope (Lissajous)

**Physical intent:** Stereo correlation oscilloscope. Goniometer display.
**Data source:** `viz.lissajous_*` rx/ry values — precomputed in backend.
**UI:** receives 4 f32 values, renders 2 SVG ellipses. Zero computation.

```
Outer ellipse: cyan, stereo width orbit
Inner ellipse: magenta, correlation tightness
Both rotated 45° (standard goniometer)
Center: white/cyan glow hotspot
Background: polar grid (3 circles + crosshair)
```

```rust
fn StereoScope(
    outer_rx: f32, outer_ry: f32,
    inner_rx: f32, inner_ry: f32,
) -> Element {
    rsx! {
        div { class: "oled-screen stereo-scope",
            svg { view_box: "0 0 120 120",
                StereoGrid {}
                // Outer — cyan (stereo width)
                ellipse { cx:"60", cy:"60",
                    rx:"{outer_rx}", ry:"{outer_ry}",
                    fill:"none", stroke:"var(--accent-cyan)",
                    stroke_width:"1.5", opacity:"0.8",
                    transform:"rotate(-45 60 60)" }
                // Inner — magenta (correlation)
                ellipse { cx:"60", cy:"60",
                    rx:"{inner_rx}", ry:"{inner_ry}",
                    fill:"none", stroke:"var(--accent-magenta)",
                    stroke_width:"1.0", opacity:"0.7",
                    transform:"rotate(-45 60 60)" }
                // Center glow
                circle { cx:"60", cy:"60", r:"3",
                    fill:"var(--accent-cyan)", opacity:"0.9",
                    filter:"url(#cg)" }
                defs {
                    filter { id:"cg",
                        feGaussianBlur { std_deviation:"2" } }
                }
            }
        }
    }
}
```

---

### 4.9 — MetricsReadout

```
PEAK   -1.5   ← amber,   28px
RANGE   12    ← gold,    28px
CORR  +0.65   ← cyan,    28px  (prefix "+" when positive)
```

Data from `session.loudness.true_peak_dbtp`, `lra`, `quality.stereo_correlation`.

---

### 4.10 — CoachPanel

```
FindingRow:
  [Label bold 12px]              [severity dot right]
  [Description 10px secondary]
  [Progress bar cyan fill 4px]   [XX% right]

[YES] filled cyan, glow shadow
[NO]  outline, no glow
```

```css
.finding-bar-track { height:4px; background:var(--bg-panel-inset); border-radius:2px; }
.finding-bar-fill  { height:4px; background:var(--accent-cyan);    border-radius:2px; }

.btn-yes {
    background: var(--accent-cyan); color: var(--bg-app);
    font-family: var(--font-mono); font-size:11px; font-weight:600;
    letter-spacing:0.15em; padding:10px 24px; border:none; border-radius:3px;
    box-shadow: 0 0 12px var(--accent-cyan-glow);
    pointer-events: all; position: relative; z-index: 1;
}
.btn-no {
    background: transparent; color: var(--text-primary);
    border: 1px solid #3a3a3a;
    pointer-events: all; position: relative; z-index: 1;
}
```

---

### 4.11 — TransportBar

```
[00:00] | [◄◄ 5s] [▶ PLAY] [5s ►►] | [●SCRUB●] | [■ STOP] | / 04:32
```

```css
.transport-bar {
    background: #111111;
    border-top: 1px solid var(--border-inset);
    height: 56px;
    display: flex; align-items: center;
    padding: 0 1.5rem; gap: 1rem;
    position: relative; z-index: 100;
    pointer-events: all;
}
/* ALL decorative pseudo-elements MUST have pointer-events: none */
.transport-bar::before,
.transport-bar::after { pointer-events: none; }

.transport-time {
    font-family: var(--font-mono);
    color: var(--accent-cyan);
    font-size: 1.25rem;
    text-shadow: 0 0 8px var(--accent-cyan-glow);
}
.transport-play {
    background: var(--accent-amber); color: var(--bg-app);
    font-family: var(--font-mono); font-size: 0.8rem; font-weight: 600;
    border: none; padding: 0.5rem 1.5rem; cursor: pointer;
    position: relative; z-index: 1; pointer-events: all;
}
.transport-skip {
    background: transparent; color: var(--text-secondary);
    border: 1px solid var(--border-inset);
    font-family: var(--font-mono); font-size: 0.75rem;
    padding: 0.4rem 0.8rem; cursor: pointer;
    position: relative; z-index: 1; pointer-events: all;
}
```

---

### 4.12 — Corner Brackets (Session Panel)

```css
.bracket-bl::before {  /* vertical arm */
    content:""; position:absolute; bottom:16px; left:16px;
    width:2px; height:20px; background:var(--bracket-color);
    pointer-events:none;
}
.bracket-bl::after {   /* horizontal arm */
    content:""; position:absolute; bottom:16px; left:16px;
    width:20px; height:2px; background:var(--bracket-color);
    pointer-events:none;
}
/* Mirror for bottom-right */
```

---

### 4.13 — Layout Grid

```
Window: fill viewport (min 1280px)
Columns: 300px | 1fr | 300px, gap: 12px
Transport: 56px pinned bottom
Panel padding: 16px
Panel header: 40px, centered, border-bottom
```

---

## §5 — Mastered View

### 5.1 — Intent

Post-session analysis room. Read-only. Engineer reviews BEFORE/AFTER.

### 5.2 — Data Sources

```rust
// Waveform paths from VisualizationDataJson
viz.waveform_before_svg  // SVG path string — Phase 14: placeholder curve
viz.waveform_after_svg   // SVG path string — Phase 15: real PCM snapshot

// Metrics from SessionStateJson — same data, different display context
session.loudness.integrated_lufs
session.loudness.true_peak_dbtp
session.loudness.short_term_lufs
session.loudness.lra
session.compliance.*
```

**Phase 14:** Waveform paths are placeholder curves generated from loudness metrics.
**Phase 15:** Real before/after waveforms from PCM cache (A-003 §11).

### 5.3 — View Modes

```rust
pub enum ViewMode { Overlay, Split, Dual }
// Signal<ViewMode> in CockpitState — UI only, no IPC
```

### 5.4 — Waveform Display

```rust
// UI receives SVG path string — renders only
fn WaveformDisplay(path: String, label: &str) -> Element {
    rsx! {
        div { class: "waveform-container",
            div { class: "waveform-label", "{label}" }
            svg { class: "waveform-svg", view_box: "0 0 800 120",
                WaveformGrid {}
                path { d: "{path}", fill: "none",
                    stroke: "var(--accent-cyan)", stroke_width: "1.5" }
            }
        }
    }
}
```

### 5.5 — Quality Gate + Compliance

```rust
fn QualityGatePanel(session: &SessionStateJson) -> Element { ... }

fn ComplianceDot(compliant: bool) -> Element {
    rsx! {
        div { class: if compliant { "compliance-dot active" }
                     else { "compliance-dot inactive" } }
    }
}
```

---

## §6 — Component Tree

```
CockpitLayout
├── SessionPanel
│   ├── PanelHeader ("THE SESSION")
│   ├── Screw × 4
│   ├── TrackInfo
│   ├── TrackList
│   ├── CornerBrackets
│   └── ExportControls (FM5 only)
├── InsightsPanel
│   ├── PanelHeader ("THE INSIGHTS")
│   ├── Screw × 4
│   └── OledScreen (2×2 grid)
│       ├── SpectrumDisplay      ← receives viz.spectrum_svg_path
│       ├── VuMeterPair          ← receives loudness.integrated_lufs + true_peak
│       ├── StereoScope          ← receives viz.lissajous_* rx/ry
│       └── MetricsReadout       ← receives peak/lra/correlation
├── CoachPanel
│   ├── PanelHeader ("SOCRATIC COACH")
│   ├── Screw × 4
│   ├── NarrativeSummary
│   ├── FindingsList → FindingRow × N
│   └── ActionButtons ([YES] [NO])
├── TransportBar
│   ├── PositionDisplay
│   ├── SkipBack [◄◄ 5s]
│   ├── PlayPause [▶/▐▐]
│   ├── SkipForward [5s ►►]
│   ├── ScrubRail
│   ├── StopButton [■]
│   └── DurationDisplay
└── MasteredView (overlay/route)
    ├── Sidebar
    ├── ViewModeSelector
    ├── WaveformBefore
    ├── WaveformAfter
    ├── QualityGatePanel
    ├── CompliancePanel
    └── ExportBar
```

---

## §7 — Per-Component Agent Prompts

Give ONE prompt at a time.

### P1 — Panel Container + Screws
```
Implement Panel component + 4 corner screws.
Spec: §4.4 shadow system, §4.5 screws.
Props: title: &str, children, variant: PanelVariant (Default | Insights).
Insights variant: cyan glow shadow.
isolation: isolate on panel. pointer-events: none on all screws + ::after.
No data fetching. Props only.
```

### P2 — CockpitState + IPC
```
Implement CockpitState per §3.
Signals: session, viz, playback, is_mastering, export_format.
use_context_provider at root.
Fetch session + viz together after mastering.
Playback polling 500ms, FM5 only, cancels on mode change.
Types must match existing session.rs and new visualization.rs exactly.
No UI rendering in this step.
```

### P3 — getVisualizationData Tauri Command
```
Add getVisualizationData command to src-tauri/src/commands/visualization.rs.
Input: blob_id: String
Output: VisualizationDataJson (§2)

Compute in Rust:
  spectrum_svg_path: gaussian curve from spectral_centroid + flatness
    → 64 points → SVG path string "M x,y L x,y ..."
  lissajous_outer_rx/ry: from stereo_width
  lissajous_inner_rx/ry: from stereo_correlation
  waveform_before_svg: placeholder sine-like curve from rms_db
  waveform_after_svg:  placeholder sine-like curve from integrated_lufs

All math uses libm. No std::f32 methods.
Register in lib.rs. cargo check -p stillair.
```

### P4 — VU Meters
```
Implement VuMeter + VuMeterPair. Spec: §4.6.
30 segments, 4px height, 2px gap.
LUFS = var(--accent-cyan). PEAK = var(--accent-amber).
Top 3 segments: #ff3b30 clipping zone.
Inactive: 8% opacity (color + "14" hex suffix).
Scale labels as separate column, hardcoded positions.
Data from CockpitState.session signal.
```

### P5 — SpectrumDisplay
```
Implement SpectrumDisplay. Spec: §4.7.
Receives: path: String (precomputed — do NOT compute in UI).
Renders SVG with gradient fill + cyan stroke.
Grid: subtle lines at 20Hz/100/500/1k/5k/10k/20kHz, opacity 0.08.
OLED screen inset (§4.4 oled-screen).
```

### P6 — StereoScope
```
Implement StereoScope. Spec: §4.8.
Receives: outer_rx, outer_ry, inner_rx, inner_ry (f32 from viz signal).
Outer ellipse: cyan, rotate(-45).
Inner ellipse: magenta, rotate(-45).
Center glow: feGaussianBlur filter.
Polar grid background (3 circles + crosshair, opacity 0.15).
Static. No animation. No computation.
```

### P7 — MetricsReadout
```
Implement MetricsReadout. Spec: §4.9.
PEAK amber 28px. RANGE gold 28px. CORR cyan 28px.
CORR: prefix "+" when >= 0.0.
Data from CockpitState.session signal.
```

### P8 — CoachPanel
```
Implement CoachPanel. Spec: §4.10.
FindingRow: label + description + progress bar (width=score*100%) + severity dot.
Severity dot color from severity string: high=#ff3b30 medium=#ff9500 low=#ffcc00 info=#00d4c8.
[YES] filled cyan glow. [NO] outline.
invoke("coachAction", { action: "yes"|"no" }) on click.
Data from CockpitState.session.findings + narrative.
```

### P9 — TransportBar
```
Implement TransportBar. Spec: §4.11.
Buttons: [◄◄ 5s] [▶/▐▐ PLAY/PAUSE] [5s ►►] [■ STOP].
PLAY/PAUSE toggles based on playback.is_playing signal.
Scrub rail: click → seek percentage of duration_ms.
Time displays: MM:SS phosphor cyan with text-shadow glow.
CRITICAL: transport-bar::before/::after → pointer-events:none.
mfd-bay → isolation:isolate.
All buttons: position:relative; z-index:1; pointer-events:all.
```

### P10 — InsightsPanel Assembly
```
Assemble InsightsPanel from P4+P5+P6+P7.
2×2 OLED grid inside oled-screen inset:
  top-left:     SpectrumDisplay (wider, spans if needed)
  top-right:    VuMeterPair
  bottom-left:  StereoScope
  bottom-right: MetricsReadout
Panel variant: Insights (cyan glow).
```

### P11 — MasteredView
```
Implement MasteredView. Spec: §5.
Sidebar: static nav labels, no routing logic.
ViewMode selector: OVERLAY/SPLIT/DUAL → Signal<ViewMode>, no IPC.
WaveformDisplay: receives SVG path string from viz signal — renders only.
QualityGatePanel: large monospace readout from session.loudness.
CompliancePanel: LED dots — cyan active, dim inactive.
ExportBar: WAV/MP3/AIFF/FLAC/PDF buttons invoking exportAudio.
```

### P12 — CockpitLayout (Final Assembly)
```
Assemble all components into CockpitLayout.
Grid: 300px | 1fr | 300px, 12px gap, fill height minus 56px transport.
TransportBar pinned bottom.
Background: var(--bg-app).
DO NOT reimplement any component.
After assembly: run ui-isolation-check.sh — must pass.
```

---

## §8 — Troubleshooting

1. Screenshot both — target + current
2. Identify ONE element
3. Give hex values directly
4. Check shadow stack — missing layers = flat UI (#1 cause)
5. Check pointer-events — invisible elements blocking clicks (#2 cause)
6. Check isolation:isolate on mfd-bay (#3 cause)

Agent adds business logic → reject, redirect to §1.
Agent creates Tauri command (except P3) → reject, redirect to §1.
Agent imports core crate → reject, STOP.

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air Cockpit (A1)
**Version:** 2.1
**Date:** 2026-04-22
**Status:** 🔒 LOCKED

---

*The Cockpit renders. The core computes. The boundary is law.*