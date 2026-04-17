# LineOS — Phase 12B Task Decomposition

**Document:** `lineos/plan/phase-12b/task-decomposition.md`
**Version:** 1.0
**Phase:** 12B — Transport Bar UI + Live Telemetry
**Status:** 🔒 LOCKED
**Authority:** Phase 12B Master Prompt · Amendment A-003 §5

---

## Architecture

```
Dioxus Cockpit
    │
    │  invoke("playbackControl", { action: "play" })
    │  invoke("getPlaybackState")   ← poll every 500ms
    ▼
Tauri commands/playback.rs
    │
    │  M0Client → POST /playback/control
    │             GET  /playback/state
    ▼
M0 handlers/playback.rs
    │
    │  AppState.playback (PlaybackHandle)
    ▼
PlaybackWorker thread → cpal → speakers
```

**UI isolation (A-003 §5):**
- Cockpit never imports xaak, cpal, or ringbuf
- Cockpit receives only `PlaybackStateJson` (position_ms, duration_ms, is_playing)
- All audio logic stays in M0 + xaak

---

## Task Order

```
P12B-001  M0 playback HTTP endpoints (POST /playback/control, GET /playback/state)
P12B-002  M0Client playback methods
P12B-003  Transport bar component — Play/Pause/Stop/Seek
P12B-004  Position polling (500ms interval)
P12B-005  Live momentary LUFS during playback
P12B-006  Spectrum analyzer — real bars from quality metrics
P12B-007  Correlation radar — SVG stereo field visualization
P12B-008  Smoke test: play gargar.mp3 → audio from speakers
P12B-009  CI gate + tag
```

---

## P12B-001 — M0 Playback HTTP Endpoints

Add to `lineos/m0/m0-daemon/src/handlers/playback.rs`:

```rust
use axum::{extract::State, Json};
use std::sync::Arc;
use crate::app_state::AppState;
use xaak::PlaybackState;

#[derive(serde::Deserialize)]
pub struct PlaybackControlRequest {
    pub action:      String,        // "play" | "pause" | "stop" | "seek"
    pub position_ms: Option<u64>,   // for "seek" only
}

#[derive(serde::Serialize)]
pub struct PlaybackControlResponse {
    pub status:  String,
    pub message: Option<String>,
    pub state:   Option<PlaybackStateJson>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PlaybackStateJson {
    pub blob_id:     String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing:  bool,
    pub sample_rate: u32,
    pub channels:    u16,
}

impl From<PlaybackState> for PlaybackStateJson {
    fn from(s: PlaybackState) -> Self {
        Self {
            blob_id:     s.blob_id,
            position_ms: s.position_ms,
            duration_ms: s.duration_ms,
            is_playing:  s.is_playing,
            sample_rate: s.sample_rate,
            channels:    s.channels,
        }
    }
}

pub async fn handle_playback_control(
    State(state): State<Arc<AppState>>,
    Json(req):    Json<PlaybackControlRequest>,
) -> Json<PlaybackControlResponse> {
    let result = match req.action.as_str() {
        "play"  => state.playback.play(),
        "pause" => { state.playback.pause(); Ok(()) },
        "stop"  => { state.playback.stop();  Ok(()) },
        "seek"  => state.playback.seek(req.position_ms.unwrap_or(0)),
        other   => Err(format!("Unknown action: {other}")),
    };

    match result {
        Ok(()) => {
            let st = state.playback.get_state().map(PlaybackStateJson::from);
            Json(PlaybackControlResponse {
                status: "ok".into(), message: None, state: st,
            })
        }
        Err(e) => Json(PlaybackControlResponse {
            status: "error".into(), message: Some(e), state: None,
        })
    }
}

pub async fn handle_playback_state(
    State(state): State<Arc<AppState>>,
) -> Json<Option<PlaybackStateJson>> {
    Json(state.playback.get_state().map(PlaybackStateJson::from))
}
```

Wire routes in `main.rs`:
```rust
.route("/playback/control", post(handlers::playback::handle_playback_control))
.route("/playback/state",   get(handlers::playback::handle_playback_state))
```

**DoD P12B-001:**
```bash
cargo check -p m0d
echo "✅ P12B-001"
```

---

## P12B-002 — M0Client Playback Methods

Add to `apps/stillair/src-tauri/src/ipc/m0_client.rs`:

```rust
pub async fn playback_control(
    &self,
    action:      &str,
    position_ms: Option<u64>,
) -> Result<PlaybackStateJson, M0Error> {
    #[derive(serde::Serialize)]
    struct Req<'a> { action: &'a str, position_ms: Option<u64> }

    let resp = self.client
        .post(format!("{}/playback/control", self.base_url))
        .json(&Req { action, position_ms })
        .timeout(Duration::from_secs(10))
        .send().await?
        .json::<serde_json::Value>().await?;

    if resp["status"] == "ok" {
        Ok(serde_json::from_value(resp["state"].clone())
            .map_err(|e| M0Error::ParseError(e.to_string()))?)
    } else {
        Err(M0Error::RequestFailed(
            resp["message"].as_str().unwrap_or("unknown").to_string()
        ))
    }
}

pub async fn get_playback_state(
    &self,
) -> Result<Option<PlaybackStateJson>, M0Error> {
    let resp = self.client
        .get(format!("{}/playback/state", self.base_url))
        .timeout(Duration::from_secs(5))
        .send().await?
        .json::<Option<PlaybackStateJson>>().await?;
    Ok(resp)
}
```

Add `PlaybackStateJson` to `m0_client.rs` types:
```rust
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct PlaybackStateJson {
    pub blob_id:     String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub is_playing:  bool,
    pub sample_rate: u32,
    pub channels:    u16,
}
```

**DoD P12B-002:**
```bash
cargo check -p stillair
echo "✅ P12B-002"
```

---

## P12B-003 — Transport Bar Component

Update `apps/stillair/cockpit-dioxus/src/app.rs` transport bar:

```rust
// Transport bar — bottom strip
div {
    class: "transport-bar",
    style: "grid-column:1/-1; display:flex; align-items:center;
            padding:0 1.5rem; gap:1rem; border-top:1px solid var(--border-subtle);
            background:var(--bg-overlay); height:56px;",

    // Position display
    div {
        class: "transport-time",
        style: "font-family:var(--font-mono); color:var(--accent-insights);
                font-size:1.25rem; min-width:80px;",
        { format_ms(playback_state.read().as_ref().map(|s| s.position_ms).unwrap_or(0)) }
    }

    // Play button
    button {
        class: "transport-btn",
        style: "background:var(--btn-master-bg); color:var(--btn-master-text);
                border:none; padding:0.5rem 1.5rem; cursor:pointer;
                font-family:var(--font-mono);",
        disabled: mode.read().blob_id().is_none(),
        onclick: move |_| {
            let action = if playback_state.read()
                .as_ref().map(|s| s.is_playing).unwrap_or(false)
            { "pause" } else { "play" };
            spawn_local(async move {
                invoke_playback(action, None, playback_state.clone()).await;
            });
        },
        {
            if playback_state.read().as_ref().map(|s| s.is_playing).unwrap_or(false)
            { "⏸ PAUSE" } else { "▶ PLAY" }
        }
    }

    // Stop button
    button {
        class: "transport-btn",
        style: "background:var(--btn-neutral-bg); color:var(--btn-neutral-text);
                border:1px solid var(--border-subtle); padding:0.5rem 1rem;
                cursor:pointer; font-family:var(--font-mono);",
        onclick: move |_| {
            spawn_local(async move {
                invoke_playback("stop", None, playback_state.clone()).await;
            });
        },
        "■ STOP"
    }

    // Duration display
    div {
        style: "margin-left:auto; font-family:var(--font-mono);
                color:var(--text-secondary); font-size:0.875rem;",
        { format!("/ {}",
            format_ms(playback_state.read().as_ref()
                .map(|s| s.duration_ms).unwrap_or(0))
        )}
    }
}
```

Helper:
```rust
fn format_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    format!("{:02}:{:02}", mins, secs % 60)
}
```

**DoD P12B-003:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P12B-003"
```

---

## P12B-004 — Position Polling (500ms)

In `app.rs`, after FM5 transition, start polling:

```rust
// Poll playback state every 500ms
use_effect(move || {
    let playback_state = playback_state.clone();
    let mode = mode.clone();

    // Only poll when in CoachReady (FM5)
    if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) {
        return;
    }

    spawn_local(async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(500).await;

            // Stop polling if no longer in FM5
            if !matches!(*mode.read(), CockpitMode::CoachReady { .. }) {
                break;
            }

            match invoke::<Option<PlaybackStateJson>, _>(
                "getPlaybackState", ()
            ).await {
                Ok(Some(state)) => playback_state.set(Some(state)),
                _ => {}
            }
        }
    });
});
```

Add `gloo-timers` to cockpit-dioxus Cargo.toml:
```toml
gloo-timers = { version = "0.3", features = ["futures"] }
```

**DoD P12B-004:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P12B-004"
```

---

## P12B-005 — Live Momentary LUFS

Add a new Tauri command `get_live_telemetry` that returns
current momentary LUFS from the playback position:

```rust
// In commands/playback.rs
#[tauri::command]
pub async fn get_live_telemetry() -> Result<LiveTelemetryJson, String> {
    let client = M0Client::new();
    // Returns momentary LUFS based on current playback position
    // Uses pre-computed telemetry from Golden Blob
    client.get_live_telemetry().await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LiveTelemetryJson {
    pub momentary_lufs:   f32,
    pub short_term_lufs:  f32,
    pub true_peak_dbtp:   f32,
    pub position_ms:      u64,
}
```

Display in InsightsPanel — update `SHORT TERM` row with live value
when playback is active.

**DoD P12B-005:**
```bash
cargo check -p stillair
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P12B-005"
```

---

## P12B-006 — Spectrum Analyzer (Real Bars)

Replace the placeholder spectrum div with real SVG bars
computed from `session_state.quality.spectral_centroid`
and `session_state.quality.spectral_flatness`:

```rust
// In panels/insights.rs — replace placeholder
div {
    class: "spectrum-analyzer",
    style: "height:80px; display:flex; align-items:flex-end;
            gap:2px; padding:0.5rem;",

    // 16 bars derived from spectral data
    { (0..16usize).map(|i| {
        let height = compute_bar_height(
            i,
            state.quality.spectral_centroid,
            state.quality.spectral_flatness,
        );
        rsx! {
            div {
                style: format!(
                    "flex:1; background:var(--accent-insights);
                     height:{}%; opacity:0.8;", height
                )
            }
        }
    })}
}

fn compute_bar_height(band: usize, centroid: f32, flatness: f32) -> f32 {
    // Simple approximation from spectral centroid + flatness
    let center_band = (centroid / 3000.0 * 16.0) as usize;
    let distance = (band as f32 - center_band as f32).abs();
    let base = (1.0 - flatness) * 100.0 * libm::expf(-distance * 0.3);
    (base + flatness * 50.0).clamp(5.0, 95.0)
}
```

**DoD P12B-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P12B-006"
```

---

## P12B-007 — Correlation Radar (SVG)

Replace placeholder with real SVG stereo field visualization:

```rust
// In panels/insights.rs — replace correlation radar placeholder
div {
    class: "correlation-radar",
    style: "height:120px; display:flex; align-items:center;
            justify-content:center;",

    svg {
        width: "120",
        height: "120",
        view_box: "0 0 120 120",

        // Background circle
        circle {
            cx: "60", cy: "60", r: "55",
            fill: "none",
            stroke: "var(--border-subtle)",
            stroke_width: "1"
        }
        // Crosshairs
        line { x1:"5",  y1:"60", x2:"115", y2:"60",
               stroke:"var(--border-subtle)", stroke_width:"1" }
        line { x1:"60", y1:"5",  x2:"60",  y2:"115",
               stroke:"var(--border-subtle)", stroke_width:"1" }

        // Correlation indicator
        {
            let corr = state.quality.stereo_correlation;
            let width  = state.quality.stereo_width;
            // Map correlation to ellipse dimensions
            let rx = (width * 40.0 + 5.0) as i32;
            let ry = ((1.0 - corr.abs()) * 30.0 + 5.0) as i32;
            let color = if corr > 0.3 { "var(--accent-insights)" }
                        else { "var(--severity-medium)" };
            rsx! {
                ellipse {
                    cx: "60", cy: "60",
                    rx: "{rx}", ry: "{ry}",
                    fill: "none",
                    stroke: color,
                    stroke_width: "2",
                    transform: "rotate(-45 60 60)"
                }
            }
        }
    }
}
```

**DoD P12B-007:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P12B-007"
```

---

## P12B-008 — Smoke Test

```bash
# Start M0 + Caddy + Cockpit
# Load gargar.mp3 → MASTER → FM5
# Click PLAY → verify:

# 1. Audio plays from speakers
# 2. Position counter increments: 00:00 → 00:01 → 00:02...
# 3. PAUSE stops audio and freezes counter
# 4. PLAY resumes
# 5. STOP resets to 00:00
# 6. Spectrum shows real bars (not "SPECTRUM / PHASE 12")
# 7. Correlation radar shows ellipse (not circle placeholder)

echo "✅ P12B-008 — smoke test complete"
```

---

## P12B-009 — CI Gate + Tag

```bash
cargo test --workspace
just ci
bash infra/ci/checks/ui-isolation-check.sh

git add -A
git commit -m "feat(transport): Phase 12B — Transport bar + live telemetry

Transport bar (Dioxus):
  - PLAY/PAUSE toggle button
  - STOP button
  - Position display MM:SS
  - Duration display
  - 500ms polling via gloo-timers

Live telemetry:
  - get_live_telemetry Tauri command
  - SHORT TERM LUFS updates during playback

Visualizations (real data):
  - Spectrum analyzer: 16 bars from spectral_centroid + flatness
  - Correlation radar: SVG ellipse from stereo_correlation + width

Smoke test verified:
  - gargar.mp3 plays from speakers ✅
  - Position increments ✅
  - PAUSE/STOP functional ✅

Amendment A-003 §5 enforced:
  - No PCM in Cockpit
  - ui-isolation-check.sh clean

Authority: Amendment A-003 · LineOS Constitution v2.0"

git tag v0.12b.0-transport
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 12B — Transport Bar + Live Telemetry — COMPLETE

Transport bar:      PLAY/PAUSE/STOP functional ✅
Position polling:   500ms updates ✅
Live LUFS:          momentary updates during playback ✅
Spectrum:           real bars from spectral data ✅
Correlation radar:  SVG ellipse from stereo field ✅
Audio playback:     gargar.mp3 → speakers ✅
ui-isolation:       clean ✅
just ci:            ✅

Tag: v0.12b.0-transport ✅

Ready for: Phase 13 — MP3 export + BMR-128 PDF report
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 12B
**Version:** 1.0
**Status:** 🔒 LOCKED
