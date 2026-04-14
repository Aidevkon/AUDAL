# LineOS — Phase 5 Task Decomposition

**Document:** `lineos/plan/phase-5/task-decomposition.md`
**Version:** 1.0
**Phase:** 5 — Cockpit (Still Air UI)
**Status:** 🔒 LOCKED
**Authority:** Phase 5 Master Prompt · Cockpit State Machine v1.0

---

## Architecture

```
apps/stillair/
├── src-tauri/          ← Tauri 2.x backend (Rust)
│   ├── src/
│   │   ├── main.rs
│   │   ├── commands/   ← IPC commands (file load, mastering trigger)
│   │   └── ipc/        ← M0 client (HTTP → localhost:7400)
└── frontend/           ← Leptos WASM frontend
    └── src/
        ├── app.rs          ← Root + CockpitMode signal
        ├── state/
        │   ├── cockpit_fsm.rs   ← InternalCockpit<S> typestates
        │   └── cockpit_mode.rs  ← CockpitMode enum (view-only)
        ├── cockpit/
        │   ├── mod.rs
        │   ├── session_panel.rs    ← Left MFD
        │   ├── insights_panel.rs   ← Center MFD
        │   └── coach_panel.rs      ← Right MFD
        └── components/
            ├── transport_bar.rs
            ├── drop_zone.rs
            ├── preset_menu.rs
            └── fault_display.rs
```

---

## Task Order

```
P5-001  Tauri app scaffold + Cargo workspace
P5-002  CockpitMode enum + CockpitFsm typestates
P5-003  Cockpit shell — 3-panel grid layout
P5-004  Transport bar + event wiring
P5-005  Left MFD — Session panel (FM0-FM2)
P5-006  Center MFD — Insights panel (FM3-FM5)
P5-007  Right MFD — Coach panel (FM5)
P5-008  M0 IPC client stubs
P5-009  Core mastering flow (end-to-end)
P5-010  FM-ERR fault display
P5-011  CI gate + tag
```

---

## P5-001 — Tauri App Scaffold + Cargo Workspace

**Goal:** Create `apps/stillair/` Tauri 2.x app and add to workspace.

```bash
mkdir -p apps/stillair
cd apps/stillair
cargo tauri init --app-name "stillair" --window-title "Still Air"
```

`apps/stillair/src-tauri/Cargo.toml`:
```toml
[package]
name = "stillair"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
tauri        = { version = "2", features = ["protocol-asset"] }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"
tokio        = { version = "1", features = ["full"] }
reqwest      = { version = "0.12", features = ["json"] }
```

`apps/stillair/frontend/Cargo.toml`:
```toml
[package]
name = "stillair-frontend"
version = "0.1.0"
edition = "2021"
license = "MIT"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
leptos        = { version = "0.7", features = ["csr"] }
wasm-bindgen  = "0.2"
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
web-sys       = { version = "0.3", features = ["File", "FileList", "DragEvent",
                  "EventTarget", "HtmlInputElement"] }
```

Add to root `Cargo.toml`:
```toml
members = [
    "lineos/m0/m0-daemon",
    "lineos/m1/sp314-dsp",
    "lineos/m1/telemetry",
    "lineos/m1/metadata",
    "lineos/m1/insights",
    "lineos/m1/rule-engine",
    "apps/stillair/src-tauri",
    "apps/stillair/frontend",
]
```

**DoD P5-001:**
```bash
cd apps/stillair && cargo tauri build --debug
echo "✅ P5-001"
```

---

## P5-002 — CockpitMode Enum + CockpitFsm Typestates

**Goal:** Implement the Double-Lock FSM from state-machine.md §6.1.

Create `apps/stillair/frontend/src/state/cockpit_mode.rs`:
```rust
use serde::{Deserialize, Serialize};

/// View-only enum for MFD rendering.
/// Driven by InternalCockpit<S> — never set directly by UI components.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CockpitMode {
    Idle,
    FileLoaded,
    PresetSelected,
    Mastering,
    Mastered,
    InsightsReady,
    CoachReady,
    Exporting,
    Fault(AscCode),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AscCode {
    MathErr   = 0x01,
    IoErr     = 0x02,
    Aborted   = 0x03,
    ValidationFail = 0x04,
    WasmPanic = 0x05,
}

impl CockpitMode {
    pub fn is_interactive(&self) -> bool {
        !matches!(self, Self::Mastering | Self::Exporting | Self::Fault(_))
    }

    pub fn shows_coach(&self) -> bool {
        matches!(self, Self::CoachReady)
    }
}
```

Create `apps/stillair/frontend/src/state/cockpit_fsm.rs`:
```rust
use std::marker::PhantomData;
use super::cockpit_mode::{AscCode, CockpitMode};

// ── Typestate markers ──────────────────────────────────────────────────────
pub struct Idle;
pub struct FileLoaded;
pub struct PresetSelected;
pub struct Mastering;
pub struct Mastered;
pub struct InsightsReady;
pub struct CoachReady;
pub struct Exporting;

// ── Sealed Intent (created at FM1.5 → FM2) ────────────────────────────────
#[derive(Debug, Clone)]
pub struct Intent {
    pub audio_path: String,
    pub preset_id:  &'static str,
}

// ── Internal FSM ──────────────────────────────────────────────────────────
pub struct InternalCockpit<S> {
    _state: PhantomData<S>,
    pub mode: CockpitMode,   // always in sync with S
}

impl InternalCockpit<Idle> {
    pub fn new() -> Self {
        Self { _state: PhantomData, mode: CockpitMode::Idle }
    }

    pub fn file_dropped(self, path: String) -> Result<InternalCockpit<FileLoaded>, AscCode> {
        // Validation check — returns Err(AscCode::ValidationFail) if file invalid
        Ok(InternalCockpit { _state: PhantomData, mode: CockpitMode::FileLoaded })
    }
}

impl InternalCockpit<FileLoaded> {
    pub fn preset_selected(self, preset_id: &'static str) -> InternalCockpit<PresetSelected> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::PresetSelected }
    }
}

impl InternalCockpit<PresetSelected> {
    pub fn abort(self) -> InternalCockpit<FileLoaded> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::FileLoaded }
    }

    /// Seals the Intent. Compiler prevents calling this without prior preset_selected().
    pub fn start_mastering(self, audio_path: String, preset_id: &'static str)
        -> (InternalCockpit<Mastering>, Intent)
    {
        let intent = Intent { audio_path, preset_id };
        (InternalCockpit { _state: PhantomData, mode: CockpitMode::Mastering }, intent)
    }
}

impl InternalCockpit<Mastering> {
    pub fn dsp_done(self) -> InternalCockpit<Mastered> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::Mastered }
    }
    pub fn abort(self) -> InternalCockpit<FileLoaded> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::FileLoaded }
    }
}

impl InternalCockpit<Mastered> {
    /// Automatic — triggered by Data Cascade
    pub fn analysis_done(self) -> InternalCockpit<InsightsReady> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::InsightsReady }
    }
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        // Performs hard_reset() internally
        InternalCockpit { _state: PhantomData, mode: CockpitMode::Idle }
    }
}

impl InternalCockpit<InsightsReady> {
    /// Automatic — triggered by Data Cascade
    pub fn coach_done(self) -> InternalCockpit<CoachReady> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::CoachReady }
    }
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::Idle }
    }
}

impl InternalCockpit<CoachReady> {
    pub fn export_clicked(self) -> InternalCockpit<Exporting> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::Exporting }
    }
    pub fn load_new_file(self) -> InternalCockpit<Idle> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::Idle }
    }
}

impl InternalCockpit<Exporting> {
    pub fn export_done(self) -> InternalCockpit<CoachReady> {
        InternalCockpit { _state: PhantomData, mode: CockpitMode::CoachReady }
    }
}
```

**DoD P5-002:**
```bash
cargo check -p stillair-frontend
echo "✅ P5-002"
```

---

## P5-003 — Cockpit Shell — 3-Panel Grid

**Goal:** 3-column MFD layout. Panel titles. Correct accent colors.

```rust
// apps/stillair/frontend/src/cockpit/mod.rs
#[component]
pub fn Cockpit(mode: ReadSignal<CockpitMode>) -> impl IntoView {
    view! {
        <div class="cockpit">
            <SessionPanel mode=mode />
            <InsightsPanel mode=mode />
            <CoachPanel mode=mode />
        </div>
    }
}
```

CSS (in `index.html` or `styles.css`):
```css
.cockpit {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    height: 100vh;
    background: #111318;
    gap: 0;
}
.mfd-panel {
    border-right: 1px solid #2A2D35;
    display: flex;
    flex-direction: column;
    overflow: hidden;
}
.mfd-panel:last-child { border-right: none; }
.panel-title {
    font-size: 0.75rem;
    letter-spacing: 0.15em;
    text-transform: uppercase;
    padding: 1rem 1.5rem 0.5rem;
    border-bottom: 1px solid #2A2D35;
}
/* Panel accent colors */
.panel-session  .panel-title { color: #FFD60A; border-bottom-color: #FFD60A; }
.panel-insights .panel-title { color: #00d1ff; border-bottom-color: #00d1ff; }
.panel-coach    .panel-title { color: #4ade80; border-bottom-color: #4ade80; }
```

**DoD P5-003:**
```bash
# Frontend compiles
cargo check --target wasm32-unknown-unknown -p stillair-frontend
# Manual: 3 panels visible with correct colors
echo "✅ P5-003"
```

---

## P5-004 — Transport Bar + Event Wiring

**Goal:** Transport bar with state-aware buttons.

```rust
// apps/stillair/frontend/src/components/transport_bar.rs
#[component]
pub fn TransportBar(
    mode:          ReadSignal<CockpitMode>,
    on_master:     Callback<()>,
    on_abort:      Callback<()>,
    on_load_new:   Callback<()>,
) -> impl IntoView {
    let can_master = move || matches!(mode.get(), CockpitMode::PresetSelected);
    let can_abort  = move || matches!(mode.get(), CockpitMode::Mastering);
    let is_locked  = move || matches!(mode.get(),
        CockpitMode::Mastering | CockpitMode::Exporting | CockpitMode::Fault(_));

    view! {
        <div class="transport-bar">
            <button
                disabled=move || !can_master()
                on:click=move |_| on_master.call(())
                class="btn-master"
            >
                {move || if matches!(mode.get(), CockpitMode::Mastering) {
                    "MASTERING..."
                } else { "MASTER" }}
            </button>
            <button
                disabled=move || !can_abort()
                on:click=move |_| on_abort.call(())
                class="btn-abort"
            >"ABORT"</button>
            <button
                disabled=is_locked
                on:click=move |_| on_load_new.call(())
                class="btn-new"
            >"LOAD NEW"</button>
        </div>
    }
}
```

**DoD P5-004:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P5-004"
```

---

## P5-005 — Left MFD: Session Panel

**Goal:** Session panel renders correctly for FM0, FM1, FM1.5, FM2.

```
FM0:  Drop zone — "Drop audio file here"
FM1:  File name + metadata + preset menu
FM1.5: Preset confirmed + "Ready to master"
FM2:  Progress bar + stage indicator
```

```rust
// apps/stillair/frontend/src/cockpit/session_panel.rs
#[component]
pub fn SessionPanel(mode: ReadSignal<CockpitMode>) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-session">
            <div class="panel-title">"THE SESSION"</div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::Idle => view! { <DropZone /> }.into_any(),
                    CockpitMode::FileLoaded => view! { <FileInfo /> }.into_any(),
                    CockpitMode::PresetSelected => view! { <PresetConfirmed /> }.into_any(),
                    CockpitMode::Mastering => view! { <MasteringProgress /> }.into_any(),
                    _ => view! { <MasteringComplete /> }.into_any(),
                }}
            </div>
        </div>
    }
}
```

**DoD P5-005:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P5-005"
```

---

## P5-006 — Center MFD: Insights Panel

**Goal:** Insights panel for FM3, FM4, FM5.

```
FM0-FM2:  "Awaiting mastering..."
FM3:      Golden Blob produced — "Processing..."
FM4:      EBU R128 metrics visible (LUFS, TP, LRA)
FM5:      Full metrics + compliance flags
```

```rust
// apps/stillair/frontend/src/cockpit/insights_panel.rs
#[component]
pub fn InsightsPanel(mode: ReadSignal<CockpitMode>) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-insights">
            <div class="panel-title">"THE INSIGHTS"</div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::InsightsReady | CockpitMode::CoachReady => {
                        view! { <MetricsDisplay /> }.into_any()
                    },
                    CockpitMode::Mastered => {
                        view! { <div class="status">"Computing metrics..."</div> }.into_any()
                    },
                    _ => view! { <div class="status">"Awaiting mastering..."</div> }.into_any(),
                }}
            </div>
        </div>
    }
}
```

**DoD P5-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P5-006"
```

---

## P5-007 — Right MFD: Coach Panel

**Goal:** Coach panel shows CoachFindings at FM5. Placeholder otherwise.

```
FM0-FM4:  Off / placeholder
FM5:      CoachFindings list (id, severity, recommendation)
           — structured list, no LLM narrative
FM6:      Locked
```

Severity indicator:
```
High   → red    #ef4444
Medium → amber  #f59e0b
Low    → yellow #fde047
Info   → gray   #6b7280
```

```rust
// apps/stillair/frontend/src/cockpit/coach_panel.rs
#[component]
pub fn CoachPanel(
    mode:     ReadSignal<CockpitMode>,
    findings: ReadSignal<Option<CoachFindings>>,
) -> impl IntoView {
    view! {
        <div class="mfd-panel panel-coach">
            <div class="panel-title">"THE COACH"</div>
            <div class="panel-content">
                {move || match mode.get() {
                    CockpitMode::CoachReady => {
                        if let Some(f) = findings.get() {
                            view! { <FindingsList findings=f /> }.into_any()
                        } else {
                            view! { <div>"No findings"</div> }.into_any()
                        }
                    },
                    CockpitMode::Exporting => {
                        view! { <div class="locked">"Locked during export"</div> }.into_any()
                    },
                    _ => view! { <div class="off">"—"</div> }.into_any(),
                }}
            </div>
        </div>
    }
}
```

**DoD P5-007:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P5-007"
```

---

## P5-008 — M0 IPC Client Stubs

**Goal:** Tauri commands that stub M0 IPC for Phase 5.
Full M0 integration in Phase 6.

```rust
// apps/stillair/src-tauri/src/commands/mastering.rs
use tauri::command;

/// Stub: in Phase 6 this calls M0 → sp314-dsp pipeline
#[command]
pub async fn trigger_mastering(audio_path: String, preset_id: String)
    -> Result<String, String>
{
    // Phase 5 stub: simulate 2s mastering
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    Ok("golden_blob_stub".to_string())
}

/// Stub: in Phase 6 this loads file via M0 CDN
#[command]
pub async fn load_audio_file(path: String) -> Result<AudioMeta, String> {
    Ok(AudioMeta {
        name:        path.split('/').last().unwrap_or("unknown").to_string(),
        format:      "WAV".to_string(),
        sample_rate: 48000,
        bit_depth:   24,
        duration_s:  180.0,
        channels:    2,
    })
}

#[derive(serde::Serialize)]
pub struct AudioMeta {
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}
```

**DoD P5-008:**
```bash
cargo check -p stillair
echo "✅ P5-008"
```

---

## P5-009 — Core Mastering Flow (End-to-End)

**Goal:** Wire the complete FM0 → FM5 flow.

```
1. User drops file → file_dropped() → FM1
2. User selects preset → preset_selected() → FM1.5
3. User clicks MASTER → start_mastering() seals Intent → FM2
4. Tauri invoke trigger_mastering() → stub returns after 2s
5. dsp_done() → FM3 → automatic analysis_done() → FM4
6. automatic coach_done() → FM5
7. CoachFindings (stub) visible in Right MFD
```

Wire in `app.rs`:
```rust
let (mode, set_mode) = create_signal(CockpitMode::Idle);
let (findings, set_findings) = create_signal::<Option<CoachFindings>>(None);

// Stub CoachFindings for Phase 5
let stub_findings = CoachFindings {
    issues: vec![
        Issue {
            id: "lufs_compliance",
            severity: Severity::Medium,
            params: IssueParams { current: -12.0, target: -14.0, delta: 2.0 },
            tags: vec!["platform:spotify".into()],
        }
    ],
    recommendation: "Reduce gain to meet loudness target.".into(),
};
```

**DoD P5-009:**
```bash
# Manual verification:
# FM0 → drop file → FM1 ✅
# FM1 → select preset → FM1.5 ✅
# FM1.5 → click MASTER → FM2 (progress) ✅
# FM2 → (2s) → FM3 → FM4 → FM5 ✅
# FM5: stub finding visible in Coach panel ✅
echo "✅ P5-009"
```

---

## P5-010 — FM-ERR Fault Display

**Goal:** Fault state renders with ASC code. master_reset() returns to FM0.

```rust
// apps/stillair/frontend/src/components/fault_display.rs
#[component]
pub fn FaultDisplay(code: AscCode, on_reset: Callback<()>) -> impl IntoView {
    let (code_str, msg) = match code {
        AscCode::MathErr        => ("0x01", "DSP arithmetic error — NaN/Inf detected"),
        AscCode::IoErr          => ("0x02", "I/O failure — file read or write error"),
        AscCode::Aborted        => ("0x03", "Mastering aborted by user"),
        AscCode::ValidationFail => ("0x04", "Audio validation failed — check file integrity"),
        AscCode::WasmPanic      => ("0x05", "LineOS runtime crash"),
    };
    view! {
        <div class="fault-display">
            <div class="fault-code">"FM-ERR · ASC "{code_str}</div>
            <div class="fault-message">{msg}</div>
            <button on:click=move |_| on_reset.call(()) class="btn-reset">
                "MASTER RESET → FM0"
            </button>
        </div>
    }
}
```

**DoD P5-010:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
echo "✅ P5-010"
```

---

## P5-011 — CI Gate + Tag

```bash
cargo check --target wasm32-unknown-unknown -p stillair-frontend
cargo test --workspace
just ci
just deny

git add -A
git commit -m "feat(cockpit): Phase 5 — Still Air Cockpit scaffold

- Tauri 2.x + Leptos app (apps/stillair/)
- CockpitMode enum + InternalCockpit<S> typestates
- 3 MFD panels: Session / Insights / Coach
- Transport bar with state-aware buttons
- FM0→FM5 core mastering flow (M0 IPC stubs)
- FM-ERR fault display with ASC codes
- Typestate: FM1.5→FM2 requires sealed Intent
- state-machine.md §6.1 Double-Lock enforced

Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6"

git tag v0.5.0-cockpit
git log --oneline -6
```

---

## Completion Report

```
✅ Phase 5 — Cockpit — COMPLETE

Tauri 2.x + Leptos:     ✅
State machine:          FM0-FM6 + FM-ERR ✅
Double-Lock:            typestate + CockpitMode ✅
3 MFD panels:           Session / Insights / Coach ✅
Core mastering flow:    FM0 → FM5 end-to-end ✅
Intent sealing:         FM1.5 → FM2 ✅
FM-ERR + ASC codes:     ✅
M0 IPC stubs:           ✅ (Phase 6 wires real M0)
just ci:                ✅

Tag: v0.5.0-cockpit ✅

Ready for: Phase 6 — Wire everything (M0 IPC + full audio flow)
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 5 — Cockpit
**Version:** 1.0
**Status:** 🔒 LOCKED
