#![allow(clippy::type_complexity)]
//! POST /preview — scout + diverse window → 5 stem WAV files
//! GET  /preview/:id/:stem — serve stem WAV
//! Authority: spatial-mixer-widget-v1_0.md §4.1
//!
//! Phase 8a: enables 5.1 Spatial Audio Mixer widget.
//! Returns 5 mini-stems (15s) for Web Audio API mixing in browser.

use crate::app_state::AppState;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ── PreviewStore ──────────────────────────────────────────────────────────────

/// In-memory store for preview stem WAV bytes.
/// Keyed by preview_id → stem_name → wav_bytes.
#[derive(Clone, Default)]
pub struct PreviewStore {
    inner: Arc<Mutex<HashMap<String, HashMap<String, Vec<u8>>>>>,
}

impl PreviewStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, preview_id: String, stems: HashMap<String, Vec<u8>>) {
        let mut lock = self.inner.lock().unwrap();
        lock.insert(preview_id, stems);
    }

    pub fn get_stem(&self, preview_id: &str, stem: &str) -> Option<Vec<u8>> {
        let lock = self.inner.lock().unwrap();
        lock.get(preview_id).and_then(|s| s.get(stem)).cloned()
    }
}

// ── Request / Response ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub audio_path: String,
    pub preset_id: String,
    pub flavour_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    pub preview_id: String,
    pub duration_s: f32,
    pub sample_rate: u32,
    pub stems: PreviewStemUrls,
    pub scout: ScoutMeta,
}

#[derive(Debug, Serialize)]
pub struct PreviewStemUrls {
    pub voice: String,
    pub drums: String,
    pub bass: String,
    pub harmonics: String,
    pub ambience: String,
}

#[derive(Debug, Serialize)]
pub struct ScoutMeta {
    pub rear_scale: f32,
    pub lfe_scale: f32,
    pub voice_idx: usize,
    pub bass_idx: usize,
    pub harmonics_idx: usize,
    pub ambience_idx: usize,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// POST /preview — decode audio, scout, find diverse 15s window,
/// render 5 stem WAVs, store in PreviewStore, return URLs.
pub async fn create_preview(
    State(state): State<AppState>,
    Json(req): Json<PreviewRequest>,
) -> Json<serde_json::Value> {
    let preview_id = uuid::Uuid::new_v4().to_string();

    state
        .audit
        .write(crate::audit::AuditEntry::new(
            "m0d.preview_started",
            crate::audit::AuditLevel::Audit,
            &format!("preview={} path={}", preview_id, req.audio_path),
        ))
        .ok();

    let preview_id_bg = preview_id.clone();
    let state_bg = state.clone();
    let audio_path = req.audio_path.clone();

    // Run in spawn_blocking — DSP is CPU-intensive
    let result =
        tokio::task::spawn_blocking(move || generate_preview_stems(&audio_path, &req.flavour_id))
            .await;

    match result {
        Err(e) => Json(serde_json::json!({
            "error": format!("spawn_blocking panic: {e}")
        })),
        Ok(Err(e)) => Json(serde_json::json!({ "error": e })),
        Ok(Ok((stems_wav, scout_meta, duration_s, sample_rate))) => {
            state_bg
                .preview_store
                .insert(preview_id_bg.clone(), stems_wav);

            state_bg
                .audit
                .write(crate::audit::AuditEntry::new(
                    "m0d.preview_complete",
                    crate::audit::AuditLevel::Audit,
                    &format!("preview={} duration={:.1}s", preview_id_bg, duration_s),
                ))
                .ok();

            let pid = &preview_id_bg;
            Json(serde_json::json!({
                "previewId":  pid,
                "durationS":  duration_s,
                "sampleRate": sample_rate,
                "stems": {
                    "voice":     format!("/preview/{pid}/voice"),
                    "drums":     format!("/preview/{pid}/drums"),
                    "bass":      format!("/preview/{pid}/bass"),
                    "harmonics": format!("/preview/{pid}/harmonics"),
                    "ambience":  format!("/preview/{pid}/ambience"),
                },
                "scout": {
                    "rearScale":    scout_meta.rear_scale,
                    "lfeScale":     scout_meta.lfe_scale,
                    "voiceIdx":     scout_meta.voice_idx,
                    "bassIdx":      scout_meta.bass_idx,
                    "harmonicsIdx": scout_meta.harmonics_idx,
                    "ambienceIdx":  scout_meta.ambience_idx,
                }
            }))
        }
    }
}

/// GET /preview/:id/:stem — serve stem WAV bytes.
pub async fn get_preview_stem(
    State(state): State<AppState>,
    Path((preview_id, stem)): Path<(String, String)>,
) -> impl IntoResponse {
    let valid_stems = ["voice", "drums", "bass", "harmonics", "ambience"];
    if !valid_stems.contains(&stem.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "application/json")],
            b"{\"error\":\"invalid stem name\"}".to_vec(),
        )
            .into_response();
    }

    match state.preview_store.get_stem(&preview_id, &stem) {
        None => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "application/json")],
            b"{\"error\":\"preview not found\"}".to_vec(),
        )
            .into_response(),
        Some(wav_bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "audio/wav")],
            wav_bytes,
        )
            .into_response(),
    }
}

// ── Core DSP logic ────────────────────────────────────────────────────────────

/// Decode audio, find diverse 15s window, scout, render 5 stem WAVs.
/// Runs in spawn_blocking.
fn generate_preview_stems(
    audio_path: &str,
    _flavour_id: &Option<String>,
) -> Result<(HashMap<String, Vec<u8>>, ScoutMeta, f32, u32), String> {
    use crate::handlers::decode;
    use sp314_dsp::stft::nmf::find_most_diverse_window;
    use sp314_dsp::stft::two_pass::TwoPassEngine;

    // Decode audio
    let payload = decode::decode_smart(audio_path).map_err(|e| format!("Decode error: {e}"))?;
    let pcm = payload.to_stereo_for_telemetry();

    let sample_rate = pcm.sample_rate;

    // Stereo → mono
    let mono: Vec<f32> = pcm
        .left
        .iter()
        .zip(pcm.right.iter())
        .map(|(l, r)| (*l + *r) * 0.5)
        .collect();

    // Find most diverse 15s window
    let (start, end) = find_most_diverse_window(&mono, sample_rate, 15.0);
    let snippet = &mono[start..end];
    let duration_s = snippet.len() as f32 / sample_rate as f32;

    // Scout on snippet
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(snippet, sample_rate, None, None);

    // Render stems for the snippet
    let mut stem_voices: Vec<f32> = Vec::new();
    let mut stem_drums: Vec<f32> = Vec::new();
    let mut stem_bass: Vec<f32> = Vec::new();
    let mut stem_harmonics: Vec<f32> = Vec::new();
    let mut stem_ambience: Vec<f32> = Vec::new();

    engine
        .process_chunks(snippet, &scout, false, |chunk| {
            stem_voices.extend_from_slice(&chunk.voice);
            stem_drums.extend_from_slice(&chunk.drums);
            stem_bass.extend_from_slice(&chunk.bass);
            stem_harmonics.extend_from_slice(&chunk.harmonics);
            stem_ambience.extend_from_slice(&chunk.ambience);
        })
        .map_err(|e| format!("process_chunks error: {e}"))?;

    // Encode each stem as WAV bytes
    let mut stems_wav = HashMap::new();
    stems_wav.insert("voice".into(), encode_wav(&stem_voices, sample_rate)?);
    stems_wav.insert("drums".into(), encode_wav(&stem_drums, sample_rate)?);
    stems_wav.insert("bass".into(), encode_wav(&stem_bass, sample_rate)?);
    stems_wav.insert(
        "harmonics".into(),
        encode_wav(&stem_harmonics, sample_rate)?,
    );
    stems_wav.insert("ambience".into(), encode_wav(&stem_ambience, sample_rate)?);

    let scout_meta = ScoutMeta {
        rear_scale: scout.rear_scale,
        lfe_scale: scout.lfe_scale,
        voice_idx: scout.voice_idx,
        bass_idx: scout.bass_idx,
        harmonics_idx: scout.harmonics_idx,
        ambience_idx: scout.ambience_idx,
    };

    Ok((stems_wav, scout_meta, duration_s, sample_rate))
}

/// Encode f32 mono PCM as WAV bytes (in-memory).
fn encode_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    use hound::{SampleFormat, WavSpec, WavWriter};
    use std::io::Cursor;

    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut buf = Vec::new();
    {
        let cursor = Cursor::new(&mut buf);
        let mut writer =
            WavWriter::new(cursor, spec).map_err(|e| format!("WAV writer error: {e}"))?;
        for &s in samples {
            writer
                .write_sample(s)
                .map_err(|e| format!("WAV write error: {e}"))?;
        }
        writer
            .finalize()
            .map_err(|e| format!("WAV finalize error: {e}"))?;
    }
    Ok(buf)
}
