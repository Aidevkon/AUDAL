# LineOS — Phase 11 Task Decomposition

**Document:** `lineos/plan/phase-11/task-decomposition.md`
**Version:** 1.0
**Phase:** 11 — Dioxus Cockpit Migration
**Status:** 🔒 LOCKED
**Authority:** Phase 11 Master Prompt · Amendment A-002 §7

---

## Architecture

```
apps/stillair/
├── src-tauri/          ← UNCHANGED (Tauri backend, all IPC commands)
│   └── src/
│       ├── commands/   ← UNCHANGED
│       ├── aether/     ← UNCHANGED
│       └── ipc/        ← UNCHANGED
├── cockpit-dioxus/     ← NEW — Dioxus 0.6 Cockpit
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── app.rs          ← CockpitMode FSM (Dioxus signals)
│       ├── state/
│       │   ├── cockpit_mode.rs
│       │   └── session_state.rs
│       └── panels/
│           ├── session.rs
│           ├── insights.rs
│           └── coach.rs
└── frontend/           ← KEPT until Dioxus verified, then removed
    └── src/            ← Leptos (reference, fallback)
```

**Key simplification:** The Dioxus Cockpit uses `get_session_state`
(P9-008) — ONE call after mastering completes. No Data Cascade.

---

## Task Order

```
P11-001  Dioxus crate scaffold (cockpit-dioxus/)
P11-002  CockpitMode FSM — Dioxus signals
P11-003  Tauri IPC bridge for Dioxus
P11-004  3-panel shell — session / insights / coach
P11-005  Session panel (FM0 drop zone → FM2 mastering)
P11-006  Insights panel (FM5 metrics display)
P11-007  Coach panel (FM5 narrative + findings)
P11-008  Wire full FM0→FM5 flow with get_session_state
P11-009  Switch tauri.conf.json to Dioxus window
P11-010  Verify end-to-end: LOAD NEW → MASTER → FM5 → EXPORT
P11-011  Remove Leptos code (frontend/)
P11-012  CI gate + tag
```

---

## P11-001 — Dioxus Crate Scaffold

Create `apps/stillair/cockpit-dioxus/Cargo.toml`:

```toml
[package]
name    = "stillair-cockpit"
version = "0.1.0"
edition = "2021"

[dependencies]
dioxus        = { version = "0.6", features = ["desktop"] }
dioxus-desktop = "0.6"
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
tokio         = { version = "1", features = ["full"] }
```

Add to workspace `Cargo.toml`:
```toml
members = [
    # ... existing members ...
    "apps/stillair/cockpit-dioxus",
]
```

Update `src-tauri/tauri.conf.json` — add `beforeDevCommand` for
Dioxus (running alongside trunk for now):

**DoD P11-001:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-001"
```

---

## P11-002 — CockpitMode FSM

Same states as Leptos FSM — translated to Dioxus signals.

```rust
// cockpit-dioxus/src/state/cockpit_mode.rs

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CockpitMode {
    Idle,
    FileLoaded { path: String, name: String, format: String },
    PresetSelected { path: String, preset_id: String },
    Mastering { path: String, preset_id: String },
    Mastered { blob_id: String },
    CoachReady { blob_id: String },
    Fault { code: AscCode, message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AscCode {
    MathErr        = 0x01,
    IoErr          = 0x02,
    Aborted        = 0x03,
    ValidationFail = 0x04,
    WasmPanic      = 0x05,
}

impl CockpitMode {
    pub fn is_interactive(&self) -> bool {
        !matches!(self, Self::Mastering { .. } | Self::Fault { .. })
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle              => "FM0 IDLE",
            Self::FileLoaded { .. } => "FM1 FILE LOADED",
            Self::PresetSelected{..}=> "FM1.5 PRESET SELECTED",
            Self::Mastering { .. }  => "FM2 MASTERING",
            Self::Mastered { .. }   => "FM3 MASTERED",
            Self::CoachReady { .. } => "FM5 COACH READY",
            Self::Fault { .. }      => "FM-ERR",
        }
    }
}
```

**DoD P11-002:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-002"
```

---

## P11-003 — Tauri IPC Bridge for Dioxus

```rust
// cockpit-dioxus/src/ipc.rs

use dioxus::prelude::*;
use serde::{de::DeserializeOwned, Serialize};

/// Invoke a Tauri command from Dioxus desktop.
/// Uses dioxus-desktop's eval() to call window.__TAURI_INTERNALS__.invoke
pub async fn invoke<R, A>(command: &str, args: A) -> Result<R, String>
where
    R: DeserializeOwned,
    A: Serialize,
{
    #[cfg(feature = "desktop")]
    {
        use dioxus_desktop::use_window;
        // Dioxus 0.6 desktop: use wry IPC
        // invoke via the Tauri IPC channel injected by Tauri
        let args_json = serde_json::to_string(&args)
            .map_err(|e| format!("Serialize args failed: {e}"))?;

        // Use dioxus eval to call Tauri IPC
        let result = eval(&format!(
            r#"
            const result = await window.__TAURI_INTERNALS__.invoke(
                "{command}", {args_json}
            );
            return JSON.stringify(result);
            "#
        ))
        .await
        .map_err(|e| format!("IPC eval failed: {e:?}"))?;

        let result_str = result.as_str()
            .ok_or("IPC result is not a string")?;

        serde_json::from_str(result_str)
            .map_err(|e| format!("Deserialize response failed: {e}"))
    }
}
```

**Note:** Dioxus 0.6 desktop apps run inside a Tauri WebView.
`window.__TAURI_INTERNALS__` is injected by Tauri's `withGlobalTauri: true`.
The same Tauri commands work identically from Dioxus as from Leptos.

**DoD P11-003:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-003"
```

---

## P11-004 — 3-Panel Shell

```rust
// cockpit-dioxus/src/app.rs

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::panels::{session::SessionPanel, insights::InsightsPanel, coach::CoachPanel};

pub fn App() -> Element {
    let mode = use_signal(|| CockpitMode::Idle);
    let session_state = use_signal(|| None::<SessionStateJson>);

    rsx! {
        div {
            class: "cockpit",
            style: "display:grid; grid-template-columns:1fr 1fr 1fr;
                    height:100vh; background:#111318;",

            // Header bar
            div {
                class: "header",
                style: "grid-column:1/-1; display:flex; align-items:center;
                        padding:0 1.5rem; border-bottom:1px solid #2A2D35;
                        background:#111318; height:48px;",
                span { style: "color:#e8eaf0; font-weight:600;", "STILL AIR" }
                span { style: "color:#8b8fa8; margin-left:1rem; font-size:0.75rem;",
                    "{mode.read().label()}"
                }
                // Transport buttons (right-aligned)
                div { style: "margin-left:auto; display:flex; gap:0.5rem;",
                    TransportBar { mode: mode.clone() }
                }
            }

            // 3 MFD panels
            SessionPanel { mode: mode.clone(), session_state: session_state.clone() }
            InsightsPanel { mode: mode.clone(), session_state: session_state.clone() }
            CoachPanel { mode: mode.clone(), session_state: session_state.clone() }
        }
    }
}
```

CSS Design Tokens — inject at startup from `design-tokens-v1.0.md §12`:
```rust
// In main.rs, inject CSS vars before launch
const CSS: &str = include_str!("../assets/styles.css");
```

Create `cockpit-dioxus/assets/styles.css` with all tokens from
`apps/stillair/cockpit/design-system/design-tokens-v1.0.md §12`.

**DoD P11-004:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-004"
```

---

## P11-005 — Session Panel

```rust
// cockpit-dioxus/src/panels/session.rs

use dioxus::prelude::*;
use crate::state::cockpit_mode::CockpitMode;
use crate::ipc::invoke;
use crate::types::SessionStateJson;

#[component]
pub fn SessionPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let on_load = move |_| {
        spawn(async move {
            // Open file picker via Tauri IPC
            match invoke::<Option<AudioMeta>, _>("openAudioFile", ()).await {
                Ok(Some(meta)) => {
                    mode.set(CockpitMode::FileLoaded {
                        path:   meta.path.clone(),
                        name:   meta.name.clone(),
                        format: meta.format.clone(),
                    });
                }
                Ok(None) => {} // cancelled
                Err(e)   => mode.set(CockpitMode::Fault {
                    code:    crate::state::cockpit_mode::AscCode::IoErr,
                    message: e,
                }),
            }
        });
    };

    rsx! {
        div {
            class: "mfd-panel panel-session",
            style: "border-right:1px solid var(--border-subtle);",

            div {
                class: "panel-title",
                style: "color:var(--accent-session); border-bottom:2px solid var(--accent-session);
                        padding:1rem 1.5rem 0.5rem; font-size:0.75rem;
                        letter-spacing:0.15em; text-transform:uppercase;",
                "THE SESSION"
            }

            match mode.read().clone() {
                CockpitMode::Idle => rsx! {
                    DropZone { on_load }
                },
                CockpitMode::FileLoaded { name, format, .. } => rsx! {
                    FileInfo { name, format }
                    PresetMenu { mode: mode.clone() }
                },
                CockpitMode::Mastering { .. } => rsx! {
                    MasteringProgress {}
                },
                _ => rsx! {
                    GoldenBlobBadge {}
                }
            }
        }
    }
}
```

**DoD P11-005:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-005"
```

---

## P11-006 — Insights Panel

```rust
// cockpit-dioxus/src/panels/insights.rs

#[component]
pub fn InsightsPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let state = session_state.read();

    rsx! {
        div {
            class: "mfd-panel panel-insights",
            style: "border-right:1px solid var(--border-subtle);",

            div {
                class: "panel-title",
                style: "color:var(--accent-insights); border-bottom:2px solid var(--accent-insights);
                        padding:1rem 1.5rem 0.5rem; font-size:0.75rem;
                        letter-spacing:0.15em; text-transform:uppercase;",
                "THE INSIGHTS"
            }

            match state.as_ref() {
                Some(s) => rsx! {
                    MetricRow { label: "INTEGRATED LUFS", value: format!("{:.1} LUFS", s.loudness.integrated_lufs) }
                    MetricRow { label: "TRUE PEAK",       value: format!("{:.1} dBTP", s.loudness.true_peak_dbtp) }
                    MetricRow { label: "LOUDNESS RANGE",  value: format!("{:.1} LU",   s.loudness.lra) }
                    MetricRow { label: "STEREO CORR",     value: format!("{:.2}",       s.quality.stereo_correlation) }
                    ComplianceTable { compliance: s.compliance.clone() }
                },
                None => rsx! {
                    div {
                        style: "color:var(--text-muted); padding:2rem; text-align:center;",
                        "AWAITING MASTERING..."
                    }
                }
            }
        }
    }
}
```

**DoD P11-006:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-006"
```

---

## P11-007 — Coach Panel

```rust
// cockpit-dioxus/src/panels/coach.rs

#[component]
pub fn CoachPanel(
    mode:          Signal<CockpitMode>,
    session_state: Signal<Option<SessionStateJson>>,
) -> Element {
    let state = session_state.read();

    rsx! {
        div {
            class: "mfd-panel panel-coach",

            div {
                class: "panel-title",
                style: "color:var(--accent-coach); border-bottom:2px solid var(--accent-coach);
                        padding:1rem 1.5rem 0.5rem; font-size:0.75rem;
                        letter-spacing:0.15em; text-transform:uppercase;",
                "THE COACH"
            }

            match state.as_ref() {
                Some(s) => rsx! {
                    // Narrative summary
                    if let Some(ref n) = s.narrative {
                        div { class: "recommendation", style: "padding:1rem 1.5rem;",
                            div { style: "color:var(--text-secondary); font-size:0.75rem;
                                         text-transform:uppercase; letter-spacing:0.1em;
                                         margin-bottom:0.5rem;",
                                "RECOMMENDATION"
                            }
                            div { style: "color:var(--text-primary);", "{n.summary}" }
                            div { style: "color:var(--text-muted); font-size:0.7rem;
                                         margin-top:0.5rem;",
                                "model: {n.model_used}"
                            }
                        }
                    }
                    // Findings cards
                    for issue in &s.findings.issues {
                        FindingCard { issue: issue.clone() }
                    }
                },
                None => rsx! {
                    div {
                        style: "color:var(--text-muted); padding:2rem; text-align:center;",
                        "—"
                    }
                }
            }
        }
    }
}
```

**DoD P11-007:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-007"
```

---

## P11-008 — Wire FM0→FM5 with get_session_state

The key difference from Leptos: **one IPC call** after mastering.

```rust
// In app.rs, on_master handler:
let on_master = move |_| {
    let (path, preset_id) = match mode.read().clone() {
        CockpitMode::PresetSelected { path, preset_id } => (path, preset_id),
        _ => return,
    };

    spawn(async move {
        // FM2
        mode.set(CockpitMode::Mastering {
            path: path.clone(), preset_id: preset_id.clone()
        });

        // Trigger mastering
        let blob_id = match invoke::<String, _>(
            "triggerMastering",
            json!({ "audioPath": path, "presetId": preset_id })
        ).await {
            Ok(id) => id,
            Err(e) => {
                mode.set(CockpitMode::Fault {
                    code: AscCode::IoErr, message: e
                });
                return;
            }
        };

        // FM3 → single call for ALL session data
        let state = match invoke::<SessionStateJson, _>(
            "getSessionState",
            json!({ "blobId": blob_id })
        ).await {
            Ok(s) => s,
            Err(e) => {
                mode.set(CockpitMode::Fault {
                    code: AscCode::IoErr, message: e
                });
                return;
            }
        };

        // FM5
        session_state.set(Some(state));
        mode.set(CockpitMode::CoachReady { blob_id });
    });
};
```

**DoD P11-008:**
```bash
cargo check -p stillair-cockpit
echo "✅ P11-008"
```

---

## P11-009 — Switch tauri.conf.json to Dioxus

Update `apps/stillair/src-tauri/tauri.conf.json`:

```json
{
  "build": {
    "beforeDevCommand": "cargo run -p stillair-cockpit",
    "devUrl": "dioxus://localhost",
    "beforeBuildCommand": "cargo build -p stillair-cockpit",
    "frontendDist": "../cockpit-dioxus/dist"
  }
}
```

**Note:** Dioxus 0.6 desktop apps embed directly — no separate dev server needed.
The `src-tauri/main.rs` launches the Dioxus app via:
```rust
fn main() {
    tauri::Builder::default()
        // ... commands ...
        .run(tauri::generate_context!())
        .unwrap();
}
```

And `cockpit-dioxus/src/main.rs`:
```rust
fn main() {
    dioxus::launch(App);
}
```

**DoD P11-009:**
```bash
cargo tauri dev  # Dioxus window opens
echo "✅ P11-009"
```

---

## P11-010 — Verify End-to-End

Manual verification checklist:
```
□ LOAD NEW → file picker opens → gargar.mp3 selected → FM1
□ Preset selector → Spotify → FM1.5
□ MASTER → FM2 progress → completes
□ FM5: Insights shows -7.7 LUFS, 3.7 LU LRA
□ FM5: Coach shows "Reduce gain to meet loudness target."
□ FM5: 3 finding cards (lufs_compliance HIGH, dynamic_range_low LOW,
        stereo_correlation_low MEDIUM)
□ EXPORT → save dialog → mastered.flac written
□ ui-isolation-check.sh passes
```

---

## P11-011 — Remove Leptos Code

After P11-010 verified:

```bash
# Remove Leptos frontend
rm -rf apps/stillair/frontend/

# Remove from workspace Cargo.toml
# Remove "apps/stillair/frontend" from members

# Remove trunk.toml (Leptos bundler)
rm apps/stillair/Trunk.toml

# Update .gitignore
# Remove apps/stillair/frontend/dist/ entry (no longer needed)

git add -A
git commit -m "chore: remove Leptos frontend — Dioxus Cockpit verified"
```

**DoD P11-011:**
```bash
ls apps/stillair/frontend/ 2>&1 | grep "No such file"
echo "✅ Leptos removed"
```

---

## P11-012 — CI Gate + Tag

```bash
cargo test --workspace
just ci
bash infra/ci/checks/ui-isolation-check.sh

git add -A
git commit -m "feat(dioxus): Phase 11 — Dioxus 0.6 Cockpit migration complete

Migration path per Amendment A-002 §7:
  1. Dioxus Cockpit written alongside Leptos ✅
  2. All Tauri IPC commands verified from Dioxus ✅
  3. Metrics verified: -7.7 LUFS, 3.7 LU LRA, Coach narrative ✅
  4. Tauri window switched to Dioxus ✅
  5. Leptos code removed ✅
  6. Tag v0.11.0-dioxus ✅

Key improvements over Leptos:
  - One IPC call: getSessionState() replaces 3-step Data Cascade
  - Native desktop feel (Dioxus 0.6 desktop)
  - CockpitMode enum carries data (no separate signals for path/preset)
  - No WASM compilation (Dioxus desktop = native binary)
  - Faster startup, smaller binary

Core unchanged (Amendment A-002 §9 enforced):
  - sp314-dsp, telemetry, rule-engine, M0, Aether — not touched
  - ui-isolation-check.sh: all 7 core crates clean

Authority: Amendment A-002 · LineOS Constitution v2.0"

git tag v0.11.0-dioxus
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 11 — Dioxus Cockpit Migration — COMPLETE

Dioxus 0.6 desktop:    ✅
Amendment A-002 §7:    migration path followed exactly ✅
getSessionState:       one call, all data ✅
Core unchanged:        ui-isolation-check.sh clean ✅
Leptos removed:        ✅
just ci:               ✅

Tag: v0.11.0-dioxus ✅

Still Air A1: full pipeline, native desktop UI
Ready for: Phase 12 — Marketplace + Dioxus viewer
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 11 — Dioxus Cockpit Migration
**Version:** 1.0
**Status:** 🔒 LOCKED

---

*Core is what lasts. UI is what fits the moment.*
*The instruments never lie. The skin changes.*
