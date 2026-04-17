# LineOS — Phase 13 Task Decomposition

**Document:** `lineos/plan/phase-13/task-decomposition.md`
**Version:** 1.0
**Phase:** 13 — MP3 Export + BMR-128 PDF Report
**Status:** 🔒 LOCKED

---

## Task Order

```
P13-001  Install libmp3lame-dev + LGPL notice doc
P13-002  Add mp3lame-encoder to m0d Cargo.toml
P13-003  Mp3ExportProvider in handlers/export.rs
P13-004  Add MP3 to export format selector (Dioxus)
P13-005  BMR-128 PDF report generator
P13-006  PDF export Tauri command + button
P13-007  CI gate + tag
```

---

## P13-001 — Install libmp3lame-dev + LGPL Notice

```bash
sudo apt install -y libmp3lame-dev
pkg-config --libs mp3lame
```

Read `docs/licenses/LAME-LGPL-NOTICE.md` before proceeding.
This documents the LGPL dynamic linking requirement.

**DoD P13-001:**
```bash
pkg-config --libs mp3lame && echo "✅ P13-001"
```

---

## P13-002 — Add mp3lame-encoder to m0d

```toml
# lineos/m0/m0-daemon/Cargo.toml
[dependencies]
mp3lame-encoder = "0.1"   # LGPL — dynamic linking only (see LAME-LGPL-NOTICE.md)
```

**DoD P13-002:**
```bash
cargo check -p m0d
echo "✅ P13-002"
```

---

## P13-003 — Mp3ExportProvider

Add to `handlers/export.rs`:

```rust
/// MP3 export via LAME (LGPL — dynamic linking only).
/// See docs/licenses/LAME-LGPL-NOTICE.md
fn export_mp3(blob: &StoredBlob, path: &Path) -> Result<(), String> {
    use mp3lame_encoder::{Builder, FlushNoGap, Id3Tag, MonoPcm, DualPcm};

    let pcm = pcm_bytes_to_f32(&blob.audio_bytes)?;

    let mut builder = Builder::new()
        .map_err(|e| format!("LAME init failed: {e:?}"))?;

    builder.set_num_channels(blob.channels as u8)
        .map_err(|e| format!("LAME channels: {e:?}"))?;
    builder.set_sample_rate(blob.sample_rate)
        .map_err(|e| format!("LAME sample rate: {e:?}"))?;
    builder.set_quality(2)   // 0=best, 9=worst — use 2 for mastering grade
        .map_err(|e| format!("LAME quality: {e:?}"))?;

    let mut encoder = builder.build()
        .map_err(|e| format!("LAME build failed: {e:?}"))?;

    // Split interleaved stereo into L/R channels
    let (left, right): (Vec<f32>, Vec<f32>) = pcm.chunks(2)
        .map(|f| (f[0], f.get(1).copied().unwrap_or(f[0])))
        .unzip();

    let input = DualPcm { left: &left, right: &right };
    let mut mp3_out = Vec::new();

    let encoded = encoder.encode(input)
        .map_err(|e| format!("LAME encode failed: {e:?}"))?;
    mp3_out.extend_from_slice(&encoded);

    let flushed = encoder.flush::<FlushNoGap>()
        .map_err(|e| format!("LAME flush failed: {e:?}"))?;
    mp3_out.extend_from_slice(&flushed);

    std::fs::write(path, &mp3_out)
        .map_err(|e| format!("MP3 write failed: {e}"))
}
```

Add `"mp3"` to `ExportFormat`:
```rust
pub enum ExportFormat {
    Wav, Flac, Opus, Mp3,  // ← add Mp3
}
```

**DoD P13-003:**
```bash
cargo check -p m0d
echo "✅ P13-003"
```

---

## P13-004 — MP3 Button in Dioxus Export Selector

In `panels/session.rs` ExportControls, add MP3 button:

```rust
button {
    class: if *export_format.read() == "mp3" {
        "export-format-btn active"
    } else { "export-format-btn" },
    onclick: move |_| export_format.set("mp3".to_string()),
    "MP3"
}
```

**DoD P13-004:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P13-004"
```

---

## P13-005 — BMR-128 PDF Report

Use `printpdf` crate (pure Rust, MIT) for PDF generation.

```toml
# apps/stillair/src-tauri/Cargo.toml
printpdf = "0.7"
```

```rust
// apps/stillair/src-tauri/src/commands/report.rs

use printpdf::*;

#[tauri::command]
pub async fn export_pdf_report(
    blob_id: String,
    app:     tauri::AppHandle,
) -> Result<String, String> {
    let client = M0Client::new();
    let blob = client.get_blob(&blob_id).await
        .map_err(|e| format!("IO_ERR:0x02:{e}"))?;

    // Native save dialog
    let path = tokio::task::spawn_blocking({
        let app = app.clone();
        move || {
            app.dialog()
                .file()
                .set_file_name("mastered-bmr128.pdf")
                .add_filter("PDF", &["pdf"])
                .blocking_save_file()
        }
    }).await.map_err(|e| e.to_string())?;

    let output_path = match path {
        Some(p) => p.to_string_lossy().to_string(),
        None    => return Err("Cancelled".into()),
    };

    generate_bmr128_pdf(&blob, &output_path)?;
    Ok(output_path)
}

fn generate_bmr128_pdf(blob: &GoldenBlobJson, path: &str) -> Result<(), String> {
    let (doc, page1, layer1) = PdfDocument::new(
        "BMR-128 Compliance Report", Mm(210.0), Mm(297.0), "Layer 1"
    );

    let page = doc.get_page(page1);
    let layer = page.get_layer(layer1);

    let font = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| e.to_string())?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| e.to_string())?;

    // Title
    layer.use_text("BMR-128 COMPLIANCE REPORT", 18.0, Mm(20.0), Mm(277.0), &font_bold);
    layer.use_text("Still Air — Creator OS", 10.0, Mm(20.0), Mm(269.0), &font);
    layer.use_text(
        &format!("Blob ID: {}", blob.blob_id),
        8.0, Mm(20.0), Mm(263.0), &font
    );
    layer.use_text(
        &format!("Preset: {}", blob.preset_id),
        8.0, Mm(20.0), Mm(258.0), &font
    );

    // Loudness section
    layer.use_text("LOUDNESS METRICS", 12.0, Mm(20.0), Mm(248.0), &font_bold);
    let metrics = [
        ("Integrated LUFS",  format!("{:.1} LUFS", blob.loudness.integrated_lufs)),
        ("True Peak",        format!("{:.1} dBTP", blob.loudness.true_peak_dbtp)),
        ("Loudness Range",   format!("{:.1} LU",   blob.loudness.lra)),
        ("Short Term LUFS",  format!("{:.1} LUFS", blob.loudness.short_term_lufs)),
        ("Momentary LUFS",   format!("{:.1} LUFS", blob.loudness.momentary_lufs)),
    ];
    for (i, (label, value)) in metrics.iter().enumerate() {
        let y = Mm(240.0 - i as f32 * 8.0);
        layer.use_text(label, 10.0, Mm(25.0), y, &font);
        layer.use_text(value, 10.0, Mm(120.0), y, &font_bold);
    }

    // Compliance section
    layer.use_text("PLATFORM COMPLIANCE", 12.0, Mm(20.0), Mm(192.0), &font_bold);
    let compliance = [
        ("Spotify (-14 LUFS)",       blob.loudness.spotify_compliant),
        ("YouTube (-14 LUFS)",       blob.loudness.youtube_compliant),
        ("Apple Music (-16 LUFS)",   blob.loudness.apple_music_compliant),
        ("Apple Podcasts (-16 LUFS)",blob.loudness.apple_podcasts_compliant),
        ("Broadcast (-23 LUFS)",     blob.loudness.broadcast_compliant),
        ("Tidal (-14 LUFS)",         blob.loudness.tidal_compliant),
    ];
    for (i, (platform, compliant)) in compliance.iter().enumerate() {
        let y = Mm(184.0 - i as f32 * 8.0);
        layer.use_text(platform, 10.0, Mm(25.0), y, &font);
        layer.use_text(
            if *compliant { "✓ PASS" } else { "✗ FAIL" },
            10.0, Mm(120.0), y, &font_bold
        );
    }

    // Quality section
    layer.use_text("QUALITY METRICS", 12.0, Mm(20.0), Mm(134.0), &font_bold);
    let quality = [
        ("Stereo Correlation", format!("{:.2}", blob.quality.stereo_correlation)),
        ("Dynamic Range",      format!("{:.1} dB", blob.quality.dynamic_range_db)),
        ("RMS Level",          format!("{:.1} dB", blob.quality.rms_db)),
        ("Clip Free",          if blob.quality.clip_free { "YES".into() } else { "NO".into() }),
    ];
    for (i, (label, value)) in quality.iter().enumerate() {
        let y = Mm(126.0 - i as f32 * 8.0);
        layer.use_text(label, 10.0, Mm(25.0), y, &font);
        layer.use_text(&value, 10.0, Mm(120.0), y, &font_bold);
    }

    // Footer
    layer.use_text(
        "Generated by Still Air — Creator OS | LineOS v1.0",
        8.0, Mm(20.0), Mm(15.0), &font
    );

    doc.save(&mut std::io::BufWriter::new(
        std::fs::File::create(path)
            .map_err(|e| format!("PDF create failed: {e}"))?
    )).map_err(|e| format!("PDF save failed: {e}"))
}
```

**DoD P13-005:**
```bash
cargo check -p stillair
echo "✅ P13-005"
```

---

## P13-006 — PDF Button in Dioxus

Add PDF report button to ExportControls in `panels/session.rs`:

```rust
button {
    class: "export-format-btn",
    onclick: move |_| {
        spawn_local(async move {
            match invoke::<String, _>(
                "exportPdfReport",
                serde_json::json!({ "blobId": blob_id })
            ).await {
                Ok(path) => web_sys::console::log_1(
                    &format!("[PDF] saved: {path}").into()
                ),
                Err(e) => web_sys::console::log_1(
                    &format!("[PDF] error: {e}").into()
                ),
            }
        });
    },
    "PDF REPORT"
}
```

**DoD P13-006:**
```bash
cargo check --target wasm32-unknown-unknown -p stillair-cockpit
echo "✅ P13-006"
```

---

## P13-007 — CI Gate + Tag

```bash
# Verify LAME dynamic linking
ldd target/debug/m0d | grep -i "mp3\|lame" && echo "✅ LAME dynamic"

cargo test --workspace
just ci

git add -A
git commit -m "feat(export): Phase 13 — MP3 (LAME) + BMR-128 PDF report

13A — MP3 Export:
  - mp3lame-encoder crate (LGPL — dynamic linking only)
  - Quality preset 2 (mastering grade)
  - DualPcm stereo encoding
  - docs/licenses/LAME-LGPL-NOTICE.md committed

13B — BMR-128 PDF Report:
  - printpdf (pure Rust, MIT)
  - Loudness metrics, platform compliance, quality metrics
  - Native save dialog via tauri-plugin-dialog
  - Still Air branding + blob ID provenance

Authority: LineOS Constitution v2.0 · LAME-LGPL-NOTICE.md"

git tag v0.13.0-mp3
```

---

**Lead Architect:** Anestis
**System:** LineOS — Still Air (A1)
**Phase:** 13
**Version:** 1.0
**Status:** 🔒 LOCKED


---