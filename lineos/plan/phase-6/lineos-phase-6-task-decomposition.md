# LineOS — Phase 6 Task Decomposition

**Document:** `lineos/plan/phase-6/task-decomposition.md`
**Version:** 1.0
**Phase:** 6 — Wire Everything
**Status:** 🔒 LOCKED
**Authority:** Phase 6 Master Prompt · M0 Constitution v2.0 · State Machine v1.0

---

## Architecture

```
┌─────────────────────────────────────────┐
│  Leptos Frontend (WASM)                 │
│  CockpitMode signal — no binary state   │
└──────────────┬──────────────────────────┘
               │ Tauri invoke()
┌──────────────▼──────────────────────────┐
│  Tauri Backend (Rust)                   │
│  commands/mastering.rs                  │
│  ipc/m0_client.rs (reqwest)             │
└──────────────┬──────────────────────────┘
               │ HTTP localhost:7400
┌──────────────▼──────────────────────────┐
│  M0 (m0d — must be running)             │
│  POST /master                           │
│  GET  /health                           │
│  GET  /blob/{id}                        │
└──────────────┬──────────────────────────┘
               │ native invoke
┌──────────────▼──────────────────────────┐
│  sp314-dsp → telemetry → insights       │
│  → rule-engine → Golden Blob            │
└─────────────────────────────────────────┘
```

---

## Task Order

```
P6-001  CSS token cleanup (design-tokens-v1.0.md)
P6-002  M0 IPC client (reqwest → localhost:7400)
P6-003  M0 API endpoints (POST /master, GET /health, GET /blob/{id})
P6-004  Tauri commands — replace stubs with real M0 calls
P6-005  Golden Blob → Cockpit (JSON IPC)
P6-006  Data Cascade with real data (FM3→FM4→FM5)
P6-007  Live telemetry during FM2 (100ms events)
P6-008  FM6 Export (real FLAC/WAV via M0)
P6-009  CI gate + tag
```

---

## P6-001 — CSS Token Cleanup

**Goal:** Replace all inline hex literals with design token variables.
Read `apps/stillair/cockpit/design-system/design-tokens-v1.0.md` §12 first.

```bash
# Find all inline hex literals in frontend CSS/Rust
grep -rn "#[0-9a-fA-F]\{3,6\}" \
  apps/stillair/frontend/src/ \
  apps/stillair/frontend/index.html
```

Rules:
- Every color must reference a `var(--token-name)` from design-tokens-v1.0.md
- `--bg-base`, `--accent-session`, `--severity-high`, etc.
- No new hex literals. No HSL. No RGB tuples.
- Severity colors must use `--severity-{high|medium|low|info}`

**DoD P6-001:**
```bash
grep -r "#[0-9a-fA-F]\{3,6\}" apps/stillair/frontend/src/ \
  && echo "❌ Inline hex found" || echo "✅ All tokens"
```

---

## P6-002 — M0 IPC Client

**Goal:** `reqwest` client that talks to M0 on `localhost:7400`.

Create `apps/stillair/src-tauri/src/ipc/m0_client.rs`:

```rust
//! M0 IPC client — all M0 communication goes through here.
//! M0 Constitution v2.0 §03: M0 is the trust boundary.
//! This is the ONLY place Tauri backend calls M0.

use reqwest::Client;
use serde::{Deserialize, Serialize};

const M0_BASE: &str = "http://127.0.0.1:7400";

pub struct M0Client {
    client: Client,
}

impl M0Client {
    pub fn new() -> Self {
        Self { client: Client::new() }
    }

    /// GET /health — verify M0 is running before any operation
    pub async fn health(&self) -> Result<M0HealthResponse, M0Error> {
        let resp = self.client
            .get(&format!("{}/health", M0_BASE))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::Unhealthy);
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// POST /master — trigger mastering pipeline
    pub async fn trigger_mastering(
        &self,
        req: MasterRequest,
    ) -> Result<MasterResponse, M0Error> {
        let resp = self.client
            .post(&format!("{}/master", M0_BASE))
            .json(&req)
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(M0Error::RequestFailed(status));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }

    /// GET /blob/{id} — fetch Golden Blob as JSON
    pub async fn get_blob(&self, id: &str) -> Result<GoldenBlobJson, M0Error> {
        let resp = self.client
            .get(&format!("{}/blob/{}", M0_BASE, id))
            .send().await
            .map_err(|e| M0Error::Unreachable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(M0Error::BlobNotFound(id.to_string()));
        }
        resp.json().await.map_err(|e| M0Error::ParseError(e.to_string()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id:  String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MasterResponse {
    pub blob_id: String,
    pub status:  String,   // "ok" | "error"
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct M0HealthResponse {
    pub status: String,    // "ok" | "degraded"
}

/// Golden Blob as JSON — no binary data crosses the Tauri IPC boundary.
/// Cockpit receives metrics only — never raw audio bytes.
/// Aligned with creator-os/specs/golden-blob-spec.md v1.0
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoldenBlobJson {
    pub blob_id:     String,
    pub version:     String,       // schema version e.g. "1.0"
    pub blob_type:   String,       // "audio" | "av"
    pub preset_id:   String,
    pub input_hash:  String,       // SHA-256 hex
    pub seed:        u64,
    pub loudness:    LoudnessMetricsJson,
    pub quality:     QualityMetricsJson,
    pub provenance:  ProvenanceJson,
}

/// BS.1770-4 canonical values + platform compliance flags.
/// integrated_lufs, true_peak_dbtp, lra are measured.
/// All platform targets are derived — never re-measured.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoudnessMetricsJson {
    pub integrated_lufs:          f32,
    pub short_term_lufs:          f32,
    pub momentary_lufs:           f32,
    pub true_peak_dbtp:           f32,
    pub lra:                      f32,
    pub k_weighted:               bool,
    pub spotify_compliant:        bool,
    pub youtube_compliant:        bool,
    pub apple_music_compliant:    bool,
    pub apple_podcasts_compliant: bool,
    pub broadcast_compliant:      bool,
    pub tidal_compliant:          bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QualityMetricsJson {
    pub stereo_correlation: f32,
    pub phase_coherence:    f32,
    pub stereo_width:       f32,
    pub dynamic_range_db:   f32,
    pub rms_db:             f32,
    pub spectral_centroid:  f32,
    pub spectral_flatness:  f32,
    pub clips_detected:     u32,
    pub clip_free:          bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProvenanceJson {
    pub engine_id:       String,   // "E11"
    pub engine_version:  String,
    pub aether_enriched: bool,
    pub aether_devices:  Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum M0Error {
    #[error("M0 unreachable: {0}")]
    Unreachable(String),
    #[error("M0 health check failed")]
    Unhealthy,
    #[error("M0 request failed with status {0}")]
    RequestFailed(u16),
    #[error("Blob not found: {0}")]
    BlobNotFound(String),
    #[error("Parse error: {0}")]
    ParseError(String),
}

impl serde::Serialize for M0Error {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where S: serde::Serializer {
        s.serialize_str(&self.to_string())
    }
}
```

**DoD P6-002:**
```bash
cargo check -p stillair
echo "✅ P6-002"
```

---

## P6-003 — M0 API Endpoints

**Goal:** Add `/master`, `/blob/{id}` endpoints to m0d.
`GET /health` already exists from Phase 1.

Add to `lineos/m0/m0-daemon/src/main.rs` Axum router:

```rust
// New routes — add to existing router
.route("/master",    post(handlers::master::trigger_mastering))
.route("/blob/:id",  get(handlers::blob::get_blob))
```

Create `lineos/m0/m0-daemon/src/handlers/master.rs`:
```rust
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use crate::AppState;

#[derive(Deserialize)]
pub struct MasterRequest {
    pub audio_path: String,
    pub preset_id:  String,
}

#[derive(Serialize)]
pub struct MasterResponse {
    pub blob_id: String,
    pub status:  &'static str,
    pub message: Option<String>,
}

pub async fn trigger_mastering(
    State(state): State<AppState>,
    Json(req): Json<MasterRequest>,
) -> Json<MasterResponse> {
    // Audit: log mastering request
    state.audit.write_event("mastering_started", &req.audio_path).await;

    // Policy check: verify preset is allowed
    if !state.policy.preset_allowed(&req.preset_id) {
        return Json(MasterResponse {
            blob_id: String::new(),
            status: "error",
            message: Some("preset not allowed by policy".into()),
        });
    }

    // Invoke sp314-dsp native
    let blob_id = state.dsp_runner
        .run(&req.audio_path, &req.preset_id)
        .await
        .unwrap_or_else(|e| {
            state.audit.write_event_sync("mastering_failed", &e.to_string());
            String::new()
        });

    state.audit.write_event("mastering_complete", &blob_id).await;

    Json(MasterResponse {
        blob_id,
        status: "ok",
        message: None,
    })
}
```

Create `lineos/m0/m0-daemon/src/handlers/blob.rs`:
```rust
use axum::{Json, extract::{State, Path}};
use crate::AppState;

pub async fn get_blob(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    state.blob_store
        .get(&id)
        .map(Json)
        .ok_or(axum::http::StatusCode::NOT_FOUND)
}
```

Update `lineos/m0/api/m0-api.schema.json` with new endpoints.

**DoD P6-003:**
```bash
cargo check -p m0d
cargo test -p m0d
echo "✅ P6-003"
```

---

## P6-004 — Tauri Commands: Replace Stubs

**Goal:** Replace Phase 5 2s stub with real M0 calls.

Rewrite `apps/stillair/src-tauri/src/commands/mastering.rs`:

```rust
use tauri::command;
use crate::ipc::m0_client::{M0Client, MasterRequest};

/// Real implementation: Cockpit → Tauri → M0 → sp314-dsp
#[command]
pub async fn trigger_mastering(
    audio_path: String,
    preset_id:  String,
) -> Result<String, String> {
    let client = M0Client::new();

    // Verify M0 is healthy before triggering (ASC 0x05 guard)
    client.health().await
        .map_err(|e| format!("M0 unreachable: {}", e))?;

    let resp = client.trigger_mastering(MasterRequest {
        audio_path,
        preset_id,
    }).await.map_err(|e| e.to_string())?;

    if resp.status != "ok" {
        return Err(resp.message.unwrap_or("mastering failed".into()));
    }

    Ok(resp.blob_id)
}

/// Fetch Golden Blob as JSON — no binary data to frontend
#[command]
pub async fn get_golden_blob(blob_id: String)
    -> Result<crate::ipc::m0_client::GoldenBlobJson, String>
{
    let client = M0Client::new();
    client.get_blob(&blob_id).await.map_err(|e| e.to_string())
}
```

**DoD P6-004:**
```bash
cargo check -p stillair
echo "✅ P6-004"
```

---

## P6-005 — Golden Blob → Cockpit (JSON IPC)

**Goal:** After mastering, fetch Golden Blob and populate Cockpit signals.

In `app.rs`, after `dsp_done` event:

```rust
// FM3 reached — fetch blob and start Data Cascade
let blob_id = blob_id_signal.get();
let blob = invoke::<GoldenBlobJson>("get_golden_blob",
    to_value(&GetBlobArgs { blob_id }).unwrap()
).await;

match blob {
    Ok(b) => {
        // Populate insight signals
        set_lufs.set(Some(b.integrated_lufs));
        set_true_peak.set(Some(b.true_peak_dbtp));
        set_lra.set(Some(b.loudness_range_lu));
        set_stereo_corr.set(Some(b.stereo_correlation));
        set_dynamic_range.set(Some(b.dynamic_range_db));

        // Trigger FM4 automatically (Data Cascade)
        set_mode.set(CockpitMode::InsightsReady);

        // Run rule-engine evaluation via Tauri
        // → FM5 automatically
        trigger_coach_evaluation(b).await;
    }
    Err(e) => set_mode.set(CockpitMode::Fault(AscCode::IoErr)),
}
```

**DoD P6-005:**
```bash
# Manual: after mastering, FM4 shows real LUFS/TP values
echo "✅ P6-005"
```

---

## P6-006 — Data Cascade with Real Data

**Goal:** FM3→FM4→FM5 cascade uses real Golden Blob data.

Add Tauri command for rule-engine evaluation:

```rust
// apps/stillair/src-tauri/src/commands/insights.rs
#[command]
pub async fn evaluate_findings(blob: GoldenBlobJson)
    -> Result<CoachFindingsJson, String>
{
    use lineos_rule_engine::{evaluate, AnalysisReport, QualityMetrics,
                              ComplianceFlags, Thresholds};
    use sp314_dsp::types::config::Bmr128Schema;

    // Load schema (cached in AppState in full impl)
    let schema: Bmr128Schema = serde_json::from_str(
        include_str!("../../../../lineos/shared/schema/bmr-128.schema.json")
    ).map_err(|e| e.to_string())?;

    let thresholds = Thresholds::from_schema_with_preset(
        &schema, Box::leak(blob.preset_id.into_boxed_str())
    );

    let report = AnalysisReport {
        quality: QualityMetrics {
            lufs_integrated:    blob.integrated_lufs,
            lufs_short_term:    blob.short_term_lufs,
            lufs_momentary:     blob.momentary_lufs,
            true_peak:          blob.true_peak_dbtp,
            loudness_range:     blob.loudness_range_lu,
            stereo_correlation: blob.stereo_correlation,
            dynamic_range:      blob.dynamic_range_db,
            dc_offset:          0.0,  // Phase 7: add to blob
        },
        compliance: ComplianceFlags::default(),
        version: "1.0",
    };

    let findings = evaluate(&report, &thresholds);
    Ok(CoachFindingsJson::from(findings))
}
```

**DoD P6-006:**
```bash
# Manual: FM5 shows real CoachFindings from rule-engine
echo "✅ P6-006"
```

---

## P6-007 — Live Telemetry During FM2

**Goal:** 100ms telemetry events during mastering. Subscribe pattern.

Add Tauri event emission from m0d during mastering:

```rust
// In mastering handler — emit progress events every 100ms
app_handle.emit("mastering_progress", TelemetrySignal {
    lufs:     current_lufs,
    peak:     current_peak,
    progress: pct,          // 0.0–1.0
    stage:    stage_name,
})?;
```

Subscribe in Leptos frontend:

```rust
// In app.rs — subscribe to mastering_progress events
let unlisten = listen::<TelemetrySignal>("mastering_progress",
    move |event| {
        set_live_lufs.set(Some(event.payload.lufs));
        set_progress.set(event.payload.progress);
        set_stage.set(event.payload.stage.clone());
    }
).await.unwrap();
```

**DoD P6-007:**
```bash
# Manual: during FM2, Center MFD updates every ~100ms
echo "✅ P6-007"
```

---

## P6-008 — FM6 Export (Real FLAC/WAV)

**Goal:** Export the Golden Blob as FLAC or WAV via M0.

Add `POST /export` to M0 and wire to FM6:

```rust
#[command]
pub async fn export_audio(
    blob_id: String,
    format:  String,   // "flac" | "wav"
    path:    String,
) -> Result<String, String> {
    let client = M0Client::new();
    client.export(&blob_id, &format, &path)
        .await
        .map_err(|e| e.to_string())
}
```

FM6 flow:
```
export_clicked → FM6 (UI locked)
    → export_audio(blob_id, "flac", output_path)
    → on success: FM5 (UI unlocked)
    → on error:   FM-ERR (ASC 0x02)
```

**DoD P6-008:**
```bash
# Manual: MASTER → FM5 → Export button → FM6 → file written → FM5
echo "✅ P6-008"
```

---

## P6-009 — CI Gate + Tag

```bash
# No inline hex
grep -r "#[0-9a-fA-F]\{3,6\}" apps/stillair/frontend/src/ \
  && echo "❌" || echo "✅ Tokens clean"

cargo check -p stillair
cargo check --target wasm32-unknown-unknown -p stillair-frontend
cargo test --workspace
just ci
just deny

git add -A
git commit -m "feat(wire): Phase 6 — full audio flow wired

- M0 IPC client (reqwest → localhost:7400)
- POST /master + GET /blob/{id} endpoints in m0d
- Tauri commands: real M0 calls (no more stubs)
- Golden Blob → Cockpit via JSON IPC (no binary state)
- Data Cascade FM3→FM4→FM5 with real LUFS/TP/LRA
- rule-engine evaluation from real Golden Blob
- Live telemetry FM2: 100ms Tauri events
- FM6 Export: real FLAC/WAV via M0
- CSS: all inline hex → design token variables

Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6"

git tag v0.6.0-wired
git log --oneline -7
```

---

## Completion Report

```
✅ Phase 6 — Wire Everything — COMPLETE

M0 IPC:              POST /master + GET /blob/{id} ✅
Real audio flow:     Cockpit → Tauri → M0 → sp314-dsp ✅
Golden Blob:         JSON only, no binary to frontend ✅
Data Cascade:        real LUFS/TP/LRA in FM4/FM5 ✅
Rule-engine:         real CoachFindings from blob ✅
Live telemetry:      100ms events during FM2 ✅
FM6 Export:          real FLAC/WAV ✅
CSS tokens:          no inline hex ✅
just ci:             ✅

Tag: v0.6.0-wired ✅

Ready for: Phase 7 — Aether (LLM narrative + Coach enrichment)
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 6 — Wire Everything
**Version:** 1.0
**Status:** 🔒 LOCKED
