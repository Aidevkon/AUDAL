//! Mastering commands — real M0 IPC calls replacing Phase 5 stubs.
//! Authority: Phase 6 task-decomposition P6-004
//! Flow: Cockpit → Tauri → M0 → sp314-dsp → Golden Blob
//!
//! FORBIDDEN: Calling sp314-dsp directly (must go through M0).
//! FORBIDDEN: Audio processing in this module.
//! FORBIDDEN: serde_json::Value in return types.

use tauri::{command, AppHandle, Runtime};
use crate::ipc::m0_client::{GoldenBlobJson, M0Client, MasterRequest};

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
pub async fn open_audio_file<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<AudioMeta>, String> {
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
        None       => return Ok(None),    // User cancelled → stay in current mode
        Some(fp)   => fp,
    };

    // Extract path string
    let path_str = match file_path {
        FilePath::Path(p)   => p.to_string_lossy().to_string(),
        FilePath::Url(u)    => u.to_string(),
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
        "wav"          => "WAV",
        "flac"         => "FLAC",
        "aiff" | "aif" => "AIFF",
        "mp3"          => "MP3",
        _              => "AUDIO",
    };

    let name = std::path::Path::new(&path_str)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Phase 6: metadata stub — Phase 7 reads real headers via hound/symphonia
    Ok(Some(AudioMeta {
        path:        path_str,
        name,
        format:      format.to_string(),
        sample_rate: 48_000,
        bit_depth:   24,
        duration_s:  0.0,    // Phase 7: real decode
        channels:    2,
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
        return Err(format!(
            "ASC:0x04:Unsupported file type '.{ext}'."
        ));
    }

    let format = match ext.as_str() {
        "wav"          => "WAV",
        "flac"         => "FLAC",
        "aiff" | "aif" => "AIFF",
        "mp3"          => "MP3",
        _              => "AUDIO",
    };

    let name = path.split('/').last().unwrap_or("unknown").to_string();

    Ok(AudioMeta {
        path:        path,
        name,
        format:      format.to_string(),
        sample_rate: 48_000,
        bit_depth:   24,
        duration_s:  0.0,
        channels:    2,
    })
}

// ── trigger_mastering ─────────────────────────────────────────────────────────

/// Real mastering: Cockpit → Tauri → M0 → sp314-dsp.
/// Returns blob_id on success; ASC-mapped error string on failure.
#[command]
pub async fn trigger_mastering(
    audio_path: String,
    preset_id:  String,
    flavour_id: String,
    intent_tone: f32,
    intent_dynamics: f32,
    client: tauri::State<'_, M0Client>,
) -> Result<String, String> {
    eprintln!("[trigger_mastering] START path={audio_path} preset={preset_id}");

    // Guard: verify M0 is healthy (ASC 0x05 guard).
    client.health().await
        .map_err(|e| format!("M0 unreachable: {e}"))?;
    eprintln!("[trigger_mastering] health OK — sending to M0...");

    let resp = client
        .trigger_mastering(MasterRequest { 
            audio_path, preset_id, flavour_id, 
            intent_tone, intent_dynamics 
        })
        .await
        .map_err(|e| e.to_string())?;

    eprintln!("[trigger_mastering] M0 returned status={} blob_id={}", resp.status, resp.blob_id);

    if resp.status != "ok" {
        return Err(resp.message.unwrap_or_else(|| "mastering failed".into()));
    }

    eprintln!("[trigger_mastering] DONE blob_id={}", resp.blob_id);
    Ok(resp.blob_id)
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
    pub path:        String,
    /// Display filename.
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
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
        assert!(err.contains("ASC:0x04"), "Must return ASC 0x04 for .txt: {err}");
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
