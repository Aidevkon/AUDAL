//! M0 IPC command stubs — Phase 5.
//! Phase 6 will replace these stubs with real M0 HTTP calls to localhost:7400.
//! Authority: state-machine.md §6.3 · Phase 5 task-decomposition P5-008
//!
//! FORBIDDEN: No blocking audio processing here — Tauri commands must be async.
//! FORBIDDEN: No serde_json::Value crossing the WASM boundary.

use tauri::command;

/// Phase 5 stub: simulates 2 seconds of DSP pipeline execution.
/// Phase 6: calls M0 → sp314-dsp pipeline via HTTP POST localhost:7400/master.
#[command]
pub async fn trigger_mastering(
    audio_path: String,
    preset_id:  String,
) -> Result<String, String> {
    // Phase 5: 2-second simulated processing delay
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    // Phase 6: replace with HTTP call to M0 → golden blob path returned
    let _ = (audio_path, preset_id); // used in Phase 6
    Ok("golden_blob_stub".to_string())
}

/// Phase 5 stub: returns fake audio metadata from path.
/// Phase 6: calls M0 CDN to decode and hash the file.
#[command]
pub async fn load_audio_file(path: String) -> Result<AudioMeta, String> {
    let name = path
        .split('/')
        .last()
        .unwrap_or("unknown")
        .to_string();

    Ok(AudioMeta {
        name,
        format:      "WAV".to_string(),
        sample_rate: 48_000,
        bit_depth:   24,
        duration_s:  180.0,
        channels:    2,
    })
}

/// Audio file metadata returned to frontend after file load.
/// All fields are typed primitives — no serde_json::Value.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioMeta {
    pub name:        String,
    pub format:      String,
    pub sample_rate: u32,
    pub bit_depth:   u8,
    pub duration_s:  f32,
    pub channels:    u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_audio_file_returns_meta() {
        let meta = load_audio_file("/tmp/track.wav".to_string()).await.unwrap();
        assert_eq!(meta.name, "track.wav");
        assert_eq!(meta.sample_rate, 48_000);
        assert_eq!(meta.channels, 2);
    }

    #[tokio::test]
    async fn test_trigger_mastering_returns_stub() {
        let result = trigger_mastering(
            "/tmp/track.wav".to_string(),
            "spotify".to_string(),
        ).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "golden_blob_stub");
    }
}
