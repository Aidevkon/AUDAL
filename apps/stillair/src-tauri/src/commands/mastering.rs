//! Mastering commands — real M0 IPC calls replacing Phase 5 stubs.
//! Authority: Phase 6 task-decomposition P6-004
//! Flow: Cockpit → Tauri → M0 → sp314-dsp → Golden Blob
//!
//! FORBIDDEN: Calling sp314-dsp directly (must go through M0).
//! FORBIDDEN: Audio processing in this module.
//! FORBIDDEN: serde_json::Value in return types.

use crate::ipc::m0_client::{GoldenBlobJson, M0Client, MasterRequest};
use tauri::{command, AppHandle, Emitter, Runtime};

// ── Accepted audio extensions ─────────────────────────────────────────────────
//
// ASC 0x04 (ValidationFail) is returned if the selected file has a
// non-audio extension (e.g. .txt, .pdf). The list is intentionally narrow —
// only formats sp314-dsp can process.
const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "aiff", "aif", "mp3"];

// ── open_audio_file ───────────────────────────────────────────────────────────

/// Open native file picker filtered to audio formats, then load metadata.
///
/// Return contract (fully typed — no serde_json::Value):
///   Ok(Some(AudioMeta))  → file selected and valid → Cockpit transitions FM1
///   Ok(None)             → dialog cancelled         → Cockpit stays in current mode
///   Err("ASC:0x04:…")   → file selected but invalid extension → FM-ERR ASC 0x04
///
/// The Err string prefix "ASC:0x04:" is parsed by the frontend to set the
/// correct fault code.
#[command]
pub async fn open_audio_file<R: Runtime>(app: AppHandle<R>) -> Result<Option<AudioMeta>, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};

    // Block the dialog call on a dedicated thread — blocking_pick_file()
    // must not run on the async executor.
    let maybe_path: Option<FilePath> = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Audio", AUDIO_EXTENSIONS)
            .add_filter("WAV", &["wav"])
            .add_filter("FLAC", &["flac"])
            .add_filter("AIFF", &["aiff", "aif"])
            .add_filter("MP3", &["mp3"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("Dialog task error: {e}"))?;

    let file_path = match maybe_path {
        None => return Ok(None), // User cancelled → stay in current mode
        Some(fp) => fp,
    };

    // Extract path string
    let path_str = match file_path {
        FilePath::Path(p) => p.to_string_lossy().to_string(),
        FilePath::Url(u) => u.to_string(),
    };

    // Validate extension — ASC 0x04 (ValidationFail) on mismatch
    let ext = std::path::Path::new(&path_str)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!(
            "ASC:0x04:Unsupported file type '.{ext}'. \
             Accepted: WAV, FLAC, AIFF, MP3."
        ));
    }

    // Derive display format from extension
    let format = match ext.as_str() {
        "wav" => "WAV",
        "flac" => "FLAC",
        "aiff" | "aif" => "AIFF",
        "mp3" => "MP3",
        _ => "AUDIO",
    };

    let name = std::path::Path::new(&path_str)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Phase 6: metadata stub — Phase 7 reads real headers via hound/symphonia
    Ok(Some(AudioMeta {
        path: path_str,
        name,
        format: format.to_string(),
        sample_rate: 48_000,
        bit_depth: 24,
        duration_s: 0.0, // Phase 7: real decode
        channels: 2,
    }))
}

// ── load_audio_file ───────────────────────────────────────────────────────────

/// Load audio file metadata by path — called on FM0 → FM1 (drag-and-drop).
/// Phase 6 stub: Phase 7 wires real M0 header decode.
#[command]
pub async fn load_audio_file(path: String) -> Result<AudioMeta, String> {
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        return Err(format!("ASC:0x04:Unsupported file type '.{ext}'."));
    }

    let format = match ext.as_str() {
        "wav" => "WAV",
        "flac" => "FLAC",
        "aiff" | "aif" => "AIFF",
        "mp3" => "MP3",
        _ => "AUDIO",
    };

    let name = path.split('/').next_back().unwrap_or("unknown").to_string();

    Ok(AudioMeta {
        path,
        name,
        format: format.to_string(),
        sample_rate: 48_000,
        bit_depth: 24,
        duration_s: 0.0,
        channels: 2,
    })
}

// ── trigger_mastering ─────────────────────────────────────────────────────────

/// Real mastering: Cockpit → Tauri → M0 → sp314-dsp.
/// Returns blob_id on success; ASC-mapped error string on failure.
#[tauri::command]
pub async fn trigger_mastering(
    audio_path: String,
    preset_id: String,
    flavour_id: String,
    intent_tone: f32,
    intent_dynamics: f32,
    client: tauri::State<'_, M0Client>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    client
        .health()
        .await
        .map_err(|e| format!("M0 unreachable: {e}"))?;

    let resp = client
        .trigger_mastering(MasterRequest {
            audio_path,
            preset_id,
            flavour_id,
            intent_tone,
            intent_dynamics,
        })
        .await
        .map_err(|e| e.to_string())?;

    // Fallback: if old daemon returns blob_id directly
    let job_id = match resp.job_id {
        Some(id) => id,
        None => {
            return resp
                .blob_id
                .ok_or("No job_id or blob_id from daemon".into())
        }
    };

    let _ = app.emit(
        "mastering://progress",
        serde_json::json!({"stage": "ANALYZING", "job_id": &job_id}),
    );

    let mut last_stage = String::new();
    let url = format!("http://127.0.0.1:7402/progress/{job_id}/stream");
    eprintln!("[SSE-OPEN] subscribing for job {}", job_id);
    let mut es = reqwest_eventsource::EventSource::get(url);

    use futures_util::StreamExt;
    while let Some(event) = es.next().await {
        match event {
            Ok(reqwest_eventsource::Event::Open) => continue,
            Ok(reqwest_eventsource::Event::Message(message)) => {
                #[derive(serde::Deserialize)]
                struct SseProgress {
                    #[serde(rename = "jobId", alias = "job_id", default)]
                    job_id: String,
                    stage: String,
                    #[serde(rename = "blobId", alias = "blob_id")]
                    blob_id: Option<String>,
                    error: Option<String>,
                }

                let progress: SseProgress = match serde_json::from_str(&message.data) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("[SSE-ERROR] parse fail: {}", e);
                        continue;
                    }
                };

                eprintln!("[SSE-TRAP] stage={} (emitting={})", progress.stage, progress.stage != last_stage);

                if progress.stage != last_stage {
                    last_stage = progress.stage.clone();
                    let _ = app.emit(
                        "mastering://progress",
                        serde_json::json!({
                            "stage":      progress.stage,
                            "job_id":     progress.job_id,
                            "elapsed_ms": 0,
                            "blob_id":    progress.blob_id,
                        }),
                    );
                }

                match progress.stage.as_str() {
                    "CERTIFIED" => {
                        eprintln!("[SSE-CLOSE] stream closed for job {}", job_id);
                        es.close();
                        return progress.blob_id.ok_or("CERTIFIED but no blob_id".into());
                    }
                    "ERROR" => {
                        eprintln!("[SSE-CLOSE] stream closed with error for job {}", job_id);
                        es.close();
                        return Err(progress.error.unwrap_or("Mastering failed".into()));
                    }
                    _ => continue,
                }
            }
            Err(e) => {
                eprintln!("[SSE-ERROR] stream error: {}", e);
                eprintln!("[SSE-CLOSE] stream disconnected prematurely for job {}", job_id);
                es.close();
                return Err("SSE Stream disconnected prematurely".into());
            }
        }
    }

    eprintln!("[SSE-CLOSE] stream ended unexpectedly for job {}", job_id);
    Err("Mastering stream ended unexpectedly".into())
}

// ── get_golden_blob ───────────────────────────────────────────────────────────

/// Fetch Golden Blob as JSON — called on FM2 → FM3 to start Data Cascade.
/// No binary data crosses the IPC boundary.
#[command]
pub async fn get_golden_blob(
    blob_id: String,
    client: tauri::State<'_, M0Client>,
) -> Result<GoldenBlobJson, String> {
    client.get_blob(&blob_id).await.map_err(|e| e.to_string())
}

// ── AudioMeta ─────────────────────────────────────────────────────────────────

/// Audio file metadata returned to frontend.
/// `path` carries the full filesystem path for downstream M0 calls.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioMeta {
    /// Full filesystem path — used by trigger_mastering.
    pub path: String,
    /// Display filename.
    pub name: String,
    pub format: String,
    pub sample_rate: u32,
    pub bit_depth: u8,
    pub duration_s: f32,
    pub channels: u8,
}

// ── export_certificate_png ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn export_certificate_png(
    blob_id: String,
    client: tauri::State<'_, M0Client>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    // Get blob data
    let _blob = client.get_blob(&blob_id).await.map_err(|e| e.to_string())?;

    // Ask user where to save
    let path = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("PNG Image", &["png"])
            .set_file_name("certificate.png")
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;

    let Some(save_path) = path else {
        return Ok("cancelled".to_string());
    };

    let path_str = match save_path {
        tauri_plugin_dialog::FilePath::Path(p) => p.to_string_lossy().to_string(),
        tauri_plugin_dialog::FilePath::Url(u) => u.to_string(),
    };

    // Generate PNG via m0-daemon
    client
        .export_certificate_png(&blob_id, &path_str)
        .await
        .map_err(|e| e.to_string())?;

    Ok(path_str)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_audio_file_wav_stub() {
        let meta = load_audio_file("/tmp/track.wav".into()).await.unwrap();
        assert_eq!(meta.name, "track.wav");
        assert_eq!(meta.format, "WAV");
        assert_eq!(meta.path, "/tmp/track.wav");
        assert_eq!(meta.sample_rate, 48_000);
        assert_eq!(meta.channels, 2);
    }

    #[tokio::test]
    async fn test_load_audio_file_flac_stub() {
        let meta = load_audio_file("/tmp/master.flac".into()).await.unwrap();
        assert_eq!(meta.format, "FLAC");
    }

    #[tokio::test]
    async fn test_load_audio_file_rejects_txt() {
        let err = load_audio_file("/tmp/readme.txt".into()).await.unwrap_err();
        assert!(
            err.contains("ASC:0x04"),
            "Must return ASC 0x04 for .txt: {err}"
        );
    }

    #[tokio::test]
    async fn test_audio_extensions_coverage() {
        for ext in AUDIO_EXTENSIONS {
            let path = format!("/tmp/track.{ext}");
            let result = load_audio_file(path).await;
            assert!(result.is_ok(), "Extension .{ext} must be accepted");
        }
    }
}
