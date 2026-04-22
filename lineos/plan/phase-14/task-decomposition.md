# LineOS — Phase 14 Task Decomposition

**Document:** `lineos/plan/phase-14/task-decomposition.md`
**Version:** 1.0
**Phase:** 14 — UI Polish (Avionics Aesthetic)
**Status:** 🔒 LOCKED

---

## Task Order

```
P14-001  getVisualizationData Tauri command (backend)
P14-002  Panel container + screws (CSS + Dioxus)
P14-003  CockpitState v2 (add viz signal)
P14-004  VU Meters (segmented 30-LED)
P14-005  SpectrumDisplay (SVG path from viz)
P14-006  StereoScope (dual ellipse Lissajous)
P14-007  MetricsReadout (PEAK/RANGE/CORR)
P14-008  CoachPanel (progress bars + severity + YES/NO)
P14-009  TransportBar (hardware buttons, pointer-events fix)
P14-010  InsightsPanel assembly (2×2 OLED grid)
P14-011  MasteredView (BEFORE/AFTER + Quality Gate + Compliance)
P14-012  CockpitLayout final assembly + CSS variables cleanup
P14-013  CI gate + tag
```

---

## P14-001 — getVisualizationData (Backend)

**This is the only backend task in Phase 14.**
All other tasks are pure UI.

Create `apps/stillair/src-tauri/src/commands/visualization.rs`:

```rust
use crate::ipc::m0_client::M0Client;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct VisualizationDataJson {
    pub spectrum_svg_path:   String,
    pub lissajous_outer_rx:  f32,
    pub lissajous_outer_ry:  f32,
    pub lissajous_inner_rx:  f32,
    pub lissajous_inner_ry:  f32,
    pub waveform_before_svg: String,
    pub waveform_after_svg:  String,
}

#[tauri::command]
pub async fn get_visualization_data(
    blob_id: String,
) -> Result<VisualizationDataJson, String> {
    let client = M0Client::new();
    let blob = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // Spectrum SVG path — gaussian curve from spectral features
    let spectrum_path = compute_spectrum_path(
        blob.quality.spectral_centroid,
        blob.quality.spectral_flatness,
        blob.quality.spectral_flux,
    );

    // Lissajous ellipse parameters
    let outer_rx = (blob.quality.stereo_width * 45.0 + 5.0).clamp(5.0, 50.0);
    let outer_ry = 50.0f32;
    let inner_rx = (blob.quality.stereo_correlation.abs() * 30.0 + 5.0).clamp(3.0, 35.0);
    let inner_ry = ((1.0 - blob.quality.stereo_correlation.abs()) * 25.0 + 3.0).clamp(3.0, 28.0);

    // Waveform placeholders — Phase 15 will use real PCM snapshots
    let before_path = compute_waveform_placeholder(blob.quality.rms_db, false);
    let after_path  = compute_waveform_placeholder(blob.loudness.integrated_lufs, true);

    Ok(VisualizationDataJson {
        spectrum_svg_path:   spectrum_path,
        lissajous_outer_rx:  outer_rx,
        lissajous_outer_ry:  outer_ry,
        lissajous_inner_rx:  inner_rx,
        lissajous_inner_ry:  inner_ry,
        waveform_before_svg: before_path,
        waveform_after_svg:  after_path,
    })
}

/// Gaussian curve approximation from spectral features.
/// All math uses libm — no std::f32 methods.
fn compute_spectrum_path(centroid: f32, flatness: f32, flux: f32) -> String {
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(64);
    let center_x = (centroid / 20000.0 * 380.0 + 10.0).clamp(10.0, 390.0);

    for i in 0..64usize {
        let x = i as f32 * 400.0 / 63.0;
        let distance = (x - center_x) / (flatness * 80.0 + 40.0);
        let base = libm::expf(-distance * distance * 0.5);
        // Add flux-derived high-frequency detail
        let detail = flux * 0.3 * libm::sinf(x * 0.3) * libm::expf(-x / 200.0);
        let y = 180.0 - (base + detail).clamp(0.0, 1.0) * 160.0;
        points.push((x, y));
    }

    // Build SVG path + fill closure
    let mut path = format!("M {:.1},{:.1}", points[0].0, points[0].1);
    for (x, y) in &points[1..] {
        path.push_str(&format!(" L {:.1},{:.1}", x, y));
    }
    // Close fill path along bottom
    path.push_str(&format!(" L 390,190 L 10,190 Z"));
    path
}

fn compute_waveform_placeholder(lufs: f32, is_mastered: bool) -> String {
    let amplitude = ((-lufs / 30.0) * 50.0).clamp(5.0, 55.0);
    let freq = if is_mastered { 0.04 } else { 0.03 };
    let mut path = String::from("M 0,60");
    for i in 1..=800usize {
        let x = i as f32;
        let envelope = libm::sinf(x * 0.002) * 0.5 + 0.5;
        let y = 60.0 - libm::sinf(x * freq) * amplitude * envelope;
        path.push_str(&format!(" L {:.0},{:.1}", x, y));
    }
    path
}
```

Register in `lib.rs`:
```rust
commands::visualization::get_visualization_data,
```

**DoD P14-001:**
```bash
cargo check -p stillair
echo "✅ P14-001"
```

---

## P14-002 — Panel Container + Screws

Per UI Agent Context §4.4 + §4.5.

Key requirements:
- `isolation: isolate` on every panel (prevents stacking bleed)
- All screw `::after` must have `pointer-events: none`
- Insights variant: cyan glow in box-shadow
- `mfd-bay` container: `isolation: isolate`

**DoD P14-002:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
# Visual: panels have depth, screws visible at corners
echo "✅ P14-002"
```

---

## P14-003 — CockpitState v2

Add `viz: Signal<Option<VisualizationDataJson>>` to CockpitState.

Fetch viz immediately after session in the mastering completion handler:
```rust
let viz = invoke::<VisualizationDataJson, _>(
    "getVisualizationData",
    json!({ "blobId": blob_id })
).await.ok();
state.viz.set(viz);
```

**DoD P14-003:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-003"
```

---

## P14-004 — VU Meters

Per UI Agent Context §4.6. Prompt P3.

30 segments. LUFS cyan. PEAK amber. Top 3 red.
Scale labels as separate column.

**DoD P14-004:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
# Visual: segmented meters match reference closeup image
echo "✅ P14-004"
```

---

## P14-005 — SpectrumDisplay

Per UI Agent Context §4.7. Prompt P5.

Receives `viz.spectrum_svg_path` — renders only.
No computation in component.

**DoD P14-005:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-005"
```

---

## P14-006 — StereoScope

Per UI Agent Context §4.8. Prompt P6.

Receives 4 f32 values from viz signal.
Outer cyan ellipse + inner magenta ellipse + polar grid + center glow.

**DoD P14-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-006"
```

---

## P14-007 — MetricsReadout

Per UI Agent Context §4.9. Prompt P7.

**DoD P14-007:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-007"
```

---

## P14-008 — CoachPanel

Per UI Agent Context §4.10. Prompt P8.

**DoD P14-008:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-008"
```

---

## P14-009 — TransportBar

Per UI Agent Context §4.11. Prompt P9.

**Known fixes required (from Phase 12B):**
- `transport-bar::before/::after` → `pointer-events: none`
- `mfd-bay` → `isolation: isolate`
- All buttons → `position: relative; z-index: 1; pointer-events: all`

**DoD P14-009:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
# Manual: all transport buttons clickable
echo "✅ P14-009"
```

---

## P14-010 — InsightsPanel Assembly

Per UI Agent Context. Prompt P10.

2×2 OLED grid:
```
┌────────────────────┬──────────────┐
│  SpectrumDisplay   │  VuMeterPair │
├────────────────────┼──────────────┤
│  StereoScope       │ MetricsRead  │
└────────────────────┴──────────────┘
```

**DoD P14-010:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-010"
```

---

## P14-011 — MasteredView

Per UI Agent Context §5. Prompt P11.

Phase 14: waveform paths are placeholders from `viz.waveform_*_svg`.
Phase 15: replaced with real PCM snapshots.

**DoD P14-011:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-011"
```

---

## P14-012 — CockpitLayout + CSS Cleanup

Per UI Agent Context §4.13. Prompt P12.

- Remove all inline hex colors → CSS variables
- Ensure no `std::f32` methods in any Rust UI code
- `ui-isolation-check.sh` must pass

**DoD P14-012:**
```bash
bash infra/ci/checks/ui-isolation-check.sh
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P14-012"
```

---

## P14-013 — CI Gate + Tag

```bash
cargo test --workspace
just ci
bash infra/ci/checks/ui-isolation-check.sh

git add -A
git commit -m "feat(ui): Phase 14 — Avionics aesthetic UI polish

P14-001: getVisualizationData Tauri command
  - Spectrum SVG path (gaussian from spectral features, libm)
  - Lissajous ellipse parameters (from correlation + width)
  - Waveform placeholder paths (Phase 15: real PCM snapshots)

P14-002: Panel container (OLED inset + screws + shadow stack)
P14-003: CockpitState v2 (viz signal added)
P14-004: VU Meters (30 segments, LUFS cyan + PEAK amber)
P14-005: SpectrumDisplay (SVG path from backend — no UI computation)
P14-006: StereoScope (dual ellipse Lissajous — static Phase 14)
P14-007: MetricsReadout (PEAK/RANGE/CORR correct colors)
P14-008: CoachPanel (progress bars + severity dots + YES/NO)
P14-009: TransportBar (hardware buttons, pointer-events fixed)
P14-010: InsightsPanel (2×2 OLED grid assembly)
P14-011: MasteredView (BEFORE/AFTER + Quality Gate + Compliance)
P14-012: CockpitLayout + full CSS variable cleanup

UI Agent Context v2.1 enforced:
  - No computation in UI components ✅
  - SVG paths precomputed in backend ✅
  - CSS variables only, no inline hex ✅
  - ui-isolation-check.sh: clean ✅
  - All transport buttons clickable ✅

Authority: UI Agent Context v2.1 · Amendment A-002 · Phase 14"

git tag v0.14.0-ui
git log --oneline -5
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 14
**Version:** 1.0
**Status:** 🔒 LOCKED
